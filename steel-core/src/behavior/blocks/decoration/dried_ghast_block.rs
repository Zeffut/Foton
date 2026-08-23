//! Vanilla `DriedGhastBlock` behavior.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, IntProperty,
};
use steel_registry::fluid::FluidState;
use steel_registry::{sound_events, vanilla_game_events};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::PlacementSource;
use crate::behavior::block::{
    BlockBehavior, place_simple_waterlogged_liquid, schedule_water_tick_if_waterlogged,
};
use crate::behavior::context::BlockPlaceContext;
use crate::entity::ai::path::PathComputationType;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelAccessor, ScheduledTickAccess, World};

/// Vanilla `DriedGhastBlock`.
///
/// A dried ghast dries out on land and rehydrates in water, one stage every
/// `HYDRATION_TICK_DELAY` ticks; at the last stage under water it hatches.
#[block_behavior]
pub struct DriedGhastBlock {
    block: BlockRef,
}

const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const HYDRATION_LEVEL: &IntProperty = &BlockStateProperties::DRIED_GHAST_HYDRATION_LEVELS;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Vanilla `DriedGhastBlock.MAX_HYDRATION_LEVEL`.
const MAX_HYDRATION_LEVEL: u8 = 3;
/// Vanilla `DriedGhastBlock.HYDRATION_TICK_DELAY`.
const HYDRATION_TICK_DELAY: i32 = 5000;

const SOUND_VOLUME: f32 = 1.0;
const SOUND_PITCH: f32 = 1.0;

impl DriedGhastBlock {
    /// Creates a dried ghast behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `DriedGhastBlock.isReadyToSpawn`.
    fn is_ready_to_spawn(state: BlockStateId) -> bool {
        state.get_value(HYDRATION_LEVEL) == MAX_HYDRATION_LEVEL
    }

    /// Writes a new hydration level and fires the block change vanilla reports.
    fn set_hydration(state: BlockStateId, world: &Arc<World>, pos: BlockPos, hydration: u8) {
        world.set_block(
            pos,
            state.set_value(HYDRATION_LEVEL, hydration),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(None, Some(state)),
        );
    }

    /// Vanilla `DriedGhastBlock.tickWaterlogged`.
    fn tick_waterlogged(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if Self::is_ready_to_spawn(state) {
            Self::spawn_ghastling(world, pos);
            return;
        }

        world.play_sound(
            &sound_events::BLOCK_DRIED_GHAST_TRANSITION,
            SoundSource::Blocks,
            pos,
            SOUND_VOLUME,
            SOUND_PITCH,
            None,
        );
        Self::set_hydration(state, world, pos, state.get_value(HYDRATION_LEVEL) + 1);
    }

    /// Vanilla `DriedGhastBlock.spawnGhastling`.
    fn spawn_ghastling(world: &Arc<World>, pos: BlockPos) {
        world.remove_block(pos, false);
        // Vanilla then adds a baby `HappyGhast` at the bottom center of this
        // block, facing the way the block did, and plays `GHASTLING_SPAWN` on
        // it. Steel has no `HappyGhast` entity, so the block just disappears;
        // the spawn and its sound belong on this line once the entity exists.
    }
}

impl BlockBehavior for DriedGhastBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(WATERLOGGED, context.is_water_source())
                .set_value(FACING, context.horizontal_direction().opposite()),
        )
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);

        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if state.get_value(WATERLOGGED) {
            Self::tick_waterlogged(state, world, pos);
            return;
        }

        let hydration = state.get_value(HYDRATION_LEVEL);
        if hydration > 0 {
            Self::set_hydration(state, world, pos, hydration - 1);
        }
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let drying_or_soaking =
            state.get_value(WATERLOGGED) || state.get_value(HYDRATION_LEVEL) > 0;
        if drying_or_soaking && !world.has_scheduled_block_tick(pos, self.block) {
            world.schedule_block_tick_default(pos, self.block, HYDRATION_TICK_DELAY);
        }
    }

    fn place_liquid(
        &self,
        level: &dyn LevelAccessor,
        pos: BlockPos,
        state: BlockStateId,
        fluid_state: FluidState,
    ) -> bool {
        if !place_simple_waterlogged_liquid(level, pos, state, fluid_state) {
            return false;
        }

        level.play_block_sound(
            &sound_events::BLOCK_DRIED_GHAST_PLACE_IN_WATER,
            pos,
            SOUND_VOLUME,
            SOUND_PITCH,
            None,
        );
        true
    }

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source: &PlacementSource<'_>,
    ) {
        let sound = if state.get_value(WATERLOGGED) {
            &sound_events::BLOCK_DRIED_GHAST_PLACE_IN_WATER
        } else {
            &sound_events::BLOCK_DRIED_GHAST_PLACE
        };
        world.play_sound(
            sound,
            SoundSource::Blocks,
            pos,
            SOUND_VOLUME,
            SOUND_PITCH,
            None,
        );
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn dried_ghast(waterlogged: bool, hydration: u8) -> BlockStateId {
        vanilla_blocks::DRIED_GHAST
            .default_state()
            .set_value(WATERLOGGED, waterlogged)
            .set_value(HYDRATION_LEVEL, hydration)
    }

    #[test]
    fn a_dried_ghast_soaks_up_one_stage_per_tick_and_dries_out_the_same_way() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("dried_ghast_hydration");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let behavior = DriedGhastBlock::new(&vanilla_blocks::DRIED_GHAST);

        let mut state = dried_ghast(true, 0);
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));
        for expected in 1..=MAX_HYDRATION_LEVEL {
            behavior.tick(state, &world, pos);
            state = world.get_block_state(pos);
            assert_eq!(state.get_value(HYDRATION_LEVEL), expected);
        }

        let mut state = dried_ghast(false, MAX_HYDRATION_LEVEL);
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));
        for expected in (0..MAX_HYDRATION_LEVEL).rev() {
            behavior.tick(state, &world, pos);
            state = world.get_block_state(pos);
            assert_eq!(state.get_value(HYDRATION_LEVEL), expected);
        }

        // A dry ghast at zero stays put rather than looping around.
        behavior.tick(state, &world, pos);
        assert_eq!(world.get_block_state(pos), state);
    }

    #[test]
    fn a_fully_soaked_dried_ghast_hatches_out_of_the_world() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("dried_ghast_hatches");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let behavior = DriedGhastBlock::new(&vanilla_blocks::DRIED_GHAST);
        let state = dried_ghast(true, MAX_HYDRATION_LEVEL);
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));

        behavior.tick(state, &world, pos);
        // Vanilla `removeBlock` leaves the fluid the block was logged with, so
        // a soaked ghast hatches into the water it was sitting in.
        assert_eq!(
            world.get_block_state(pos).get_block(),
            &vanilla_blocks::WATER
        );
    }
}
