//! The data components a block entity carries between block and item.
//!
//! Vanilla parity: the `components` field of `BlockEntity` together with
//! `applyImplicitComponents`/`collectImplicitComponents`.
//!
//! A block entity holds two kinds of component. The *implicit* ones are the
//! ones it already stores as fields -- a banner's patterns, a hive's bees, a
//! container's name -- and it rebuilds them on demand. Everything else an item
//! put on it is kept verbatim in the stored map so the block can hand it back
//! when it becomes an item again.

use std::cell::RefCell;

use steel_registry::data_components::{
    Component, DataComponentMap, DataComponentType,
    vanilla_components::{BLOCK_ENTITY_DATA, BLOCK_STATE},
};
use steel_registry::item_stack::ItemStack;
use steel_utils::{DowncastType, Identifier};

/// The merged item view an `apply_implicit_components` override reads from.
///
/// Vanilla parity: the anonymous `DataComponentGetter` built inside
/// `BlockEntity.applyComponents`. It records every component type an override
/// asks for, because those are exactly the ones the block entity has taken
/// responsibility for; the rest of the item's patch is stored verbatim.
pub struct ImplicitComponentInput<'a> {
    stack: &'a ItemStack,
    /// The types an override has asked for. Only ever touched by the single
    /// thread running the override, which is why a `RefCell` is enough.
    read: RefCell<Vec<Identifier>>,
}

impl<'a> ImplicitComponentInput<'a> {
    /// Wraps the item whose components are being applied.
    #[must_use]
    pub const fn new(stack: &'a ItemStack) -> Self {
        Self {
            stack,
            read: RefCell::new(Vec::new()),
        }
    }

    /// Vanilla parity: `DataComponentGetter.get`.
    #[must_use]
    pub fn get<T: Component + DowncastType + Clone>(
        &self,
        component: DataComponentType<T>,
    ) -> Option<T> {
        self.read.borrow_mut().push(component.key().clone());
        self.stack.get(component).cloned()
    }

    /// Vanilla parity: `DataComponentGetter.getOrDefault`.
    #[must_use]
    pub fn get_or_default<T: Component + DowncastType + Clone>(
        &self,
        component: DataComponentType<T>,
        default: T,
    ) -> T {
        self.get(component).unwrap_or(default)
    }

    /// The stored half of `BlockEntity.applyComponents`: the item's patch with
    /// every type the overrides claimed forgotten, keeping only what it adds.
    ///
    /// `BLOCK_ENTITY_DATA` and `BLOCK_STATE` are claimed whether an override
    /// asked for them or not, exactly as vanilla seeds its implicit set.
    pub(super) fn leftover_components(self) -> DataComponentMap {
        let mut patch = self.stack.patch().clone();
        patch.clear_key(BLOCK_ENTITY_DATA.key());
        patch.clear_key(BLOCK_STATE.key());
        for key in self.read.into_inner() {
            patch.clear_key(&key);
        }
        patch.added()
    }
}
