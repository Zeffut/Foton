//! Shulker box block behavior.
//!
//! Vanilla parity: `ShulkerBoxBlock`. Seventeen blocks -- one undyed and
//! sixteen colors -- that are a chest until you break one, at which point the
//! contents leave inside the item. That last part is the block: a shulker box
//! that scattered its contents would be an expensive barrel.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use foton_registry::data_components::components::ItemContainerContents;
use foton_registry::data_components::vanilla_components::CONTAINER;
use foton_registry::item_stack::ItemStack;
use foton_registry::item_stack_template::ItemStackTemplate;
use foton_registry::{REGISTRY, RegistryExt as _, vanilla_block_entity_types};
use foton_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use glam::DVec3;
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation, BlockLootContext};
use crate::behavior::context::{
    BlockHitResult, BlockPlaceContext, InteractionResult, PlacementSource,
};
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::entities::{SHULKER_BOX_SLOTS, ShulkerBoxBlockEntity};
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::chest;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Which way the box opens.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::FACING;

/// Rows the menu shows.
const MENU_ROWS: usize = 3;

/// Behavior for every shulker box.
#[block_behavior]
pub struct ShulkerBoxBlock {
    block: BlockRef,
}

impl ShulkerBoxBlock {
    /// Creates a shulker box block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Empties the box into the item that represents it.
    ///
    /// Vanilla parity: the `itemStack.applyComponents(blockEntity.collectComponents())`
    /// that `ShulkerBoxBlock.playerWillDestroy` and the `CONTENTS` dynamic drop
    /// of `getDrops` both go through. Both callers here need the same stack,
    /// and only one of them ever runs for a given break.
    fn take_contents_into_item(&self, shulker: &ShulkerBoxBlockEntity) -> Option<ItemStack> {
        let mut dropped = ItemStack::new(REGISTRY.items.by_key(&self.block.key)?);
        let templates: Vec<Option<ItemStackTemplate>> = shulker
            .take_all()
            .iter()
            .map(|item| ItemStackTemplate::from_stack(item).ok())
            .collect();

        if let Ok(contents) = ItemContainerContents::new(templates) {
            dropped.set(CONTAINER, contents);
        }

        Some(dropped)
    }
}

impl BlockBehavior for ShulkerBoxBlock {
    /// Vanilla parity: `ShulkerBoxBlock.getStateForPlacement`, which points the
    /// box at the face that was clicked -- so one placed on a ceiling opens
    /// downwards.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.clicked_face()),
        )
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(container_ref) = ContainerRef::from_block_entity(block_entity.clone()) else {
            return InteractionResult::Pass;
        };

        // Vanilla parity: `RandomizableContainerBlockEntity.createMenu`
        // unpacks with the opening player, whose luck the roll uses.
        container_ref.unpack_loot_table(Some(player));

        let inventory = player.inventory.clone();
        player.open_menu(
            block_entity.display_name(TextComponent::translated(
                translations::CONTAINER_SHULKER_BOX.msg(),
            )),
            move |context| chest(inventory, context.container_id, container_ref, MENU_ROWS),
        );

        // TODO: vanilla refuses to open a box whose lid would be inside a
        // block, and angers nearby piglins. Foton has neither the lid animation
        // nor piglins yet.
        InteractionResult::Success
    }

    /// Puts the item's contents back into the placed box.
    ///
    /// Vanilla parity: the `applyComponentsFromItemStack` every block entity
    /// gets on placement. Without this the contents survive being broken and
    /// then vanish when the box is put down again, which is worse than losing
    /// them outright because the item still says it has them.
    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        let Some(contents) = source.with_item(|item| item.get(CONTAINER).cloned()) else {
            return;
        };
        let Some(block_entity) = world.get_block_entity(pos) else {
            return;
        };
        let Some(shulker) = block_entity.downcast_ref::<ShulkerBoxBlockEntity>() else {
            return;
        };

        let mut items = vec![ItemStack::empty(); SHULKER_BOX_SLOTS];
        for (slot, template) in contents.items().iter().enumerate().take(items.len()) {
            if let Some(template) = template {
                items[slot] = template.create();
            }
        }
        shulker.restore(&items);
    }

    /// Drops one box carrying everything that was inside it.
    ///
    /// Vanilla parity: the `CONTENTS` dynamic drop of `ShulkerBoxBlock.getDrops`.
    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        let block_entity = context.world().get_block_entity(context.pos())?;
        let shulker = block_entity.downcast_ref::<ShulkerBoxBlockEntity>()?;

        Some(vec![self.take_contents_into_item(shulker)?])
    }

    /// Hands a creative player the box they just broke, contents included.
    ///
    /// Vanilla parity: `ShulkerBoxBlock.playerWillDestroy`. Creative breaking
    /// skips block loot entirely, so `get_drops` never runs and the box would
    /// be deleted with everything in it. Vanilla answers that by spawning the
    /// item here instead -- the one block that drops itself in creative.
    fn player_will_destroy(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> BlockStateId {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return state;
        };
        let Some(shulker) = block_entity.downcast_ref::<ShulkerBoxBlockEntity>() else {
            return state;
        };

        // An empty box in creative is left to vanish, exactly as vanilla's
        // `!shulkerBoxBlockEntity.isEmpty()` decides.
        if player.has_infinite_materials() && !shulker.is_empty() {
            if let Some(dropped) = self.take_contents_into_item(shulker) {
                let (x, y, z) = pos.get_center();
                world.spawn_item(DVec3::new(x, y, z), dropped);
            }
        } else if let Some(container_ref) = ContainerRef::from_block_entity(block_entity.clone()) {
            // Vanilla rolls a still-packed loot table with the breaking player,
            // whose luck the roll uses.
            container_ref.unpack_loot_table(Some(player));
        }

        state
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::SHULKER_BOX,
            level,
            pos,
            state,
        ))
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, calculate_redstone_signal_from_container)
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_items};
    use foton_utils::ChunkPos;
    use foton_utils::types::{InteractionHand, UpdateFlags};

    use foton_utils::types::GameType;
    use foton_utils::WorldAabb;

    use super::*;
    use crate::behavior::context::PlacementOrientation;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::entity::entities::ItemEntity;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    /// A shulker box with one diamond in it, standing in a real world.
    fn placed_box(name: &'static str, pos: BlockPos) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let _ = world.set_block(
            pos,
            vanilla_blocks::SHULKER_BOX.default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        world
    }

    fn put_diamond_in(world: &Arc<World>, pos: BlockPos, slot: usize, count: i32) {
        let block_entity = world
            .get_block_entity(pos)
            .expect("a placed shulker box has a block entity");
        let container_ref =
            ContainerRef::from_block_entity(block_entity).expect("it is a container");
        let mut guard = ContainerLockGuard::lock_all(&[&container_ref]);
        let container = guard
            .get_mut(container_ref.container_id())
            .expect("the container is locked");
        container.set_item(slot, ItemStack::with_count(&vanilla_items::DIAMOND, count));
    }

    /// The whole point of the block: break it, and the contents come with it.
    ///
    /// Placing one back must restore them. A box that kept its contents on the
    /// way out and lost them on the way back in would be worse than one that
    /// never kept them, because the item would still claim to be full.
    #[test]
    fn the_contents_survive_being_broken_and_placed_again() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_box("shulker_round_trip", pos);
        put_diamond_in(&world, pos, 5, 12);

        let behavior = ShulkerBoxBlock::new(&vanilla_blocks::SHULKER_BOX);
        let drops = behavior
            .get_drops(
                vanilla_blocks::SHULKER_BOX.default_state(),
                &BlockLootContext::new(&world, pos),
            )
            .expect("a shulker box overrides its own loot");

        assert_eq!(drops.len(), 1, "one box, not a pile of loose items");
        let mut dropped = drops.into_iter().next().expect("the box");
        let carried = dropped
            .get(CONTAINER)
            .cloned()
            .expect("the dropped box carries its contents");
        assert!(
            carried.items().get(5).and_then(Option::as_ref).is_some(),
            "the diamond should be in slot five of the item"
        );

        // Now put it back down somewhere else and see the diamond again.
        let elsewhere = BlockPos::new(9, 70, 8);
        let _ = world.set_block(
            elsewhere,
            vanilla_blocks::SHULKER_BOX.default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        let source = PlacementSource::direct(
            None,
            InteractionHand::MainHand,
            &mut dropped,
            PlacementOrientation::Player {
                rotation: 0.0,
                pitch: 0.0,
            },
            false,
        );
        behavior.set_placed_by(
            vanilla_blocks::SHULKER_BOX.default_state(),
            &world,
            elsewhere,
            &source,
        );

        let restored = world
            .get_block_entity(elsewhere)
            .and_then(|entity| {
                entity
                    .downcast_ref::<ShulkerBoxBlockEntity>()
                    .map(ShulkerBoxBlockEntity::snapshot)
            })
            .expect("the new box is a shulker box");
        assert!(
            restored[5].is(&vanilla_items::DIAMOND),
            "the diamond did not come back"
        );
        assert_eq!(restored[5].count(), 12, "the stack changed size");
    }

    /// Every item stack lying on the ground at `pos`.
    fn dropped_items(world: &Arc<World>, pos: BlockPos) -> Vec<ItemStack> {
        let (x, y, z) = pos.get_center();
        let center = DVec3::new(x, y, z);
        world
            .get_entities_in_aabb_matching(
                &WorldAabb::from_min_max(center - DVec3::ONE, center + DVec3::ONE),
                |entity| entity.entity_type() == &vanilla_entities::ITEM,
            )
            .iter()
            .filter_map(|entity| entity.downcast_ref::<ItemEntity>().map(ItemEntity::get_item))
            .collect()
    }

    /// Breaking a full box in creative still hands the box over.
    ///
    /// Creative skips block loot entirely, so `get_drops` never runs and the
    /// only thing standing between the player and a deleted inventory is
    /// `player_will_destroy`. That is the report this test exists for.
    #[test]
    fn a_creative_break_still_drops_the_box_with_its_contents() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_box("shulker_creative_drop", pos);
        put_diamond_in(&world, pos, 5, 12);

        let player = TestPlayerBuilder::new(Arc::clone(&world), "ShulkerTester", 1).build();
        player.restore_game_modes(GameType::Creative, None);

        let behavior = ShulkerBoxBlock::new(&vanilla_blocks::SHULKER_BOX);
        behavior.player_will_destroy(
            vanilla_blocks::SHULKER_BOX.default_state(),
            &world,
            pos,
            &player,
        );

        let dropped = dropped_items(&world, pos);
        assert_eq!(dropped.len(), 1, "creative should drop exactly one box");

        let item = &dropped[0];
        assert!(
            item.is(&vanilla_items::SHULKER_BOX),
            "the dropped item should be the box itself"
        );
        let carried = item
            .get(CONTAINER)
            .cloned()
            .expect("the box carries its contents out of a creative break");
        assert!(
            carried.items().get(5).and_then(Option::as_ref).is_some(),
            "the diamond should still be in slot five"
        );
    }

    /// An empty box broken in creative is left to vanish, as vanilla does.
    #[test]
    fn a_creative_break_of_an_empty_box_drops_nothing() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_box("shulker_creative_empty", pos);

        let player = TestPlayerBuilder::new(Arc::clone(&world), "ShulkerTester", 1).build();
        player.restore_game_modes(GameType::Creative, None);

        let behavior = ShulkerBoxBlock::new(&vanilla_blocks::SHULKER_BOX);
        behavior.player_will_destroy(
            vanilla_blocks::SHULKER_BOX.default_state(),
            &world,
            pos,
            &player,
        );

        assert!(
            dropped_items(&world, pos).is_empty(),
            "an empty box in creative drops nothing"
        );
    }

    /// Taking the loot empties the box, so nothing can be duplicated.
    #[test]
    fn the_broken_box_is_emptied_so_nothing_is_duplicated() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_box("shulker_no_dupe", pos);
        put_diamond_in(&world, pos, 0, 1);

        let behavior = ShulkerBoxBlock::new(&vanilla_blocks::SHULKER_BOX);
        let _ = behavior.get_drops(
            vanilla_blocks::SHULKER_BOX.default_state(),
            &BlockLootContext::new(&world, pos),
        );

        let still_inside = world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<ShulkerBoxBlockEntity>()
                    .map(ShulkerBoxBlockEntity::is_empty)
            })
            .expect("the box is still there until the block is removed");
        assert!(still_inside, "the contents left twice");
    }
}
