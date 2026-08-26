//! Entity equipment access and owned storage.

use std::mem;

use steel_registry::item_stack::ItemStack;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::inventory::container::Container;

pub use steel_registry::equipment::{EquipmentSlot, EquipmentSlotType};

/// Equipment access shared by player inventories and owned entity storage.
///
/// Equipment is also a [`Container`], which is what lets a menu slot sit on a
/// worn item. Vanilla reaches the same place from the other side, wrapping the
/// slot in the `ContainerSingleItem` view of `Mob.createEquipmentSlotContainer`;
/// a Steel [`Container`] hands out `&[ItemStack]`, so the storage itself has to
/// be the container and [`Self::container_index`] says where in it a slot sits.
pub trait EntityEquipment: Container {
    /// The container index backing `slot`.
    fn container_index(&self, slot: EquipmentSlot) -> usize;

    /// Gets a reference to the item in a slot.
    fn get_ref(&self, slot: EquipmentSlot) -> &ItemStack;

    /// Gets a mutable reference to the item in a slot.
    fn get_mut(&mut self, slot: EquipmentSlot) -> &mut ItemStack;

    /// Sets the item in a slot, returning the old item.
    fn set(&mut self, slot: EquipmentSlot, stack: ItemStack) -> ItemStack;

    /// Takes the item from a slot, leaving an empty stack in its place.
    fn take(&mut self, slot: EquipmentSlot) -> ItemStack;

    /// Clears all equipment slots.
    fn clear(&mut self);

    /// Returns non-empty equipment slots for initial spawn synchronization.
    fn non_empty_items(&self) -> Vec<(EquipmentSlot, ItemStack)> {
        EquipmentSlot::ALL
            .into_iter()
            .filter_map(|slot| {
                let item = self.get_ref(slot);
                (!item.is_empty()).then(|| (slot, item.clone()))
            })
            .collect()
    }
}

/// Owned equipment storage used by non-player living entities.
pub struct OwnedEntityEquipment {
    slots: [ItemStack; 8],
}

impl Default for OwnedEntityEquipment {
    fn default() -> Self {
        Self::new()
    }
}

impl OwnedEntityEquipment {
    /// Creates a new empty equipment storage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: [
                ItemStack::empty(),
                ItemStack::empty(),
                ItemStack::empty(),
                ItemStack::empty(),
                ItemStack::empty(),
                ItemStack::empty(),
                ItemStack::empty(),
                ItemStack::empty(),
            ],
        }
    }
}

// SAFETY: This Steel-owned key uniquely identifies `OwnedEntityEquipment`.
unsafe impl DowncastType for OwnedEntityEquipment {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/entity_equipment");
}

impl Container for OwnedEntityEquipment {
    fn items(&self) -> &[ItemStack] {
        &self.slots
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.slots
    }

    /// Vanilla parity: the empty `setChanged` of the `ContainerSingleItem` that
    /// `Mob.createEquipmentSlotContainer` returns. Equipment is picked up by
    /// `LivingEntity.detectEquipmentUpdates` on the next tick, so a write has
    /// nothing to announce here.
    fn set_changed(&mut self) {}
}

impl EntityEquipment for OwnedEntityEquipment {
    fn container_index(&self, slot: EquipmentSlot) -> usize {
        slot.index()
    }

    fn get_ref(&self, slot: EquipmentSlot) -> &ItemStack {
        &self.slots[slot.index()]
    }

    fn get_mut(&mut self, slot: EquipmentSlot) -> &mut ItemStack {
        &mut self.slots[slot.index()]
    }

    fn set(&mut self, slot: EquipmentSlot, stack: ItemStack) -> ItemStack {
        mem::replace(&mut self.slots[slot.index()], stack)
    }

    fn take(&mut self, slot: EquipmentSlot) -> ItemStack {
        mem::take(&mut self.slots[slot.index()])
    }

    fn clear(&mut self) {
        for slot in EquipmentSlot::ALL {
            self.slots[slot.index()] = ItemStack::empty();
        }
    }
}
