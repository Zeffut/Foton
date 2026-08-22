//! Furnace result slot.
//!
//! Vanilla parity: `FurnaceResultSlot`. Items can only be taken out, and taking
//! them pays out the experience the furnace banked while cooking.

use steel_registry::item_stack::ItemStack;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::block_entity::SharedBlockEntity;
use crate::block_entity::entities::FurnaceBlockEntity;
use crate::entity::Entity as _;
use crate::entity::entities::ExperienceOrbEntity;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::slots::normal_slot::NormalSlot;
use crate::inventory::slots::slot::{Slot, SlotStorage};
use crate::player::Player;

/// The output slot of a furnace.
///
/// Placement is always refused, and collecting the result releases the stored
/// experience as orbs at the player's feet.
pub struct FurnaceResultSlot {
    inner: NormalSlot,
    block_entity: SharedBlockEntity,
}

// SAFETY: This key uniquely identifies Steel's `FurnaceResultSlot`.
unsafe impl DowncastType for FurnaceResultSlot {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:slot/furnace_result");
}

impl FurnaceResultSlot {
    /// Creates the result slot backed by `container` at `index`.
    #[must_use]
    pub fn new(
        container: impl Into<ContainerRef>,
        index: usize,
        block_entity: SharedBlockEntity,
    ) -> Self {
        Self {
            inner: NormalSlot::new(container, index),
            block_entity,
        }
    }
}

impl Slot for FurnaceResultSlot {
    fn storage(&self) -> &SlotStorage {
        self.inner.storage()
    }

    fn get_item<'a>(&self, guard: &'a ContainerLockGuard) -> &'a ItemStack {
        self.inner.get_item(guard)
    }

    fn get_item_mut<'a>(&self, guard: &'a mut ContainerLockGuard) -> &'a mut ItemStack {
        self.inner.get_item_mut(guard)
    }

    fn set_item(&self, guard: &mut ContainerLockGuard, stack: ItemStack) {
        self.inner.set_item(guard, stack);
    }

    fn remove(&self, guard: &mut ContainerLockGuard, amount: i32) -> ItemStack {
        self.inner.remove(guard, amount)
    }

    fn set_changed(&self, guard: &mut ContainerLockGuard) {
        self.inner.set_changed(guard);
    }

    fn get_container_slot(&self) -> usize {
        self.inner.get_container_slot()
    }

    fn get_max_stack_size(&self, guard: &ContainerLockGuard) -> i32 {
        self.inner.get_max_stack_size(guard)
    }

    /// Vanilla parity: `FurnaceResultSlot.mayPlace` always refuses.
    fn may_place(&self, _stack: &ItemStack) -> bool {
        false
    }

    /// Pays out the banked experience.
    ///
    /// Vanilla parity: `FurnaceResultSlot.onTake` awards the recipes used since
    /// the last collection, which is why smelting a full stack and collecting once
    /// yields the same experience as collecting after every item.
    fn on_take(
        &self,
        guard: &mut ContainerLockGuard,
        _stack: &ItemStack,
        player: &Player,
    ) -> Option<ItemStack> {
        self.set_changed(guard);

        let furnace = self.block_entity.downcast_ref::<FurnaceBlockEntity>()?;
        let experience = furnace.take_earned_experience();
        if experience <= 0.0 {
            return None;
        }

        // Vanilla splits the float into whole orbs and rolls for the remainder.
        let whole = experience.floor();
        let mut amount = whole as i32;
        if rand::random::<f32>() < experience - whole {
            amount += 1;
        }
        if amount <= 0 {
            return None;
        }

        if let Some(world) = player.level() {
            ExperienceOrbEntity::award(&world, player.position(), amount);
        }
        None
    }
}
