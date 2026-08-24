//! Dispenser and dropper block entity.
//!
//! Vanilla parity: `DispenserBlockEntity` and `DropperBlockEntity`, which differ
//! only in their block-entity type; the nine slots and the random-slot pick are
//! shared.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::block_entity_type::BlockEntityType;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase, ContainerLoot};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Slots in a dispenser or dropper.
///
/// Vanilla parity: `DispenserBlockEntity.CONTAINER_SIZE`.
pub const DISPENSER_SLOTS: usize = 9;

/// Dispenser and dropper block entity.
pub struct DispenserBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<DispenserContainer>>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `RandomizableContainer` half of a dispenser, which
    /// is how a jungle temple's trap arrives loaded.
    loot: Arc<ContainerLoot>,
}

/// The nine slots of a dispenser or dropper.
pub struct DispenserContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `DispenserBlockEntity`.
unsafe impl DowncastType for DispenserBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/dispenser");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently
// lockable inventory data used by a dispenser or dropper block entity.
unsafe impl DowncastType for DispenserContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/dispenser");
}

impl DispenserBlockEntity {
    /// Creates a dispenser block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::of_type(&vanilla_block_entity_types::DISPENSER, level, pos, state)
    }

    /// Creates a dropper block entity.
    ///
    /// Vanilla parity: `DropperBlockEntity`, which subclasses the dispenser for
    /// nothing but its own block-entity type and container name.
    #[must_use]
    pub fn new_dropper(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::of_type(&vanilla_block_entity_types::DROPPER, level, pos, state)
    }

    fn of_type(
        block_entity_type: &'static BlockEntityType,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        let base = Arc::new(BlockEntityBase::new(block_entity_type, level, pos, state));
        let container = Arc::new(SyncMutex::new(DispenserContainer {
            items: vec![ItemStack::empty(); DISPENSER_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        let loot = Arc::new(ContainerLoot::new());
        Self {
            container_ref: ContainerRef::owned_by_randomizable_block_entity(
                shared_container,
                Arc::clone(&base),
                Arc::clone(&loot),
            ),
            base,
            container,
            loot,
        }
    }

    /// Picks one non-empty slot at random, or `None` when everything is empty.
    ///
    /// Vanilla parity: `DispenserBlockEntity.getRandomSlot`, which is reservoir
    /// sampling: every filled slot is equally likely, in one pass. It opens
    /// with `unpackLootTable(null)`, so a generated trap dispenser loads itself
    /// the first time it fires.
    #[must_use]
    pub fn get_random_slot(&self) -> Option<usize> {
        self.container_ref.unpack_loot_table(None);
        let container = self.container.lock();
        let mut chosen = None;
        let mut seen = 0;
        for (slot, item) in container.items.iter().enumerate() {
            if item.is_empty() {
                continue;
            }
            seen += 1;
            if rand::random_range(0..seen) == 0 {
                chosen = Some(slot);
            }
        }
        chosen
    }

    /// Returns a copy of the stack in `slot`.
    #[must_use]
    pub fn get_item(&self, slot: usize) -> ItemStack {
        self.container_ref.unpack_loot_table(None);
        let container = self.container.lock();
        container
            .items
            .get(slot)
            .map_or_else(ItemStack::empty, |item| item.copy_with_count(item.count()))
    }

    /// Replaces the stack in `slot`.
    pub fn set_item(&self, slot: usize, stack: ItemStack) {
        self.container_ref.unpack_loot_table(None);
        self.container.lock().set_item(slot, stack);
        self.set_changed();
    }

    /// Puts `stack` back into the first slot that will take it, returning what
    /// did not fit.
    ///
    /// Vanilla parity: `Container.insertItem`, used by the dispense behaviors
    /// that hand back a remainder such as an empty bucket.
    #[must_use = "the remainder has to be thrown or it is destroyed"]
    pub fn insert_item(&self, mut stack: ItemStack) -> ItemStack {
        self.container_ref.unpack_loot_table(None);
        {
            let mut container = self.container.lock();
            for slot in 0..DISPENSER_SLOTS {
                if stack.is_empty() {
                    break;
                }
                let current = container.items[slot].copy_with_count(container.items[slot].count());
                if current.is_empty() {
                    container.set_item(slot, mem::take(&mut stack));
                } else if ItemStack::is_same_item_same_components(&current, &stack) {
                    let space = current.max_stack_size() - current.count();
                    let moved = stack.count().min(space);
                    if moved > 0 {
                        stack.shrink(moved);
                        container.items[slot].grow(moved);
                    }
                }
            }
        }
        self.set_changed();
        stack
    }

    /// Returns the independently lockable container behind this block entity.
    #[must_use]
    pub fn container_ref(&self) -> ContainerRef {
        self.container_ref.clone()
    }
}

impl BlockEntity for DispenserBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        self.container_ref.unpack_loot_table(None);
        let items = {
            let mut container = self.container.lock();
            mem::replace(
                &mut container.items,
                vec![ItemStack::empty(); DISPENSER_SLOTS],
            )
        };
        let Some(world) = self.get_level() else {
            return;
        };
        for item in items {
            world.drop_item_stack(pos, item);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        // Vanilla parity: a dispenser stores either a loot table or its items.
        let packed = self.loot.try_load_loot_table(&nbt_view);
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());
        if packed {
            return;
        }

        if let Some(items_list) = nbt_view.list("Items")
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < DISPENSER_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if self.loot.try_save_loot_table(nbt) {
            return;
        }
        let container = self.container.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in container.items.iter().enumerate() {
            if !item.is_empty()
                && let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag()
            {
                item_nbt.insert("Slot", slot as i8);
                items.push(item_nbt);
            }
        }
        nbt.insert("Items", NbtList::Compound(items));
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for DispenserContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        DISPENSER_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot >= DISPENSER_SLOTS {
            return;
        }
        let max_stack_size = self.get_max_stack_size_for_item(&stack);
        if !stack.is_empty() && stack.count() > max_stack_size {
            stack.set_count(max_stack_size);
        }
        self.items[slot] = stack;
    }

    fn get_max_stack_size(&self) -> i32 {
        64
    }

    fn set_changed(&mut self) {}
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};

    use super::*;

    fn dispenser() -> DispenserBlockEntity {
        init_vanilla_registry();
        DispenserBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::DISPENSER.default_state(),
        )
    }

    #[test]
    fn an_empty_dispenser_has_no_slot_to_pick() {
        assert_eq!(dispenser().get_random_slot(), None);
    }

    /// Vanilla parity: `getRandomSlot` only ever returns a filled slot.
    #[test]
    fn the_random_slot_is_always_one_that_holds_something() {
        let dispenser = dispenser();
        dispenser.set_item(4, ItemStack::new(&vanilla_items::STONE));

        for _ in 0..32 {
            assert_eq!(dispenser.get_random_slot(), Some(4));
        }
    }

    /// Reservoir sampling has to reach every filled slot, not just the first.
    #[test]
    fn every_filled_slot_can_be_picked() {
        let dispenser = dispenser();
        dispenser.set_item(0, ItemStack::new(&vanilla_items::STONE));
        dispenser.set_item(8, ItemStack::new(&vanilla_items::DIRT));

        let mut saw_first = false;
        let mut saw_last = false;
        for _ in 0..256 {
            match dispenser.get_random_slot() {
                Some(0) => saw_first = true,
                Some(8) => saw_last = true,
                other => panic!("picked an empty slot: {other:?}"),
            }
        }

        assert!(saw_first && saw_last);
    }

    #[test]
    fn inserting_tops_up_a_matching_stack_before_taking_a_new_slot() {
        let dispenser = dispenser();
        dispenser.set_item(0, ItemStack::with_count(&vanilla_items::STONE, 10));

        let leftover = dispenser.insert_item(ItemStack::with_count(&vanilla_items::STONE, 5));

        assert!(leftover.is_empty());
        assert_eq!(dispenser.get_item(0).count(), 15);
        assert!(dispenser.get_item(1).is_empty());
    }

    #[test]
    fn inserting_hands_back_what_does_not_fit() {
        let dispenser = dispenser();
        for slot in 0..DISPENSER_SLOTS {
            dispenser.set_item(slot, ItemStack::with_count(&vanilla_items::STONE, 64));
        }

        let leftover = dispenser.insert_item(ItemStack::with_count(&vanilla_items::DIRT, 3));

        assert_eq!(leftover.count(), 3);
    }
}
