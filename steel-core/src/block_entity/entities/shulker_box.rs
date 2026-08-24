//! Shulker box block entity.
//!
//! Vanilla parity: `ShulkerBoxBlockEntity`. Twenty-seven slots like a barrel --
//! what makes it a shulker box is that the contents leave with the block rather
//! than scattering, which is why [`Self::pre_remove_side_effects`] deliberately
//! does nothing and the block's loot override carries them out instead.

use std::mem;
use std::sync::{Arc, Weak};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::block_entity::{BlockEntity, BlockEntityBase, ContainerLoot};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Number of slots in a shulker box.
pub const SHULKER_BOX_SLOTS: usize = 27;

/// A shulker box.
pub struct ShulkerBoxBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<ShulkerBoxContainer>>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `RandomizableContainer` half of a shulker box, which
    /// is how an end city box arrives stocked.
    loot: Arc<ContainerLoot>,
}

struct ShulkerBoxContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies the block entity.
unsafe impl DowncastType for ShulkerBoxBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/shulker_box");
}

// SAFETY: This key is owned by Steel and uniquely identifies the inventory.
unsafe impl DowncastType for ShulkerBoxContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/shulker_box");
}

impl ShulkerBoxBlockEntity {
    /// Creates a shulker box block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::SHULKER_BOX,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(ShulkerBoxContainer {
            items: vec![ItemStack::empty(); SHULKER_BOX_SLOTS],
        }));
        let shared: SharedContainer = container.clone();
        let loot = Arc::new(ContainerLoot::new());
        Self {
            container_ref: ContainerRef::owned_by_randomizable_block_entity(
                shared,
                Arc::clone(&base),
                Arc::clone(&loot),
            ),
            base,
            container,
            loot,
        }
    }

    /// Returns a copy of everything inside.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ItemStack> {
        self.container_ref.unpack_loot_table(None);
        self.container.lock().items.clone()
    }

    /// Replaces the contents, which is how a placed box gets its items back.
    pub fn restore(&self, items: &[ItemStack]) {
        let mut container = self.container.lock();
        for (slot, item) in items.iter().take(SHULKER_BOX_SLOTS).enumerate() {
            container.items[slot] = item.clone();
        }
    }

    /// Empties the box without dropping anything.
    ///
    /// The contents leave inside the item the block drops, so scattering them
    /// as well would duplicate every stack.
    #[must_use]
    pub fn take_all(&self) -> Vec<ItemStack> {
        self.container_ref.unpack_loot_table(None);
        let mut container = self.container.lock();
        mem::replace(
            &mut container.items,
            vec![ItemStack::empty(); SHULKER_BOX_SLOTS],
        )
    }

    /// Returns whether the box holds nothing.
    ///
    /// Vanilla parity: `RandomizableContainerBlockEntity.isEmpty`, which rolls
    /// a packed table before answering -- a generated box is not empty, it just
    /// has not been looked in yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.container_ref.unpack_loot_table(None);
        self.container.lock().items.iter().all(ItemStack::is_empty)
    }
}

impl BlockEntity for ShulkerBoxBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: a shulker box does *not* scatter its contents. They ride
    /// out inside the item the block drops, which is the whole point of it.
    fn pre_remove_side_effects(&self, _pos: BlockPos, _state: BlockStateId) {}

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        // Vanilla parity: `ShulkerBoxBlockEntity.loadFromTag`, which stores
        // either a loot table or its items.
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
                    if slot < SHULKER_BOX_SLOTS
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
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "there are twenty-seven slots"
                )]
                item_nbt.insert("Slot", slot as i8);
                items.push(item_nbt);
            }
        }
        nbt.insert("Items", NbtList::Compound(items));
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for ShulkerBoxContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        SHULKER_BOX_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < SHULKER_BOX_SLOTS {
            let max_stack_size = self.get_max_stack_size_for_item(&stack);
            if !stack.is_empty() && stack.count() > max_stack_size {
                stack.set_count(max_stack_size);
            }
            self.items[slot] = stack;
        }
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

    fn test_box() -> ShulkerBoxBlockEntity {
        init_vanilla_registry();
        ShulkerBoxBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::SHULKER_BOX.default_state(),
        )
    }

    /// Breaking a shulker box must not scatter its contents.
    ///
    /// The barrel next door does exactly that, and copying it would have made a
    /// shulker box an expensive barrel: the items would fall on the floor and
    /// then be duplicated by the loot override that carries them out inside the
    /// item.
    #[test]
    fn removal_does_not_scatter_the_contents() {
        let shulker = test_box();
        shulker
            .container
            .lock()
            .set_item(0, ItemStack::new(&vanilla_items::DIAMOND));

        shulker.pre_remove_side_effects(
            BlockPos::new(1, 2, 3),
            vanilla_blocks::SHULKER_BOX.default_state(),
        );

        assert!(!shulker.is_empty(), "the diamond should still be inside");
    }

    #[test]
    fn taking_everything_leaves_an_empty_box() {
        let shulker = test_box();
        shulker
            .container
            .lock()
            .set_item(3, ItemStack::new(&vanilla_items::DIAMOND));

        let taken = shulker.take_all();

        assert_eq!(taken.len(), SHULKER_BOX_SLOTS);
        assert!(taken[3].is(&vanilla_items::DIAMOND));
        assert!(shulker.is_empty());
    }

    #[test]
    fn restoring_puts_the_items_back_in_their_slots() {
        let shulker = test_box();
        let mut items = vec![ItemStack::empty(); SHULKER_BOX_SLOTS];
        items[7] = ItemStack::with_count(&vanilla_items::DIAMOND, 5);

        shulker.restore(&items);

        let container = shulker.container.lock();
        assert!(container.get_item(7).is(&vanilla_items::DIAMOND));
        assert_eq!(container.get_item(7).count(), 5);
    }
}
