use crate::{
    inventory::{
        lock::{ContainerLockGuard, ContainerRef},
        slots::{NormalSlot, Slot, SlotStorage},
    },
    player::Player,
};
use foton_registry::{
    enchantment_effect::EnchantmentEffectComponent, equipment::EquipmentSlot, item_stack::ItemStack,
};
use foton_utils::{DowncastType, DowncastTypeKey};

/// A [`NormalSlot`] that only accepts items equippable in its equipment slot,
/// caps at one item, and respects the prevent-armor-change enchantment effect.
pub struct ArmorSlot {
    base: NormalSlot,
    slot: EquipmentSlot,
}

// SAFETY: This key uniquely identifies Foton's `ArmorSlot`.
unsafe impl DowncastType for ArmorSlot {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:slot/armor");
}

impl ArmorSlot {
    /// Creates a new armor slot.
    pub fn new(container: impl Into<ContainerRef>, index: usize, slot: EquipmentSlot) -> Self {
        Self {
            base: NormalSlot::new(container, index),
            slot,
        }
    }

    /// Returns the equipment slot this armor slot accepts.
    #[must_use]
    pub const fn equipment_slot(&self) -> EquipmentSlot {
        self.slot
    }

    /// Returns a reference to the container.
    #[must_use]
    pub fn container_ref(&self) -> ContainerRef {
        self.base.container_ref()
    }
}

impl Slot for ArmorSlot {
    fn storage(&self) -> &SlotStorage {
        self.base.storage()
    }

    fn get_item<'a>(&self, guard: &'a ContainerLockGuard) -> &'a ItemStack {
        self.base.get_item(guard)
    }

    fn get_item_mut<'a>(&self, guard: &'a mut ContainerLockGuard) -> &'a mut ItemStack {
        self.base.get_item_mut(guard)
    }

    fn set_item(&self, guard: &mut ContainerLockGuard, stack: ItemStack) {
        self.base.set_item(guard, stack);
    }

    /// MISSING FOUNDATION: vanilla's `ArmorSlot.setByPlayer` opens with
    /// `this.owner.onEquipItem(slot, oldStack, newStack)`, which is where the
    /// clunk of putting a helmet on comes from. Foton's `on_equip_item` exists
    /// and the mount's own equipment slots already call it -- see
    /// `MountEquipmentSlot::set_by_player`, which holds a `Weak` to its mount.
    ///
    /// This slot has no such handle. `Slot::set_by_player` takes no player (as
    /// vanilla's does not), the menu is built inside `Player::new` before any
    /// `Arc<Player>` exists to weaken, and `Slot::safe_insert` reaches this
    /// with no player at all. Wiring it needs an owner reachable at
    /// menu-construction time; until then armour equipped from the inventory
    /// screen goes on in silence, and emits neither `EQUIP` nor `UNEQUIP`.
    fn set_by_player(
        &self,
        guard: &mut ContainerLockGuard,
        stack: ItemStack,
        previous: &ItemStack,
    ) {
        let _ = previous;
        self.set_item(guard, stack);
    }

    fn may_place(&self, stack: &ItemStack) -> bool {
        stack.is_equippable_in_slot(self.slot)
    }

    fn may_pickup(&self, guard: &ContainerLockGuard, player: &Player) -> bool {
        let item = self.get_item(guard);
        if !item.is_empty()
            && !player.has_infinite_materials()
            && item.has_enchantment_effect(EnchantmentEffectComponent::PreventArmorChange)
        {
            return false;
        }
        true
    }

    fn get_max_stack_size(&self, _guard: &ContainerLockGuard) -> i32 {
        1
    }

    fn set_changed(&self, guard: &mut ContainerLockGuard) {
        self.base.set_changed(guard);
    }

    fn get_container_slot(&self) -> usize {
        self.base.get_container_slot()
    }
}
