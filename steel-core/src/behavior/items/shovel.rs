use steel_macros::item_behavior;
use steel_registry::{
    blocks::{
        Block,
        block_state_ext::BlockStateExt,
        properties::{BlockStateProperties, BoolProperty},
    },
    level_events, sound_events,
    vanilla_block_tags::BlockTag,
    vanilla_blocks, vanilla_game_events,
};
use steel_utils::Direction;
use steel_utils::types::UpdateFlags;

use crate::{
    behavior::{InteractionResult, ItemBehavior, UseOnContext},
    entity::Entity as _,
    world::game_event::GameEventContext,
};

const FLATTENABLES: [&Block; 6] = [
    &vanilla_blocks::GRASS_BLOCK,
    &vanilla_blocks::DIRT,
    &vanilla_blocks::PODZOL,
    &vanilla_blocks::COARSE_DIRT,
    &vanilla_blocks::MYCELIUM,
    &vanilla_blocks::ROOTED_DIRT,
];

const LIT_PROPERTY: BoolProperty = BlockStateProperties::LIT;

/// Behavior for Shovels, extinguishes campfires and turns grass blocks into paths
#[item_behavior]
pub struct ShovelItem;

impl ItemBehavior for ShovelItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        if context.hit_result.direction == Direction::Down {
            return InteractionResult::Pass;
        }

        let block_state = context.world.get_block_state(context.hit_result.block_pos);
        let block = block_state.get_block();

        let pos = context.hit_result.block_pos;

        // Vanilla builds an `updatedState` in one branch or the other and then
        // shares the tail: the block change, the game event and the durability.
        // Keeping that shape is what stops the campfire branch quietly skipping
        // the wear the path branch pays.
        let updated_state = if FLATTENABLES.contains(&block) {
            if !context.world.get_block_state(pos.above()).is_air() {
                return InteractionResult::Pass;
            }
            // Vanilla `level.playSound(player, ..)` excludes the acting player,
            // who already played the sound locally.
            context.world.play_block_sound(
                &sound_events::ITEM_SHOVEL_FLATTEN,
                pos,
                1.0,
                1.0,
                Some(context.player.id()),
            );
            vanilla_blocks::DIRT_PATH.default_state()
        } else if block.has_tag(&BlockTag::CAMPFIRES) {
            if !block_state.get_value(&LIT_PROPERTY) {
                return InteractionResult::Pass;
            }
            context
                .world
                .level_event(level_events::SOUND_EXTINGUISH_FIRE, pos, 0, None);
            // TODO: CampfireBlock::dowse() — eject cooking items
            block_state.set_value(&LIT_PROPERTY, false)
        } else {
            return InteractionResult::Pass;
        };

        context
            .world
            .set_block(pos, updated_state, UpdateFlags::UPDATE_ALL_IMMEDIATE);
        context.world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(context.player), Some(updated_state)),
        );
        context.player.hurt_item_in_hand(context.hand, 1);

        InteractionResult::Success
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::item_stack::ItemStack;
    use steel_registry::vanilla_items;
    use steel_utils::types::InteractionHand;
    use steel_utils::{BlockPos, ChunkPos};

    use super::*;
    use crate::behavior::BlockHitResult;
    use crate::bootstrap::init_globals_once;
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
    use crate::world::{LevelReader as _, World};

    const TARGET: BlockPos = BlockPos::new(8, 64, 8);

    fn world_with(key: &'static str, state: steel_utils::BlockStateId) -> Arc<World> {
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(TARGET));
        world.set_block(TARGET, state, UpdateFlags::UPDATE_NONE);
        world
    }

    fn player_with_a_shovel(world: &Arc<World>) -> Arc<Player> {
        let player = TestPlayerBuilder::new(Arc::clone(world), "Digger", 1).build();
        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::new(&vanilla_items::IRON_SHOVEL));
        player
    }

    fn dig(world: &Arc<World>, player: &Player) -> InteractionResult {
        let mut context = UseOnContext::new(
            player,
            InteractionHand::MainHand,
            BlockHitResult {
                location: DVec3::new(8.5, 65.0, 8.5),
                direction: Direction::Up,
                block_pos: TARGET,
                miss: false,
                inside: false,
                world_border_hit: false,
            },
            world,
            player.inventory.clone(),
        );
        ShovelItem.use_on(&mut context)
    }

    fn shovel_damage(player: &Player) -> i32 {
        player
            .inventory
            .lock()
            .get_selected_item()
            .get_damage_value()
    }

    #[test]
    fn making_a_path_wears_the_shovel() {
        init_globals_once();
        let world = world_with(
            "shovel_flatten_costs_durability",
            vanilla_blocks::GRASS_BLOCK.default_state(),
        );
        let player = player_with_a_shovel(&world);

        assert_eq!(dig(&world, &player), InteractionResult::Success);
        assert_eq!(
            world.get_block_state(TARGET).get_block(),
            &vanilla_blocks::DIRT_PATH
        );
        assert_eq!(shovel_damage(&player), 1);
    }

    /// Putting a campfire out costs the same point the path does. Vanilla pays
    /// it in the tail both branches share; Steel's campfire branch used to
    /// return before reaching it, so a shovel could dowse every campfire in a
    /// village for free.
    #[test]
    fn putting_out_a_campfire_wears_the_shovel_too() {
        init_globals_once();
        let world = world_with(
            "shovel_dowse_costs_durability",
            vanilla_blocks::CAMPFIRE.default_state(),
        );
        let player = player_with_a_shovel(&world);
        assert!(
            world.get_block_state(TARGET).get_value(&LIT_PROPERTY),
            "a fresh campfire is lit"
        );

        assert_eq!(dig(&world, &player), InteractionResult::Success);
        assert!(!world.get_block_state(TARGET).get_value(&LIT_PROPERTY));
        assert_eq!(shovel_damage(&player), 1);
    }

    /// An unlit campfire and a block with no shovel use both leave the tool
    /// alone -- the shared tail must not run for a click that did nothing.
    #[test]
    fn a_click_that_changes_nothing_is_free() {
        init_globals_once();
        let world = world_with(
            "shovel_unlit_campfire_is_free",
            vanilla_blocks::CAMPFIRE
                .default_state()
                .set_value(&LIT_PROPERTY, false),
        );
        let player = player_with_a_shovel(&world);

        assert_eq!(dig(&world, &player), InteractionResult::Pass);
        assert_eq!(shovel_damage(&player), 0);

        world.set_block(
            TARGET,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        );
        assert_eq!(dig(&world, &player), InteractionResult::Pass);
        assert_eq!(shovel_damage(&player), 0);
    }
}
