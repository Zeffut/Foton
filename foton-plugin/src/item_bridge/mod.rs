//! Canonical, process-local item snapshots used by the Java bridge.

pub(crate) mod block_inventory;
pub(crate) mod block_item;
mod edits;
pub(crate) mod entity;
mod error;
pub(crate) mod inventory;
pub(crate) mod legacy;
pub(crate) mod menu;
pub(crate) mod merchant;
pub(crate) mod meta_kind;
pub(crate) mod mounts;
pub(crate) mod mutation;
mod preflight;
pub(crate) mod public_components;
pub(crate) mod queries;
mod snapshot;
pub(crate) mod transfer;
pub(crate) mod transitions;

use std::num::NonZeroUsize;
use std::sync::OnceLock;

static STORE: OnceLock<SnapshotStore> = OnceLock::new();

pub(crate) fn initialize(limit: NonZeroUsize) {
    STORE.get_or_init(|| SnapshotStore::new(limit));
}

pub(crate) fn store() -> Result<&'static SnapshotStore, ItemBridgeError> {
    STORE.get().ok_or(ItemBridgeError::Closed)
}

pub(crate) fn require_registry() -> Result<(), ItemBridgeError> {
    foton_registry::REGISTRY
        .get()
        .ok_or(ItemBridgeError::RegistryNotReady)?;
    Ok(())
}

pub(crate) use error::ItemBridgeError;
pub(crate) use snapshot::{LeaseKey, SnapshotStore};

#[cfg(test)]
mod test_world;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod jvm_tests;
