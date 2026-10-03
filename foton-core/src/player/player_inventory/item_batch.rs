//! Owned menu writes survive dispatch, replay and terminal disposal.

use super::{
    DeferredMenuAction, OpenMenuDispatch, OpenMenuReadSource, player_handlers::menu_top_slot_count,
};
use crate::{
    inventory::{lock::ContainerLockGuard, menu::Menu, slots::MenuSlotReadSource},
    player::Player,
};
use foton_registry::item_stack::ItemStack;

/// One logical top-inventory write. Actual Slot policy is applied by the menu.
pub struct MenuSlotWrite {
    /// Logical position in the menu's top inventory.
    pub index: usize,
    /// Complete owning item value to install.
    pub stack: ItemStack,
}

/// Owns the operation, not merely its item values. `take_writes` must return the
/// same writes exposed by `writes`, once. Admission/lifecycle ownership must
/// remain on this object until drop, including after its values are consumed.
pub trait MenuItemBatch: Send + 'static {
    /// Full replacements remain full when replay targets the then-current menu.
    /// Same-size menu replacement is allowed; this is not instance pinning.
    fn expected_size(&self) -> Option<usize> {
        None
    }
    /// Borrows all writes before any destination mutation.
    fn writes(&self) -> &[MenuSlotWrite];
    /// Moves out those writes once, retaining operation ownership on self.
    fn take_writes(&mut self) -> Vec<MenuSlotWrite>;
}

/// Whether this operation completed now or remains owned by callback dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItemBatchStatus {
    /// Writes and synchronous callbacks have completed.
    Applied,
    /// Existing dispatch owns the operation until replay or terminal disposal.
    Queued,
}

/// Recoverable failures never mean that part of the batch was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItemBatchError {
    /// No writable menu remains.
    Closed,
    /// A logical or physical slot is outside the target.
    InvalidSlot,
    /// Two writes address one logical position.
    DuplicateSlot,
    /// A full replacement no longer fits the current menu.
    SizeMismatch,
    /// A slot cannot provide callback-safe live storage reads.
    UnsupportedLiveRead,
}

struct NativeBatch(Vec<MenuSlotWrite>);
impl MenuItemBatch for NativeBatch {
    fn writes(&self) -> &[MenuSlotWrite] {
        &self.0
    }
    fn take_writes(&mut self) -> Vec<MenuSlotWrite> {
        std::mem::take(&mut self.0)
    }
}

pub(super) fn single(index: usize, stack: ItemStack) -> Box<dyn MenuItemBatch> {
    Box::new(NativeBatch(vec![MenuSlotWrite { index, stack }]))
}

fn validate(writes: &[MenuSlotWrite], size: usize) -> Result<(), MenuItemBatchError> {
    let mut seen = std::collections::BTreeSet::new();
    for write in writes {
        if write.index >= size {
            return Err(MenuItemBatchError::InvalidSlot);
        }
        if !seen.insert(write.index) {
            return Err(MenuItemBatchError::DuplicateSlot);
        }
    }
    Ok(())
}

fn validate_batch(batch: &dyn MenuItemBatch, size: usize) -> Result<(), MenuItemBatchError> {
    if batch
        .expected_size()
        .is_some_and(|expected| expected != size || batch.writes().len() != size)
    {
        return Err(MenuItemBatchError::SizeMismatch);
    }
    validate(batch.writes(), size)
}

fn sources(menu: &Menu, size: usize) -> Result<Vec<MenuSlotReadSource>, MenuItemBatchError> {
    (0..size)
        .map(|index| {
            menu.behavior()
                .slots()
                .get(index)
                .and_then(|slot| slot.live_read_source())
                .ok_or(MenuItemBatchError::UnsupportedLiveRead)
        })
        .collect()
}

fn apply(menu: &Menu, batch: &mut dyn MenuItemBatch) -> Result<(), MenuItemBatchError> {
    let size = menu_top_slot_count(menu).ok_or(MenuItemBatchError::Closed)?;
    validate_batch(batch, size)?;
    let reads = sources(menu, size)?;
    let writes = batch.take_writes();
    validate(&writes, size)?;
    // Metadata validation happens before loot unpacking can invoke world callbacks.
    for write in &writes {
        let source = &reads[write.index];
        if source.index >= source.container.container_size() {
            return Err(MenuItemBatchError::InvalidSlot);
        }
    }
    let mut guard: ContainerLockGuard = menu.behavior().lock_all_containers();
    for write in &writes {
        let source = &reads[write.index];
        if guard
            .get(source.container.container_id())
            .is_none_or(|container| source.index >= container.get_container_size())
        {
            return Err(MenuItemBatchError::InvalidSlot);
        }
    }
    for write in writes {
        menu.behavior().slots()[write.index].set_item(&mut guard, write.stack);
    }
    Ok(())
}

impl Player {
    /// Prevalidates the complete logical batch before enqueue/write, then releases
    /// the menu mutex before loot and Slot callbacks. Notifications remain ordered
    /// and synchronous; this is invalid-input atomicity, not observer rollback.
    pub fn set_open_container_items(
        &self,
        mut batch: Box<dyn MenuItemBatch>,
    ) -> Result<MenuItemBatchStatus, MenuItemBatchError> {
        let menu = {
            let mut state = self.open_menu.lock();
            if state.terminal_removal.is_some() {
                return Err(MenuItemBatchError::Closed);
            }
            if let Some(dispatch) = state.dispatch.as_mut() {
                validate_batch(batch.as_ref(), dispatch.top_slot_count)?;
                if !dispatch.live_reads_supported {
                    return Err(MenuItemBatchError::UnsupportedLiveRead);
                }
                dispatch.actions.push(DeferredMenuAction::SetItems(batch));
                return Ok(MenuItemBatchStatus::Queued);
            }
            let menu = state.menu.as_ref().ok_or(MenuItemBatchError::Closed)?;
            let size = menu_top_slot_count(menu).ok_or(MenuItemBatchError::Closed)?;
            validate_batch(batch.as_ref(), size)?;
            let reads = sources(menu, size)?;
            let menu = state.menu.take().ok_or(MenuItemBatchError::Closed)?;
            state.dispatch = Some(OpenMenuDispatch {
                instance: menu.behavior().instance_id(),
                overrides_player_slots: menu.overrides_player_slots(),
                top_slot_count: size,
                menu_type: menu.menu_type().map(|kind| kind.key.to_string()),
                reads: OpenMenuReadSource::Live(reads),
                live_reads_supported: true,
                actions: Vec::new(),
            });
            menu
        };
        let result = apply(&menu, batch.as_mut());
        // Restore or terminal-clean even when validation fails after detachment.
        // The original owner survives nested replay and the final broadcast.
        self.finish_open_menu_callback(menu);
        if result.is_ok() {
            self.broadcast_inventory_changes();
        }
        drop(batch);
        result.map(|()| MenuItemBatchStatus::Applied)
    }
}
