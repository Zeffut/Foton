//! Chest block entity implementation.
//!
//! Chests hold 27 slots (3x9 grid). Two adjacent chests facing the same
//! direction form a double chest, which is presented as a single 54-slot menu
//! while each half keeps its own independently lockable container.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase, ContainerLoot};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Number of slots in a single chest (3 rows of 9).
pub const CHEST_SLOTS: usize = 27;

/// Chest block entity.
///
/// Vanilla parity: `ChestBlockEntity`. A double chest is not a distinct block
/// entity: it is two `ChestBlockEntity` halves combined at menu creation time.
pub struct ChestBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<ChestContainer>>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `RandomizableContainer` half of a chest, which is
    /// what a generated dungeon or mineshaft chest arrives with.
    loot: Arc<ContainerLoot>,
}

struct ChestContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ChestBlockEntity`.
unsafe impl DowncastType for ChestBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/chest");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently
// lockable inventory data used by a chest block entity.
unsafe impl DowncastType for ChestContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/chest");
}

impl ChestBlockEntity {
    /// Creates a new chest block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(&vanilla_block_entity_types::CHEST, level, pos, state)
    }

    /// Creates a chest block entity of a given type.
    ///
    /// Vanilla parity: `TrappedChestBlockEntity`, which exists only to carry a
    /// different block entity type -- the storage is identical. The type is not
    /// cosmetic: `BlockEntityBase::new` refuses a type that does not match the
    /// block, which is what caught this being hard-coded.
    #[must_use]
    pub fn with_type(
        block_entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        let base = Arc::new(BlockEntityBase::new(block_entity_type, level, pos, state));
        let container = Arc::new(SyncMutex::new(ChestContainer {
            items: vec![ItemStack::empty(); CHEST_SLOTS],
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
}

impl BlockEntity for ChestBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        // Vanilla drops what a chest holds through `Container.getItem`, which
        // rolls a packed table first: breaking an untouched dungeon chest
        // scatters its loot rather than nothing.
        self.container_ref.unpack_loot_table(None);
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); CHEST_SLOTS])
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
        // Vanilla parity: a chest stores either a loot table or its items,
        // never both, and clears the slots either way.
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
                    if slot < CHEST_SLOTS
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

    fn get_update_tag(&self) -> Option<NbtCompound> {
        // Chest contents are not needed client-side on chunk load.
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for ChestContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        CHEST_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < CHEST_SLOTS {
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
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::blocks::properties::Direction;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::{BLOCK_BEHAVIORS, init_behaviors};
    use crate::block_entity::init_block_entities;
    use crate::inventory::lock::ContainerLockGuard;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The loot table every generated dungeon chest carries.
    const SIMPLE_DUNGEON: &str = "minecraft:chests/simple_dungeon";

    fn test_chest() -> ChestBlockEntity {
        init_vanilla_registry();
        ChestBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::CHEST.default_state(),
        )
    }

    fn load_from_owned_nbt(entity: &dyn BlockEntity, nbt: &NbtCompound) {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("test nbt should reborrow");
        entity.load_additional(&borrowed);
    }

    /// The NBT worldgen writes onto a chest it places inside a structure.
    fn generated_chest_nbt(seed: i64) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("LootTable", SIMPLE_DUNGEON);
        nbt.insert("LootTableSeed", seed);
        nbt
    }

    /// Places a chest carrying a still-packed loot table in a fresh world.
    fn generated_chest(key: &'static str, seed: i64) -> (Arc<World>, BlockPos, ContainerRef) {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world(key);
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        assert!(world.set_block(
            pos,
            vanilla_blocks::CHEST.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        let block_entity = world
            .get_block_entity(pos)
            .expect("a placed chest should have a block entity");
        load_from_owned_nbt(block_entity.as_ref(), &generated_chest_nbt(seed));
        let container = ContainerRef::from_block_entity(block_entity)
            .expect("a chest should expose a container");
        (world, pos, container)
    }

    /// Reads the chest through the ordinary container path, which is what rolls
    /// a still-packed table.
    fn contents(container: &ContainerRef) -> Vec<(usize, String, i32)> {
        let guard = ContainerLockGuard::lock_all(&[container]);
        let locked = guard
            .get(container.container_id())
            .expect("the container was just locked");
        (0..locked.get_container_size())
            .filter(|&slot| !locked.get_item(slot).is_empty())
            .map(|slot| {
                let item = locked.get_item(slot);
                (slot, item.item.key.to_string(), item.count())
            })
            .collect()
    }

    #[test]
    fn set_item_limits_stack_to_vanilla_container_maximum() {
        let chest = test_chest();
        chest
            .container
            .lock()
            .set_item(0, ItemStack::with_count(&vanilla_items::STONE, 100));

        assert_eq!(chest.container.lock().get_item(0).count(), 64);
    }

    /// Worldgen writes the table and nothing looks inside yet, so the chest has
    /// to hand the same table back when the chunk is written out again.
    #[test]
    fn a_generated_chest_saves_its_table_instead_of_empty_slots() {
        let chest = test_chest();
        load_from_owned_nbt(&chest, &generated_chest_nbt(42));

        let mut saved = NbtCompound::new();
        chest.save_additional(&mut saved);

        assert_eq!(
            saved.string("LootTable").map(ToString::to_string),
            Some(SIMPLE_DUNGEON.to_owned())
        );
        assert_eq!(saved.long("LootTableSeed"), Some(42));
        assert!(
            saved.list("Items").is_none(),
            "a packed chest stores its table, never both"
        );
    }

    /// A chest that already holds items is stored the old way.
    #[test]
    fn a_chest_with_no_table_still_saves_its_items() {
        let chest = test_chest();
        chest
            .container
            .lock()
            .set_item(4, ItemStack::new(&vanilla_items::DIAMOND));

        let mut saved = NbtCompound::new();
        chest.save_additional(&mut saved);

        assert!(saved.string("LootTable").is_none());
        assert!(saved.list("Items").is_some());
    }

    #[test]
    fn the_same_loot_table_seed_fills_a_chest_the_same_way_twice() {
        let (_first_world, _, first) = generated_chest("chest_loot_seed_a", 1234);
        let (_second_world, _, second) = generated_chest("chest_loot_seed_b", 1234);

        let rolled = contents(&first);
        assert!(!rolled.is_empty(), "simple_dungeon should roll something");
        assert_eq!(rolled, contents(&second));
    }

    #[test]
    fn a_different_loot_table_seed_fills_a_chest_differently() {
        let (_first_world, _, first) = generated_chest("chest_loot_seed_c", 1234);
        let (_second_world, _, second) = generated_chest("chest_loot_seed_d", 9876);

        assert_ne!(contents(&first), contents(&second));
    }

    /// Once rolled, the table is gone: the chest saves its items and comes back
    /// holding exactly those, rather than rolling a second time.
    #[test]
    fn an_unpacked_chest_stays_unpacked_across_a_save_and_load() {
        let (world, pos, container) = generated_chest("chest_loot_round_trip", 1234);
        let rolled = contents(&container);
        assert!(!rolled.is_empty(), "simple_dungeon should roll something");

        let saved = world
            .get_block_entity(pos)
            .expect("the chest is still there")
            .save_custom_only();
        assert!(
            saved.string("LootTable").is_none(),
            "a rolled chest must not save the table it already spent"
        );

        let reloaded = test_chest();
        load_from_owned_nbt(&reloaded, &saved);
        let reloaded_contents: Vec<(usize, String, i32)> = reloaded
            .container
            .lock()
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.is_empty())
            .map(|(slot, item)| (slot, item.item.key.to_string(), item.count()))
            .collect();

        assert_eq!(reloaded_contents, rolled);
    }

    /// Vanilla rolls the table from `getItem`, not from opening the menu, so a
    /// comparator -- or a hopper -- reaches a generated chest without a player.
    #[test]
    fn a_comparator_reading_a_generated_chest_rolls_it() {
        let (world, pos, _container) = generated_chest("chest_loot_comparator", 1234);

        let signal = BLOCK_BEHAVIORS
            .get_behavior(&vanilla_blocks::CHEST)
            .get_analog_output_signal(
                world.get_block_state(pos),
                world.as_ref(),
                pos,
                Direction::West,
            );

        assert!(
            signal > 0,
            "an untouched dungeon chest read as empty; the table was never rolled"
        );
    }

    #[test]
    fn pre_remove_empties_the_container() {
        let chest = test_chest();
        chest
            .container
            .lock()
            .set_item(0, ItemStack::new(&vanilla_items::STONE));

        chest.pre_remove_side_effects(
            BlockPos::new(1, 2, 3),
            vanilla_blocks::CHEST.default_state(),
        );

        let container = chest.container.lock();
        assert_eq!(container.items.len(), CHEST_SLOTS);
        assert!(container.items.iter().all(ItemStack::is_empty));
    }
}
