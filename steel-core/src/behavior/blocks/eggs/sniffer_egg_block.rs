//! Vanilla `SnifferEggBlock` behavior.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, IntProperty};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{level_events, sound_events, vanilla_game_events};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::entity::ai::path::PathComputationType;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Vanilla `SnifferEggBlock` behavior.
#[block_behavior]
pub struct SnifferEggBlock {
    block: BlockRef,
}

const HATCH: &IntProperty = &BlockStateProperties::HATCH;

/// Vanilla `SnifferEggBlock.MAX_HATCH_LEVEL`.
const MAX_HATCH_LEVEL: u8 = 2;
/// Vanilla `SnifferEggBlock.REGULAR_HATCH_TIME_TICKS`.
const REGULAR_HATCH_TIME_TICKS: i32 = 24_000;
/// Vanilla `SnifferEggBlock.BOOSTED_HATCH_TIME_TICKS`, used on moss.
const BOOSTED_HATCH_TIME_TICKS: i32 = 12_000;
/// Vanilla `SnifferEggBlock.RANDOM_HATCH_OFFSET_TICKS`.
const RANDOM_HATCH_OFFSET_TICKS: i32 = 300;
/// The three stages the hatch time is split across: two cracks and the hatch.
const HATCH_STAGES: i32 = 3;

impl SnifferEggBlock {
    /// Creates a sniffer egg behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `SnifferEggBlock.hatchBoost`: moss under the egg halves the wait.
    #[must_use]
    pub fn hatch_boost(world: &dyn LevelReader, pos: BlockPos) -> bool {
        world
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::SNIFFER_EGG_HATCH_BOOST)
    }

    /// Vanilla `SnifferEggBlock.isReadyToHatch`.
    #[must_use]
    fn is_ready_to_hatch(state: BlockStateId) -> bool {
        state.get_value(HATCH) == MAX_HATCH_LEVEL
    }

    /// The delay vanilla `onPlace` schedules for the next hatch stage.
    #[must_use]
    fn progression_tick_delay(boosted: bool) -> i32 {
        let hatch_time = if boosted {
            BOOSTED_HATCH_TIME_TICKS
        } else {
            REGULAR_HATCH_TIME_TICKS
        };

        hatch_time / HATCH_STAGES + rand::random_range(0..RANDOM_HATCH_OFFSET_TICKS)
    }
}

impl BlockBehavior for SnifferEggBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        let boosted = Self::hatch_boost(world.as_ref(), pos);
        if boosted {
            world.level_event(level_events::PARTICLES_EGG_CRACK, pos, 0, None);
        }

        world.game_event(
            &vanilla_game_events::BLOCK_PLACE,
            pos,
            &GameEventContext::new(None, Some(state)),
        );
        world.schedule_block_tick_default(pos, self.block, Self::progression_tick_delay(boosted));
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !Self::is_ready_to_hatch(state) {
            world.play_sound(
                &sound_events::BLOCK_SNIFFER_EGG_CRACK,
                SoundSource::Blocks,
                pos,
                0.7,
                0.9 + rand::random::<f32>() * 0.2,
                None,
            );
            // Writing the next hatch level re-enters `on_place`, which is how
            // vanilla schedules the following stage: `SnifferEggBlock` has no
            // random ticking and never calls `scheduleTick` from `tick`.
            world.set_block(
                pos,
                state.set_value(HATCH, state.get_value(HATCH) + 1),
                UpdateFlags::UPDATE_CLIENTS,
            );
            return;
        }

        world.play_sound(
            &sound_events::BLOCK_SNIFFER_EGG_HATCH,
            SoundSource::Blocks,
            pos,
            0.7,
            0.9 + rand::random::<f32>() * 0.2,
            None,
        );
        world.destroy_block(pos, false);
        // Vanilla spawns a baby `Sniffer` at the center of this block, facing a
        // random yaw. Steel has no `Sniffer` entity, so the egg just disappears;
        // the spawn belongs on this line once `EntityTypes.SNIFFER` is
        // implemented.
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;
    use crate::test_support::TestLevel;

    #[test]
    fn moss_under_a_sniffer_egg_halves_the_wait_between_stages() {
        init_vanilla_registry();

        let boosted = SnifferEggBlock::progression_tick_delay(true);
        let regular = SnifferEggBlock::progression_tick_delay(false);
        let boosted_base = BOOSTED_HATCH_TIME_TICKS / HATCH_STAGES;
        let regular_base = REGULAR_HATCH_TIME_TICKS / HATCH_STAGES;

        assert!((boosted_base..boosted_base + RANDOM_HATCH_OFFSET_TICKS).contains(&boosted));
        assert!((regular_base..regular_base + RANDOM_HATCH_OFFSET_TICKS).contains(&regular));
        assert_eq!(boosted_base * 2, regular_base);
    }

    #[test]
    fn only_the_hatch_boost_tag_speeds_an_egg_up() {
        init_vanilla_registry();
        let pos = BlockPos::new(0, 64, 0);

        let on_moss = TestLevel::default()
            .with_block(pos.below(), vanilla_blocks::MOSS_BLOCK.default_state());
        assert!(SnifferEggBlock::hatch_boost(&on_moss, pos));

        let on_stone =
            TestLevel::default().with_block(pos.below(), vanilla_blocks::STONE.default_state());
        assert!(!SnifferEggBlock::hatch_boost(&on_stone, pos));
    }

    #[test]
    fn an_egg_cracks_twice_before_it_is_ready_to_hatch() {
        init_vanilla_registry();
        let mut state = vanilla_blocks::SNIFFER_EGG.default_state();

        assert!(!SnifferEggBlock::is_ready_to_hatch(state));
        state = state.set_value(HATCH, 1u8);
        assert!(!SnifferEggBlock::is_ready_to_hatch(state));
        state = state.set_value(HATCH, MAX_HATCH_LEVEL);
        assert!(SnifferEggBlock::is_ready_to_hatch(state));
    }
}
