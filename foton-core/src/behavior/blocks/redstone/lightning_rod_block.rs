//! Lightning rod behavior.
//!
//! Vanilla parity: `LightningRodBlock` and `WeatheringLightningRodBlock`. The
//! rod is the one block that turns weather into redstone: a bolt that lands on
//! it powers it fully for eight ticks, then a scheduled tick drops it back.
//! Because the rod points the way it was placed, the power it emits directly is
//! one-directional -- only the block the rod's tip points away from gets the
//! strong signal -- while the weak signal leaks to everything touching it, the
//! same asymmetry a lever or an observer has.
//!
//! Not implemented: `animateTick`, the electric-spark particles a rod at the
//! world surface throws during a thunderstorm. Those are client-local and the
//! vanilla client runs them itself.

use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use foton_registry::level_events;
use foton_utils::axis::Axis;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId};

use crate::behavior::block::schedule_water_tick_if_waterlogged;
use crate::behavior::blocks::redstone::{MAX_REDSTONE_SIGNAL, MIN_REDSTONE_SIGNAL};
use crate::behavior::blocks::{WeatherState, WeatheringCopper};
use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::world::{LevelReader, ScheduledTickAccess, SignalQueryContext, World};

/// Which way the rod's tip points.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::FACING;

/// Whether a bolt is currently holding the rod on.
const POWERED: &BoolProperty = &BlockStateProperties::POWERED;

/// Whether the rod is standing in water.
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Ticks a strike keeps the rod powered.
///
/// Vanilla parity: `LightningRodBlock.ACTIVATION_TICKS`.
const ACTIVATION_TICKS: i32 = 8;

/// The seam `LightningBolt.powerLightningRod` needs.
///
/// Vanilla does `stateBelow.getBlock() instanceof LightningRodBlock`, and Foton
/// has no class hierarchy to test, so the two rod behaviors advertise
/// themselves through [`BlockBehavior::as_lightning_rod`] the way bonemealable
/// and rail blocks already do.
pub trait LightningRod {
    /// Powers the rod for [`ACTIVATION_TICKS`] and schedules it back off.
    ///
    /// Vanilla parity: `LightningRodBlock.onLightningStrike`.
    fn on_lightning_strike(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos);
}

/// Behavior for the waxed lightning rods, which never oxidize.
#[block_behavior]
pub struct LightningRodBlock {
    block: BlockRef,
}

impl LightningRodBlock {
    /// Creates a lightning rod behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Wakes the block the rod's tip points away from.
    ///
    /// Vanilla parity: `LightningRodBlock.updateNeighbors`. Vanilla passes an
    /// `ExperimentalRedstoneUtils.initialOrientation`; Foton's neighbor updater
    /// has no orientation value, matching how `ObserverBlock` is ported.
    fn update_neighbors(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let front = state.get_value(FACING).opposite();
        world.update_neighbors_at(pos.relative(front), self.block);
    }

    /// Vanilla parity: `LightningRodBlock.getStateForPlacement`.
    fn placement_state(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        self.block
            .default_state()
            .set_value(FACING, context.clicked_face())
            .set_value(WATERLOGGED, context.is_water_source())
    }

    /// Vanilla parity: `LightningRodBlock.ownSignal`.
    fn own_signal(state: BlockStateId) -> i32 {
        if state.get_value(POWERED) {
            MAX_REDSTONE_SIGNAL
        } else {
            MIN_REDSTONE_SIGNAL
        }
    }

    /// Vanilla parity: `LightningRodBlock.getDirectSignal`.
    fn direct_signal(state: BlockStateId, direction: Direction) -> i32 {
        if state.get_value(POWERED) && state.get_value(FACING) == direction {
            MAX_REDSTONE_SIGNAL
        } else {
            MIN_REDSTONE_SIGNAL
        }
    }

    /// Vanilla parity: `LightningRodBlock.tick`, which drops the power again.
    fn power_off(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        world.set_block(
            pos,
            state.set_value(POWERED, false),
            UpdateFlags::UPDATE_ALL,
        );
        self.update_neighbors(state, world, pos);
    }

    /// Vanilla parity: `LightningRodBlock.onPlace`.
    ///
    /// A rod pasted in already powered -- by a structure, a clone, or a piston
    /// -- would stay on forever without this, because only the scheduled tick
    /// ever clears `POWERED`.
    fn placed(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos, old: BlockStateId) {
        if state.get_block() == old.get_block()
            || !state.get_value(POWERED)
            || world.has_scheduled_block_tick(pos, self.block)
        {
            return;
        }
        world.schedule_block_tick_default(pos, self.block, ACTIVATION_TICKS);
    }

    /// Vanilla parity: `LightningRodBlock.affectNeighborsAfterRemoval`.
    fn removed(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if state.get_value(POWERED) {
            self.update_neighbors(state, world, pos);
        }
    }
}

impl LightningRod for LightningRodBlock {
    fn on_lightning_strike(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        world.set_block(pos, state.set_value(POWERED, true), UpdateFlags::UPDATE_ALL);
        self.update_neighbors(state, world, pos);
        world.schedule_block_tick_default(pos, self.block, ACTIVATION_TICKS);
        world.level_event(
            level_events::PARTICLES_ELECTRIC_SPARK,
            pos,
            axis_ordinal(state.get_value(FACING).get_axis()),
            None,
        );
    }
}

impl BlockBehavior for LightningRodBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.placement_state(context))
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
        self.power_off(state, world, pos);
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        self.placed(state, world, pos, old_state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        self.removed(state, world, pos);
    }

    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    fn get_own_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        Self::own_signal(state)
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        Self::direct_signal(state, direction)
    }

    fn as_lightning_rod(&self) -> Option<&dyn LightningRod> {
        Some(self)
    }
}

/// Behavior for the unwaxed lightning rods, which oxidize over time.
///
/// Vanilla parity: `WeatheringLightningRodBlock`, a `LightningRodBlock` that
/// also implements `WeatheringCopper`.
#[block_behavior]
pub struct WeatheringLightningRodBlock {
    rod: LightningRodBlock,
    #[json_arg(r#enum = "WeatherState", json = "weather_state")]
    weathering: WeatheringCopper,
}

impl WeatheringLightningRodBlock {
    /// Creates a weathering lightning rod behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weather_state: WeatherState) -> Self {
        Self {
            rod: LightningRodBlock::new(block),
            weathering: WeatheringCopper::new(weather_state),
        }
    }
}

impl LightningRod for WeatheringLightningRodBlock {
    fn on_lightning_strike(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.rod.on_lightning_strike(state, world, pos);
    }
}

impl BlockBehavior for WeatheringLightningRodBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.rod.placement_state(context))
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
        self.rod.power_off(state, world, pos);
    }

    /// Vanilla parity: `WeatheringLightningRodBlock.randomTick`.
    ///
    /// Vanilla's `isRandomlyTicking` stops at the oxidized stage; Foton keeps
    /// ticking it and `change_over_time` finds no next stage, which is the same
    /// outcome for one extra lookup.
    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.weathering.change_over_time(state, world, pos);
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        self.rod.placed(state, world, pos, old_state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        self.rod.removed(state, world, pos);
    }

    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    fn get_own_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        LightningRodBlock::own_signal(state)
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        LightningRodBlock::direct_signal(state, direction)
    }

    fn as_lightning_rod(&self) -> Option<&dyn LightningRod> {
        Some(self)
    }
}

/// Returns the level-event payload vanilla sends for the spark particles.
///
/// Vanilla parity: the `getAxis().ordinal()` of `onLightningStrike`. The client
/// reads it back as a `Direction.Axis` ordinal, so the numbering is protocol,
/// not an implementation detail.
const fn axis_ordinal(axis: Axis) -> i32 {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::init_vanilla_registry;
    use foton_registry::item_stack::ItemStack;
    use foton_registry::vanilla_blocks;
    use foton_utils::ChunkPos;

    use super::*;
    use crate::behavior::weathering::next_copper_stage;
    use crate::behavior::{BLOCK_BEHAVIORS, init_behaviors};
    use crate::test_support::{TestLevel, fresh_test_world, insert_ready_full_chunk};

    fn rod_state(block: BlockRef, facing: Direction, powered: bool) -> BlockStateId {
        block
            .default_state()
            .set_value(FACING, facing)
            .set_value(POWERED, powered)
    }

    fn rod_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    #[test]
    fn a_strike_powers_the_rod_and_queues_it_back_off_eight_ticks_later() {
        let world = rod_world("lightning_rod_strike_powers_then_expires");
        let pos = BlockPos::new(4, 64, 4);
        let placed = rod_state(&vanilla_blocks::LIGHTNING_ROD, Direction::Up, false);
        assert!(world.set_block(pos, placed, UpdateFlags::UPDATE_ALL));

        let behavior = BLOCK_BEHAVIORS.get_behavior(&vanilla_blocks::LIGHTNING_ROD);
        let rod = behavior
            .as_lightning_rod()
            .expect("the lightning rod advertises itself as one");
        rod.on_lightning_strike(placed, &world, pos);

        assert!(world.get_block_state(pos).get_value(POWERED));
        assert!(world.has_scheduled_block_tick(pos, &vanilla_blocks::LIGHTNING_ROD));

        behavior.tick(world.get_block_state(pos), &world, pos);
        assert!(!world.get_block_state(pos).get_value(POWERED));
    }

    #[test]
    fn a_powered_rod_leaks_weakly_everywhere_but_powers_only_the_way_it_points() {
        init_vanilla_registry();
        init_behaviors();
        let level = TestLevel::default();
        let pos = BlockPos::new(0, 64, 0);
        let rod = LightningRodBlock::new(&vanilla_blocks::WAXED_LIGHTNING_ROD);
        let state = rod_state(&vanilla_blocks::WAXED_LIGHTNING_ROD, Direction::Up, true);

        for direction in Direction::ALL {
            assert_eq!(
                rod.get_signal(state, &level, pos, direction, SignalQueryContext::DEFAULT),
                15,
                "the weak signal reaches every neighbor"
            );
        }
        assert_eq!(
            rod.get_direct_signal(
                state,
                &level,
                pos,
                Direction::Up,
                SignalQueryContext::DEFAULT
            ),
            15
        );
        assert_eq!(
            rod.get_direct_signal(
                state,
                &level,
                pos,
                Direction::Down,
                SignalQueryContext::DEFAULT
            ),
            0
        );
    }

    #[test]
    fn an_unpowered_rod_emits_nothing_at_all() {
        init_vanilla_registry();
        init_behaviors();
        let level = TestLevel::default();
        let pos = BlockPos::new(0, 64, 0);
        let rod = LightningRodBlock::new(&vanilla_blocks::WAXED_LIGHTNING_ROD);
        let state = rod_state(&vanilla_blocks::WAXED_LIGHTNING_ROD, Direction::Up, false);

        assert_eq!(
            rod.get_signal(
                state,
                &level,
                pos,
                Direction::Up,
                SignalQueryContext::DEFAULT
            ),
            0
        );
        assert_eq!(
            rod.get_direct_signal(
                state,
                &level,
                pos,
                Direction::Up,
                SignalQueryContext::DEFAULT
            ),
            0
        );
    }

    #[test]
    fn placing_a_rod_points_it_at_the_face_that_was_clicked_and_waterlogs_it_in_water() {
        let world = rod_world("lightning_rod_placement_faces_and_waterlogs");
        let dry = BlockPos::new(6, 64, 6);
        let wet = BlockPos::new(7, 64, 6);
        assert!(world.set_block(
            wet,
            vanilla_blocks::WATER.default_state(),
            UpdateFlags::UPDATE_ALL
        ));

        let rod = LightningRodBlock::new(&vanilla_blocks::LIGHTNING_ROD);
        for (pos, face, waterlogged) in [(dry, Direction::North, false), (wet, Direction::Up, true)]
        {
            let mut stack = ItemStack::empty();
            let context =
                BlockPlaceContext::directional(&world, pos, face.opposite(), &mut stack, face);
            let state = rod
                .get_state_for_placement(&context)
                .expect("a rod always has a placement state");
            assert_eq!(state.get_value(FACING), face);
            assert_eq!(state.get_value(WATERLOGGED), waterlogged);
        }
    }

    #[test]
    fn a_waterlogged_rod_keeps_the_water_flowing_when_a_neighbor_changes() {
        init_vanilla_registry();
        init_behaviors();
        let level = TestLevel::default();
        let pos = BlockPos::new(0, 64, 0);
        let rod = LightningRodBlock::new(&vanilla_blocks::LIGHTNING_ROD);
        let state = vanilla_blocks::LIGHTNING_ROD
            .default_state()
            .set_value(WATERLOGGED, true);

        rod.update_shape(
            state,
            &level,
            pos,
            Direction::North,
            pos.relative(Direction::North),
            vanilla_blocks::STONE.default_state(),
        );

        assert!(level.scheduled_water_tick());
    }

    #[test]
    fn every_unwaxed_rod_variant_oxidizes_except_the_last_one() {
        init_vanilla_registry();
        init_behaviors();

        let chain = [
            &vanilla_blocks::LIGHTNING_ROD,
            &vanilla_blocks::EXPOSED_LIGHTNING_ROD,
            &vanilla_blocks::WEATHERED_LIGHTNING_ROD,
            &vanilla_blocks::OXIDIZED_LIGHTNING_ROD,
        ];
        for pair in chain.windows(2) {
            assert_eq!(next_copper_stage(pair[0]), Some(pair[1]));
        }
        assert_eq!(
            next_copper_stage(&vanilla_blocks::OXIDIZED_LIGHTNING_ROD),
            None
        );
        // The waxed rods are outside the chain entirely, which is what keeps a
        // waxed rod looking the way the player left it.
        assert_eq!(
            next_copper_stage(&vanilla_blocks::WAXED_LIGHTNING_ROD),
            None
        );
    }
}
