//! Creaking heart behavior.
//!
//! Vanilla parity: `CreakingHeartBlock`.
//!
//! The heart is the block buried in a pale oak that keeps a creaking alive. Everything the
//! block itself owns is here: the three-state machine (`uprooted` when the pale oak logs
//! that hold it are gone, `dormant` by day, `awake` by night), the axis it is placed on, the
//! twenty-odd experience a naturally generated one drops when a player breaks it, and the
//! comparator output.
//!
//! Not implemented: the creaking. Steel has no `Creaking` entity, so the heart never spawns
//! one, never tears one down, and its comparator therefore always reads zero -- vanilla
//! scales that output by how far the creaking has wandered, and with no creaking vanilla
//! reads zero too. Everything a player can see without a creaking in the world behaves as
//! vanilla does; the moment `Creaking` lands, `CreakingHeartBlockEntity` is where the spawn,
//! the tether and the hurt-transfer belong.

use std::ops::RangeInclusive;
use std::sync::{Arc, Weak};

use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, CreakingHeartState, Direction, EnumProperty,
};
use steel_registry::vanilla_block_entity_types;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::axis::Axis;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::BlockPlaceContext;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::entity::Entity as _;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Vanilla `CreakingHeartBlock.AXIS`.
pub(crate) const AXIS: &EnumProperty<Axis> = &BlockStateProperties::AXIS;
/// Vanilla `CreakingHeartBlock.STATE`.
pub(crate) const STATE: &EnumProperty<CreakingHeartState> =
    &BlockStateProperties::CREAKING_HEART_STATE;
/// Vanilla `CreakingHeartBlock.NATURAL`.
const NATURAL: &BoolProperty = &BlockStateProperties::NATURAL;

/// Vanilla `CreakingHeartBlock.tryAwardExperience`: `nextIntBetweenInclusive(20, 24)`.
const NATURAL_BREAK_EXPERIENCE: RangeInclusive<i32> = 20..=24;

/// Vanilla `CreakingHeartBlock`.
#[block_behavior]
pub struct CreakingHeartBlock {
    block: BlockRef,
}

impl CreakingHeartBlock {
    /// Creates the creaking heart behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `CreakingHeartBlock.hasRequiredLogs`: pale oak logs on both ends of the axis,
    /// laid along the same axis as the heart.
    #[must_use]
    pub fn has_required_logs(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let axis = state.get_value(AXIS);
        axis_directions(axis).into_iter().all(|direction| {
            let neighbor = world.get_block_state(pos.relative(direction));
            neighbor.get_block().has_tag(&BlockTag::PALE_OAK_LOGS)
                && neighbor.try_get_value(AXIS) == Some(axis)
        })
    }

    /// Vanilla `CreakingHeartBlock.updateState`.
    ///
    /// This only ever wakes an uprooted heart that has just found its logs; the walk back
    /// to `uprooted`, and the day-night flip between `dormant` and `awake`, belong to
    /// `CreakingHeartBlockEntity`, which is only ticked once the heart is rooted.
    #[must_use]
    pub fn updated_state(state: BlockStateId, world: &World, pos: BlockPos) -> BlockStateId {
        let uprooted = state.get_value(STATE) == CreakingHeartState::Uprooted;
        if !uprooted || !Self::has_required_logs(state, world, pos) {
            return state;
        }
        state.set_value(STATE, awake_or_dormant(world))
    }
}

/// Vanilla `Direction.Axis.getDirections`, which is `{positive, negative}`.
const fn axis_directions(axis: Axis) -> [Direction; 2] {
    match axis {
        Axis::X => [Direction::East, Direction::West],
        Axis::Y => [Direction::Up, Direction::Down],
        Axis::Z => [Direction::South, Direction::North],
    }
}

/// Vanilla's `EnvironmentAttributes.CREAKING_ACTIVE` branch, shared with the block entity.
pub(crate) fn awake_or_dormant(world: &World) -> CreakingHeartState {
    if world.creaking_active() {
        CreakingHeartState::Awake
    } else {
        CreakingHeartState::Dormant
    }
}

impl BlockBehavior for CreakingHeartBlock {
    /// Vanilla `CreakingHeartBlock.getStateForPlacement`.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let placed = self
            .block
            .default_state()
            .set_value(AXIS, context.clicked_face().get_axis());
        Some(CreakingHeartBlock::updated_state(
            placed,
            context.world.as_ref(),
            context.place_pos(),
        ))
    }

    /// Vanilla `CreakingHeartBlock.updateShape`, which only schedules the state check.
    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        world.schedule_block_tick_default(pos, self.block, 1);
        state
    }

    /// Vanilla `CreakingHeartBlock.tick`.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let updated = CreakingHeartBlock::updated_state(state, world.as_ref(), pos);
        if updated != state {
            world.set_block(pos, updated, UpdateFlags::UPDATE_ALL);
        }
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::CREAKING_HEART,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla `CreakingHeartBlock.getTicker`, which refuses to tick an uprooted heart.
    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        if state.get_value(STATE) == CreakingHeartState::Uprooted {
            return None;
        }
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::CREAKING_HEART,
        )
    }

    /// Vanilla `CreakingHeartBlock.affectNeighborsAfterRemoval`, whose one line is the
    /// `Containers` after-destroy neighbor update: a comparator reading the heart has to
    /// notice that it is gone.
    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        world.update_neighbor_for_output_signal(pos, state.get_block());
    }

    /// Vanilla `CreakingHeartBlock.playerWillDestroy`, minus the creaking it would tear down.
    fn player_will_destroy(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> BlockStateId {
        // Vanilla `Player.preventsBlockDrops` is the creative `instabuild` ability.
        if !player.is_spectator() && !player.has_infinite_materials() && state.get_value(NATURAL) {
            world.pop_experience(pos, rand::rng().random_range(NATURAL_BREAK_EXPERIENCE));
        }
        state
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla `CreakingHeartBlock.getAnalogOutputSignal`, which scales with how far the
    /// creaking has wandered. With no creaking to measure, vanilla reads zero too.
    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn init() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
    }

    fn heart(axis: Axis, state: CreakingHeartState) -> BlockStateId {
        vanilla_blocks::CREAKING_HEART
            .default_state()
            .set_value(AXIS, axis)
            .set_value(STATE, state)
    }

    /// A heart only counts logs that run along its own axis, which is what makes a heart
    /// generated inside a trunk different from one a player wedged between two crossways
    /// logs. Getting the axis check wrong would let any two logs root a heart.
    #[test]
    fn a_heart_is_rooted_only_by_logs_lying_along_its_own_axis() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let world = fresh_test_world("creaking_heart_logs");
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let upright = heart(Axis::Y, CreakingHeartState::Uprooted);
        assert!(!CreakingHeartBlock::has_required_logs(
            upright,
            world.as_ref(),
            pos
        ));

        let upright_log = vanilla_blocks::PALE_OAK_LOG
            .default_state()
            .set_value(AXIS, Axis::Y);
        world.set_block(pos.above(), upright_log, UpdateFlags::UPDATE_NONE);
        assert!(
            !CreakingHeartBlock::has_required_logs(upright, world.as_ref(), pos),
            "one log is not enough; vanilla needs both ends"
        );

        world.set_block(pos.below(), upright_log, UpdateFlags::UPDATE_NONE);
        assert!(CreakingHeartBlock::has_required_logs(
            upright,
            world.as_ref(),
            pos
        ));

        let crossways_log = vanilla_blocks::PALE_OAK_LOG
            .default_state()
            .set_value(AXIS, Axis::X);
        world.set_block(pos.below(), crossways_log, UpdateFlags::UPDATE_NONE);
        assert!(
            !CreakingHeartBlock::has_required_logs(upright, world.as_ref(), pos),
            "a log turned across the heart's axis does not root it"
        );
    }

    /// A heart that lost its logs must fall back to `uprooted` and stop ticking, and one
    /// that finds them must wake. This is the whole observable state machine, so a wrong
    /// transition here is the difference between a working heart and an inert block.
    #[test]
    fn a_heart_uproots_without_logs_and_roots_again_with_them() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let world = fresh_test_world("creaking_heart_state");
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        let behavior = CreakingHeartBlock::new(&vanilla_blocks::CREAKING_HEART);

        let uprooted = heart(Axis::Y, CreakingHeartState::Uprooted);
        world.set_block(pos, uprooted, UpdateFlags::UPDATE_NONE);

        behavior.tick(uprooted, &world, pos);
        assert_eq!(
            world.get_block_state(pos).get_value(STATE),
            CreakingHeartState::Uprooted,
            "a heart with no logs stays uprooted"
        );

        let log = vanilla_blocks::PALE_OAK_LOG
            .default_state()
            .set_value(AXIS, Axis::Y);
        world.set_block(pos.above(), log, UpdateFlags::UPDATE_NONE);
        world.set_block(pos.below(), log, UpdateFlags::UPDATE_NONE);

        behavior.tick(uprooted, &world, pos);
        assert_ne!(
            world.get_block_state(pos).get_value(STATE),
            CreakingHeartState::Uprooted,
            "a heart with logs on both ends roots itself"
        );
    }

    /// An uprooted heart is inert, and vanilla expresses that by refusing it a ticker at
    /// all. Ticking one anyway would spawn a creaking out of a block the player has just
    /// dug free of its tree.
    #[test]
    fn only_a_rooted_heart_is_given_a_ticker() {
        init();
        let world = fresh_test_world("creaking_heart_ticker");
        let behavior = CreakingHeartBlock::new(&vanilla_blocks::CREAKING_HEART);

        assert!(
            behavior
                .get_block_entity_ticker(
                    &world,
                    heart(Axis::Y, CreakingHeartState::Uprooted),
                    &vanilla_block_entity_types::CREAKING_HEART,
                )
                .is_none()
        );
        assert!(
            behavior
                .get_block_entity_ticker(
                    &world,
                    heart(Axis::Y, CreakingHeartState::Dormant),
                    &vanilla_block_entity_types::CREAKING_HEART,
                )
                .is_some()
        );
    }
}
