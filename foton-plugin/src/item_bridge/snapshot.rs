use std::num::NonZeroUsize;
use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_utils::locks::SyncMutex;
use rustc_hash::FxHashMap;
use uuid::Uuid;

use super::{ItemBridgeError, preflight};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LeaseKey {
    pub epoch: Uuid,
    pub id: Uuid,
}

struct Admission {
    limit: NonZeroUsize,
    state: SyncMutex<AdmissionState>,
    terminal: tokio::sync::Notify,
}

struct AdmissionState {
    open: bool,
    retained: usize,
    candidates: usize,
}

#[derive(Clone, Copy)]
enum PermitKind {
    Retained,
    Candidate,
}

/// A permit lives with the value, not its lookup-table entry.
struct Permit {
    admission: Arc<Admission>,
    kind: PermitKind,
}

impl Admission {
    fn reserve(self: &Arc<Self>, kind: PermitKind) -> Result<Permit, ItemBridgeError> {
        let mut state = self.state.lock();
        if !state.open {
            return Err(ItemBridgeError::Closed);
        }
        let count = match kind {
            PermitKind::Retained => &mut state.retained,
            PermitKind::Candidate => &mut state.candidates,
        };
        if *count == self.limit.get() {
            return Err(ItemBridgeError::Capacity);
        }
        *count += 1;
        Ok(Permit {
            admission: Arc::clone(self),
            kind,
        })
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.admission.state.lock();
        match self.kind {
            PermitKind::Retained => state.retained -= 1,
            PermitKind::Candidate => state.candidates -= 1,
        }
        let terminal = !state.open && state.candidates == 0;
        drop(state);
        if terminal {
            self.admission.terminal.notify_waiters();
        }
    }
}

pub(crate) struct Snapshot {
    stack: ItemStack,
    _permit: Permit,
}

impl Snapshot {
    pub(crate) const fn stack(&self) -> &ItemStack {
        &self.stack
    }
}

/// Detached candidate; dropping it releases in-flight admission.
pub(crate) struct Candidate {
    pub stack: ItemStack,
    _permit: Permit,
}

impl Candidate {
    /// The operation permit survives all destination writes and reentrant callbacks.
    pub(crate) fn commit<R>(self, commit: impl FnOnce(ItemStack) -> R) -> R {
        let Self {
            stack,
            _permit: permit,
        } = self;
        let result = commit(stack);
        drop(permit);
        result
    }
}

/// Core can consume item values without releasing deferred operation ownership.
struct MenuCandidates {
    writes: Vec<foton_core::player::player_inventory::MenuSlotWrite>,
    expected_size: Option<usize>,
    _permits: Vec<Permit>,
}

impl foton_core::player::player_inventory::MenuItemBatch for MenuCandidates {
    fn writes(&self) -> &[foton_core::player::player_inventory::MenuSlotWrite] {
        &self.writes
    }
    fn expected_size(&self) -> Option<usize> {
        self.expected_size
    }
    fn take_writes(&mut self) -> Vec<foton_core::player::player_inventory::MenuSlotWrite> {
        std::mem::take(&mut self.writes)
    }
}

pub(super) fn menu_batch(
    candidates: Vec<(usize, Candidate)>,
    expected_size: Option<usize>,
) -> Result<Box<dyn foton_core::player::player_inventory::MenuItemBatch>, ItemBridgeError> {
    let mut writes = Vec::with_capacity(candidates.len());
    let mut permits = Vec::with_capacity(candidates.len() + 1);
    // Even a zero-slot operation must respect terminal admission.
    if candidates.is_empty() {
        permits.push(super::store()?.admission.reserve(PermitKind::Candidate)?);
    }
    for (index, Candidate { stack, _permit }) in candidates {
        writes.push(foton_core::player::player_inventory::MenuSlotWrite { index, stack });
        permits.push(_permit);
    }
    Ok(Box::new(MenuCandidates {
        writes,
        expected_size,
        _permits: permits,
    }))
}

pub(crate) struct SnapshotStore {
    epoch: Uuid,
    admission: Arc<Admission>,
    entries: SyncMutex<FxHashMap<Uuid, Arc<Snapshot>>>,
}

/// Owns an unpublished capture until Java's owning DTO was constructed.
pub(crate) struct Capture<'a> {
    store: &'a SnapshotStore,
    snapshot: Arc<Snapshot>,
    key: LeaseKey,
    published: bool,
    _building: Permit,
}

impl Capture<'_> {
    pub(crate) fn stack(&self) -> &ItemStack {
        self.snapshot.stack()
    }
    pub(crate) const fn key(&self) -> LeaseKey {
        self.key
    }

    pub(crate) fn publish(mut self) -> LeaseKey {
        self.published = true;
        self.key
    }
}

impl Drop for Capture<'_> {
    fn drop(&mut self) {
        if !self.published {
            self.store.release(self.key);
        }
    }
}

impl SnapshotStore {
    pub(crate) fn new(limit: NonZeroUsize) -> Self {
        Self {
            epoch: Uuid::new_v4(),
            admission: Arc::new(Admission {
                limit,
                terminal: tokio::sync::Notify::new(),
                state: SyncMutex::new(AdmissionState {
                    open: true,
                    retained: 0,
                    candidates: 0,
                }),
            }),
            entries: SyncMutex::new(FxHashMap::default()),
        }
    }

    pub(crate) fn capture(&self, stack: &ItemStack) -> Result<Capture<'_>, ItemBridgeError> {
        super::require_registry()?;
        // Building an outgoing semantic clone is also an in-flight materialization.
        let building = self.admission.reserve(PermitKind::Candidate)?;
        let permit = self.admission.reserve(PermitKind::Retained)?;
        preflight::check(stack)?;
        // A semantic snapshot deliberately owns an independent component graph.
        self.insert_snapshot(stack.clone(), permit, building)
    }

    /// A materialized candidate already owns its component graph. Publication
    /// transfers that ownership and the existing operation permit without cloning.
    pub(crate) fn capture_candidate(
        &self,
        candidate: Candidate,
    ) -> Result<Capture<'_>, ItemBridgeError> {
        let retained = self.admission.reserve(PermitKind::Retained)?;
        let Candidate {
            stack,
            _permit: building,
        } = candidate;
        self.insert_snapshot(stack, retained, building)
    }

    fn insert_snapshot(
        &self,
        stack: ItemStack,
        permit: Permit,
        building: Permit,
    ) -> Result<Capture<'_>, ItemBridgeError> {
        let snapshot = Arc::new(Snapshot {
            stack,
            _permit: permit,
        });
        let mut entries = self.entries.lock();
        if !self.admission.state.lock().open {
            return Err(ItemBridgeError::Closed);
        }
        let id = loop {
            let id = Uuid::new_v4();
            if !entries.contains_key(&id) {
                break id;
            }
        };
        entries.insert(id, Arc::clone(&snapshot));
        Ok(Capture {
            store: self,
            snapshot,
            key: LeaseKey {
                epoch: self.epoch,
                id,
            },
            published: false,
            _building: building,
        })
    }

    pub(crate) fn lookup(&self, key: LeaseKey) -> Result<Arc<Snapshot>, ItemBridgeError> {
        if key.epoch != self.epoch {
            return Err(ItemBridgeError::StaleLease);
        }
        self.entries
            .lock()
            .get(&key.id)
            .map(Arc::clone)
            .ok_or(ItemBridgeError::StaleLease)
    }

    pub(crate) fn materialize(&self, key: LeaseKey) -> Result<Candidate, ItemBridgeError> {
        let permit = self.admission.reserve(PermitKind::Candidate)?;
        let snapshot = self.lookup(key)?;
        Ok(Candidate {
            stack: snapshot.stack.clone(),
            _permit: permit,
        })
    }

    #[cfg(test)]
    pub(crate) fn candidate(&self, stack: ItemStack) -> Result<Candidate, ItemBridgeError> {
        self.stage(|| Ok(stack))
    }

    pub(crate) fn stage(
        &self,
        build: impl FnOnce() -> Result<ItemStack, ItemBridgeError>,
    ) -> Result<Candidate, ItemBridgeError> {
        super::require_registry()?;
        let permit = self.admission.reserve(PermitKind::Candidate)?;
        let stack = build()?;
        preflight::check(&stack)?;
        Ok(Candidate {
            stack,
            _permit: permit,
        })
    }

    pub(crate) fn release(&self, key: LeaseKey) {
        if key.epoch == self.epoch {
            // Drop the possibly final value after unlocking the lookup table.
            let removed = self.entries.lock().remove(&key.id);
            drop(removed);
        }
    }

    /// Retires admission after Java invocation drain. Already admitted operations
    /// keep terminal quiescence pending until their final callback/commit returns.
    pub(crate) fn close(&self) {
        let removed = {
            let mut entries = self.entries.lock();
            self.admission.state.lock().open = false;
            std::mem::take(&mut *entries)
        };
        drop(removed);
        self.admission.terminal.notify_waiters();
    }

    pub(crate) fn is_terminal(&self) -> bool {
        let state = self.admission.state.lock();
        !state.open && state.candidates == 0
    }

    /// Register before checking so close/final-drop cannot race the await.
    /// This runs on the outer shutdown coordinator, never a game tick or JNI callback.
    pub(crate) async fn drained(&self) {
        loop {
            let notified = self.admission.terminal.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_terminal() {
                return;
            }
            notified.await;
        }
    }
}
