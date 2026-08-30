use crate::{
    entity::WeakEntity,
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
    /// Who wears what lands here.
    ///
    /// Vanilla parity: the `LivingEntity owner` field of `ArmorSlot`. It is
    /// what `setByPlayer` needs to make a sound.
    owner: WeakEntity,
}

// SAFETY: This key uniquely identifies Foton's `ArmorSlot`.
unsafe impl DowncastType for ArmorSlot {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:slot/armor");
}

impl ArmorSlot {
    /// Creates a new armor slot worn by `owner`.
    pub fn new(
        container: impl Into<ContainerRef>,
        index: usize,
        slot: EquipmentSlot,
        owner: WeakEntity,
    ) -> Self {
        Self {
            base: NormalSlot::new(container, index),
            slot,
            owner,
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

    /// Vanilla parity: `ArmorSlot.setByPlayer`, which opens with
    /// `this.owner.onEquipItem(slot, oldStack, newStack)`. That call is where
    /// the clunk of putting a helmet on comes from, and it is what emits the
    /// `EQUIP`/`UNEQUIP` game event.
    ///
    /// The owner arrives through the menu: the player is built inside an
    /// `Arc::new_cyclic`, so a `Weak` to it exists while its inventory menu is
    /// still being assembled.
    fn set_by_player(
        &self,
        guard: &mut ContainerLockGuard,
        stack: ItemStack,
        previous: &ItemStack,
    ) {
        if let Some(owner) = self.owner.upgrade()
            && let Some(living) = owner.as_living_entity()
        {
            let slot = self.slot;
            let equipped = stack.clone();
            guard.run_unlocked(|| {
                living.on_equip_item(slot, previous, &equipped);
            });
        }
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
