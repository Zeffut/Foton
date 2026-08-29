//! Sculk sensor behavior, plain and calibrated.
//!
//! Vanilla parity: `SculkSensorBlock` and `CalibratedSculkSensorBlock`.
//!
//! A vibration reaching the block entity's listener ends in [`activate_sculk_sensor`],
//! which drives the phase machine, the redstone output, the comparator read and the
//! resonance that amethyst beside the sensor re-emits.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, IntProperty, SculkSensorPhase,
};
use foton_registry::fluid::FluidStateExt as _;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types, vanilla_blocks,
    vanilla_game_events,
};
use foton_utils::types::UpdateFlags;
use foton_utils::value_providers::IntProvider;
use foton_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::blocks::NoteBlock;
use crate::behavior::context::BlockPlaceContext;
use crate::behavior::try_drop_experience;
use crate::block_entity::entities::SculkSensorBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::entity::Entity;
use crate::entity::ai::path::PathComputationType;
use crate::fluid::get_fluid_state;
use crate::world::game_event::GameEventContext;
use crate::world::game_event::vibrations::resonance_event_by_frequency;
use crate::world::{
    LevelReader, ScheduledTickAccess, SignalGetter as _, SignalQueryContext, World,
};

/// Vanilla `SculkSensorBlock.ACTIVE_TICKS`.
const ACTIVE_TICKS: i32 = 30;
/// Vanilla `CalibratedSculkSensorBlock.getActiveTicks`.
const CALIBRATED_ACTIVE_TICKS: i32 = 10;
/// Vanilla `SculkSensorBlock.COOLDOWN_TICKS`.
const COOLDOWN_TICKS: i32 = 10;
/// Vanilla `SculkSensorBlock.spawnAfterBreak` uses `ConstantInt.of(5)`.
const SENSOR_EXPERIENCE: IntProvider = IntProvider::Constant(5);
/// Vanilla `SculkSensorBlock.onPlace` clears the power with flags `18`.
const PLACEMENT_POWER_RESET_FLAGS: UpdateFlags =
    UpdateFlags::UPDATE_CLIENTS.union(UpdateFlags::UPDATE_KNOWN_SHAPE);

const PHASE: &EnumProperty<SculkSensorPhase> = &BlockStateProperties::SCULK_SENSOR_PHASE;
const POWER: &IntProperty = &BlockStateProperties::POWER;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Vanilla `SculkSensorBlock.getPhase`.
#[must_use]
pub fn sculk_sensor_phase(state: BlockStateId) -> SculkSensorPhase {
    state.get_value(PHASE)
}

/// Vanilla `SculkSensorBlock.canActivate`.
#[must_use]
pub fn can_activate_sculk_sensor(state: BlockStateId) -> bool {
    sculk_sensor_phase(state) == SculkSensorPhase::Inactive
}

/// Vanilla `SculkSensorBlock.updateNeighbours`.
///
/// The sensor updates the block under it as well as its own neighbors, which is what lets
/// a sensor sitting on a redstone component drive it directly.
fn update_neighbors(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
    let block = state.get_block();
    world.update_neighbors_at(pos, block);
    world.update_neighbors_at(pos.below(), block);
}

/// Vanilla `SculkSensorBlock.deactivate`.
pub fn deactivate_sculk_sensor(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
    world.set_block(
        pos,
        state
            .set_value(PHASE, SculkSensorPhase::Cooldown)
            .set_value(POWER, 0),
        UpdateFlags::UPDATE_ALL,
    );
    world.schedule_block_tick_default(pos, state.get_block(), COOLDOWN_TICKS);
    update_neighbors(world, pos, state);
}

/// Vanilla `SculkSensorBlock.RESONANCE_PITCH_BEND`, kept as the notes it bends to.
///
/// Vanilla derives the sixteen pitches from this tone map through
/// `NoteBlock.getPitchFromNote`; keeping the notes and converting on demand avoids a
/// lazily-initialized float table for sixteen values.
const RESONANCE_TONES: [u8; 16] = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];

/// Vanilla `SculkSensorBlock.tryResonateVibration`.
///
/// Amethyst touching an activating sensor re-emits the vibration at its own frequency,
/// which is how a redstone build filters one sound out of many.
pub fn try_resonate_vibration(
    source_entity: Option<&dyn Entity>,
    world: &Arc<World>,
    pos: BlockPos,
    vibration_frequency: i32,
) {
    let Ok(tone_index) = usize::try_from(vibration_frequency) else {
        return;
    };
    let (Some(tone), Some(event)) = (
        RESONANCE_TONES.get(tone_index),
        resonance_event_by_frequency(vibration_frequency),
    ) else {
        return;
    };

    for direction in Direction::ALL {
        let relative_pos = pos.relative(direction);
        let block_state = world.get_block_state(relative_pos);
        if !REGISTRY
            .blocks
            .is_in_tag(block_state.get_block(), &BlockTag::VIBRATION_RESONATORS)
        {
            continue;
        }

        world.game_event(
            event,
            relative_pos,
            &GameEventContext::new(source_entity, Some(block_state)),
        );
        world.play_block_sound(
            &sound_events::BLOCK_AMETHYST_BLOCK_RESONATE,
            relative_pos,
            1.0,
            NoteBlock::pitch_from_note(*tone),
            None,
        );
    }
}

/// Vanilla `SculkSensorBlock.activate`, reached from the block entity's vibration user.
///
/// Vanilla dispatches on the block instance the sensor's state names. The two sensors differ
/// only in how long they stay active, so the state selects that here instead of a downcast
/// back to the behavior.
pub fn activate_sculk_sensor(
    source_entity: Option<&dyn Entity>,
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
    calculated_power: u8,
    vibration_frequency: i32,
) {
    let active_ticks = if state.get_block() == &vanilla_blocks::CALIBRATED_SCULK_SENSOR {
        CALIBRATED_ACTIVE_TICKS
    } else {
        ACTIVE_TICKS
    };
    activate(
        source_entity,
        world,
        pos,
        state,
        calculated_power,
        vibration_frequency,
        active_ticks,
    );
}

/// Vanilla `SculkSensorBlock.activate`.
fn activate(
    source_entity: Option<&dyn Entity>,
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
    calculated_power: u8,
    vibration_frequency: i32,
    active_ticks: i32,
) {
    world.set_block(
        pos,
        state
            .set_value(PHASE, SculkSensorPhase::Active)
            .set_value(POWER, calculated_power),
        UpdateFlags::UPDATE_ALL,
    );
    world.schedule_block_tick_default(pos, state.get_block(), active_ticks);
    update_neighbors(world, pos, state);
    try_resonate_vibration(source_entity, world, pos, vibration_frequency);
    world.game_event(
        &vanilla_game_events::SCULK_SENSOR_TENDRILS_CLICKING,
        pos,
        &GameEventContext::new(source_entity, None),
    );
    if !state.get_value(WATERLOGGED) {
        world.play_block_sound(
            &sound_events::BLOCK_SCULK_SENSOR_CLICKING,
            pos,
            1.0,
            rand::random::<f32>().mul_add(0.2, 0.8),
            None,
        );
    }
}

/// The half of the two sculk sensors that does not depend on the calibration face.
///
/// Rust has no `extends`, so vanilla's `CalibratedSculkSensorBlock extends SculkSensorBlock`
/// becomes shared state both behaviors delegate to. `active_ticks` is the one value the
/// subclass overrides.
struct SculkSensorCore {
    block: BlockRef,
    active_ticks: i32,
    calibrated: bool,
}

impl SculkSensorCore {
    const fn new(block: BlockRef, active_ticks: i32, calibrated: bool) -> Self {
        Self {
            block,
            active_ticks,
            calibrated,
        }
    }

    /// Vanilla `SculkSensorBlock.tick`.
    ///
    /// The phase machine is identical for both sensors, so this needs nothing off `self`.
    fn tick(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        match sculk_sensor_phase(state) {
            SculkSensorPhase::Active => deactivate_sculk_sensor(world, pos, state),
            SculkSensorPhase::Cooldown => {
                world.set_block(
                    pos,
                    state.set_value(PHASE, SculkSensorPhase::Inactive),
                    UpdateFlags::UPDATE_ALL,
                );
                if !state.get_value(WATERLOGGED) {
                    world.play_block_sound(
                        &sound_events::BLOCK_SCULK_SENSOR_CLICKING_STOP,
                        pos,
                        1.0,
                        rand::random::<f32>().mul_add(0.2, 0.8),
                        None,
                    );
                }
            }
            SculkSensorPhase::Inactive => {}
        }
    }

    /// Vanilla `SculkSensorBlock.activate`.
    fn activate(
        &self,
        source_entity: Option<&dyn Entity>,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        calculated_power: u8,
        vibration_frequency: i32,
    ) {
        activate(
            source_entity,
            world,
            pos,
            state,
            calculated_power,
            vibration_frequency,
            self.active_ticks,
        );
    }

    /// Vanilla `SculkSensorBlock.onPlace`.
    ///
    /// A sensor pasted in powered -- by a structure, a command or a schematic -- has no
    /// vibration behind that power, so vanilla clears it unless a tick is already pending.
    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
    ) {
        if state.get_block() == old_state.get_block()
            || state.get_value(POWER) == 0
            || world.has_scheduled_block_tick(pos, self.block)
        {
            return;
        }

        world.set_block(pos, state.set_value(POWER, 0), PLACEMENT_POWER_RESET_FLAGS);
    }

    /// Vanilla `SculkSensorBlock.getStateForPlacement`.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        let waterlogged = get_fluid_state(context.world, context.place_pos()).is_water();
        let state = self
            .block
            .default_state()
            .set_value(WATERLOGGED, waterlogged);
        if self.calibrated {
            // Vanilla `CalibratedSculkSensorBlock.getStateForPlacement` faces the player.
            return state.set_value(FACING, context.horizontal_direction());
        }
        state
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        let block_entity_type = if self.calibrated {
            &vanilla_block_entity_types::CALIBRATED_SCULK_SENSOR
        } else {
            &vanilla_block_entity_types::SCULK_SENSOR
        };
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            block_entity_type,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla `SculkSensorBlock.getTicker`, which only ticks the vibration system.
    fn block_entity_ticker(
        &self,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        let expected = if self.calibrated {
            &vanilla_block_entity_types::CALIBRATED_SCULK_SENSOR
        } else {
            &vanilla_block_entity_types::SCULK_SENSOR
        };
        BlockEntityTicker::for_matching_entity_tick(block_entity_type, expected)
    }

    /// Vanilla `SculkSensorBlock.getAnalogOutputSignal`.
    fn analog_output_signal(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> i32 {
        if sculk_sensor_phase(state) != SculkSensorPhase::Active {
            return 0;
        }

        world
            .get_block_entity(pos)
            .and_then(|block_entity| {
                block_entity
                    .downcast_ref::<SculkSensorBlockEntity>()
                    .map(SculkSensorBlockEntity::last_vibration_frequency)
            })
            .unwrap_or(0)
    }
}

/// Vanilla `SculkSensorBlock`.
#[block_behavior]
pub struct SculkSensorBlock {
    core: SculkSensorCore,
}

impl SculkSensorBlock {
    /// Creates the plain sculk sensor behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            core: SculkSensorCore::new(block, ACTIVE_TICKS, false),
        }
    }

    /// Vanilla `SculkSensorBlock.activate`.
    ///
    /// Nothing in Foton calls this yet: it is the entry point a vibration listener uses
    /// once one exists. It is kept whole so the phase machine, the resonance and the
    /// redstone edge are already correct when that listener arrives.
    pub fn activate(
        &self,
        source_entity: Option<&dyn Entity>,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        calculated_power: u8,
        vibration_frequency: i32,
    ) {
        self.core.activate(
            source_entity,
            world,
            pos,
            state,
            calculated_power,
            vibration_frequency,
        );
    }
}

impl BlockBehavior for SculkSensorBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.core.get_state_for_placement(context))
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        SculkSensorCore::tick(state, world, pos);
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        self.core.on_place(state, world, pos, old_state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        if sculk_sensor_phase(state) == SculkSensorPhase::Active {
            update_neighbors(world, pos, state);
        }
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

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.core.new_block_entity(level, pos, state)
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        self.core.block_entity_ticker(block_entity_type)
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
        state.get_value(POWER).into()
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
        context: SignalQueryContext,
    ) -> i32 {
        if direction == Direction::Up {
            self.get_signal(state, world, pos, direction, context)
        } else {
            0
        }
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        SculkSensorCore::analog_output_signal(state, world, pos)
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            try_drop_experience(world, pos, tool, &SENSOR_EXPERIENCE);
        }
    }
}

/// Vanilla `CalibratedSculkSensorBlock`.
///
/// Not implemented: `rotate` and `mirror`. Foton resolves block rotation from extracted
/// state data rather than a behavior hook, so there is nothing here to override.
#[block_behavior]
pub struct CalibratedSculkSensorBlock {
    core: SculkSensorCore,
}

impl CalibratedSculkSensorBlock {
    /// Creates the calibrated sculk sensor behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            core: SculkSensorCore::new(block, CALIBRATED_ACTIVE_TICKS, true),
        }
    }

    /// Vanilla `SculkSensorBlock.activate`, with the calibrated sensor's shorter
    /// ten-tick active window.
    ///
    /// As with the plain sensor, nothing calls this until Foton has vibrations.
    pub fn activate(
        &self,
        source_entity: Option<&dyn Entity>,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        calculated_power: u8,
        vibration_frequency: i32,
    ) {
        self.core.activate(
            source_entity,
            world,
            pos,
            state,
            calculated_power,
            vibration_frequency,
        );
    }

    /// Vanilla `CalibratedSculkSensorBlockEntity.VibrationUser.getBackSignal`.
    ///
    /// The redstone strength entering the back face is the frequency the sensor listens
    /// for. Foton measures it here so the calibration is already readable when a vibration
    /// system arrives.
    #[must_use]
    pub fn back_signal(world: &Arc<World>, pos: BlockPos, state: BlockStateId) -> i32 {
        let direction = state.get_value(FACING).opposite();
        world
            .as_ref()
            .get_signal(pos.relative(direction), direction)
    }
}

impl BlockBehavior for CalibratedSculkSensorBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.core.get_state_for_placement(context))
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        SculkSensorCore::tick(state, world, pos);
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        self.core.on_place(state, world, pos, old_state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        if sculk_sensor_phase(state) == SculkSensorPhase::Active {
            update_neighbors(world, pos, state);
        }
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

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.core.new_block_entity(level, pos, state)
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        self.core.block_entity_ticker(block_entity_type)
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
        state.get_value(POWER).into()
    }

    /// Vanilla `CalibratedSculkSensorBlock.getSignal`.
    ///
    /// The calibration face is an input, so the sensor must not push its own output back
    /// out of it, or a calibrated sensor would feed itself.
    fn get_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
        context: SignalQueryContext,
    ) -> i32 {
        if direction == state.get_value(FACING) {
            0
        } else {
            self.get_own_signal(state, world, pos, context)
        }
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
        context: SignalQueryContext,
    ) -> i32 {
        if direction == Direction::Up {
            self.get_signal(state, world, pos, direction, context)
        } else {
            0
        }
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        SculkSensorCore::analog_output_signal(state, world, pos)
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            try_drop_experience(world, pos, tool, &SENSOR_EXPERIENCE);
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use foton_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// Block states are registry lookups, so nothing in a test may name one before this.
    fn init() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
    }

    /// Puts `state` in a real world without letting `onPlace` see a block change.
    ///
    /// The sensor clears stray power the moment it is placed, so a test that wants a
    /// pre-powered sensor has to place the plain one first and then swap the state.
    fn world_with_sensor(name: &'static str, pos: BlockPos, state: BlockStateId) -> Arc<World> {
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        assert!(world.set_block(
            pos,
            state.get_block().default_state(),
            UpdateFlags::UPDATE_ALL
        ));
        // The second write is a no-op when `state` already is the default, and
        // `set_block` reports that as "nothing changed" rather than as a failure.
        let _ = world.set_block(pos, state, UpdateFlags::UPDATE_ALL);
        assert_eq!(world.get_block_state(pos), state);
        world
    }

    fn sensor_state(phase: SculkSensorPhase, power: u8) -> BlockStateId {
        vanilla_blocks::SCULK_SENSOR
            .default_state()
            .set_value(PHASE, phase)
            .set_value(POWER, power)
    }

    /// The whole point of the cooldown phase is that a sensor cannot fire twice in a
    /// row: it drops its power the moment the active window ends, and only ten ticks
    /// later can it hear anything again.
    #[test]
    fn an_active_sensor_drops_its_power_before_it_can_listen_again() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let active = sensor_state(SculkSensorPhase::Active, 9);
        let world = world_with_sensor("sculk_sensor_cooldown", pos, active);
        let behavior = SculkSensorBlock::new(&vanilla_blocks::SCULK_SENSOR);

        behavior.tick(active, &world, pos);

        let cooling = world.get_block_state(pos);
        assert_eq!(sculk_sensor_phase(cooling), SculkSensorPhase::Cooldown);
        assert_eq!(cooling.get_value(POWER), 0);
        assert!(!can_activate_sculk_sensor(cooling));
        assert!(world.has_scheduled_block_tick(pos, &vanilla_blocks::SCULK_SENSOR));

        behavior.tick(cooling, &world, pos);

        let idle = world.get_block_state(pos);
        assert_eq!(sculk_sensor_phase(idle), SculkSensorPhase::Inactive);
        assert!(can_activate_sculk_sensor(idle));
    }

    /// `activate` is the entry point a vibration system will call. It has to leave the
    /// sensor powered, active, and scheduled to switch itself off, or one vibration
    /// would latch the output on forever.
    #[test]
    fn activating_a_sensor_powers_it_and_schedules_its_own_shutdown() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let idle = sensor_state(SculkSensorPhase::Inactive, 0);
        let world = world_with_sensor("sculk_sensor_activate", pos, idle);
        let behavior = SculkSensorBlock::new(&vanilla_blocks::SCULK_SENSOR);

        behavior.activate(None, &world, pos, idle, 9, 3);

        let active = world.get_block_state(pos);
        assert_eq!(sculk_sensor_phase(active), SculkSensorPhase::Active);
        assert_eq!(active.get_value(POWER), 9);
        assert!(world.has_scheduled_block_tick(pos, &vanilla_blocks::SCULK_SENSOR));
    }

    /// A sensor pasted in already powered -- by a structure or a `/setblock` -- has no
    /// vibration behind that power. Vanilla clears it so the sensor does not sit there
    /// driving redstone with nothing left to switch it off.
    #[test]
    fn a_sensor_placed_already_powered_loses_that_power() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let world = fresh_test_world("sculk_sensor_stray_power");
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        let powered = sensor_state(SculkSensorPhase::Active, 12);
        assert!(world.set_block(pos, powered, UpdateFlags::UPDATE_ALL));

        SculkSensorBlock::new(&vanilla_blocks::SCULK_SENSOR).on_place(
            powered,
            &world,
            pos,
            vanilla_blocks::AIR.default_state(),
            false,
        );

        assert_eq!(world.get_block_state(pos).get_value(POWER), 0);
    }

    /// Only the block above a sensor is powered strongly; everything else sees the weak
    /// signal. Getting this wrong would let a sensor power a block through its own side.
    #[test]
    fn only_the_block_above_a_sensor_is_powered_strongly() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let active = sensor_state(SculkSensorPhase::Active, 9);
        let world = world_with_sensor("sculk_sensor_direct_signal", pos, active);
        let behavior = SculkSensorBlock::new(&vanilla_blocks::SCULK_SENSOR);
        let level: &World = world.as_ref();

        assert_eq!(
            behavior.get_direct_signal(
                active,
                level,
                pos,
                Direction::Up,
                SignalQueryContext::DEFAULT
            ),
            9
        );
        assert_eq!(
            behavior.get_direct_signal(
                active,
                level,
                pos,
                Direction::North,
                SignalQueryContext::DEFAULT
            ),
            0
        );
        assert_eq!(
            behavior.get_own_signal(active, level, pos, SignalQueryContext::DEFAULT),
            9
        );
    }

    /// A comparator beside a sensor reads the frequency of what it heard, but only while
    /// the sensor is still active -- otherwise the comparator would keep reporting the
    /// last sound long after the sensor went quiet.
    #[test]
    fn a_comparator_reads_the_frequency_only_while_the_sensor_is_active() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let active = sensor_state(SculkSensorPhase::Active, 9);
        let world = world_with_sensor("sculk_sensor_comparator", pos, active);
        let behavior = SculkSensorBlock::new(&vanilla_blocks::SCULK_SENSOR);

        let block_entity = world
            .get_block_entity(pos)
            .expect("a sculk sensor carries a block entity");
        block_entity
            .downcast_ref::<SculkSensorBlockEntity>()
            .expect("the sensor's block entity is sculk sensor storage")
            .set_last_vibration_frequency(11);

        let level: &World = world.as_ref();
        assert_eq!(
            behavior.get_analog_output_signal(active, level, pos, Direction::Up),
            11
        );
        assert_eq!(
            behavior.get_analog_output_signal(
                sensor_state(SculkSensorPhase::Inactive, 0),
                level,
                pos,
                Direction::Up,
            ),
            0
        );
    }

    /// The calibration face is an input. If the sensor pushed its own output back out of
    /// it, a calibrated sensor would drive its own filter.
    #[test]
    fn a_calibrated_sensor_never_pushes_output_out_of_its_calibration_face() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let active = vanilla_blocks::CALIBRATED_SCULK_SENSOR
            .default_state()
            .set_value(PHASE, SculkSensorPhase::Active)
            .set_value(POWER, 7)
            .set_value(FACING, Direction::North);
        let world = world_with_sensor("calibrated_sculk_sensor_signal", pos, active);
        let behavior = CalibratedSculkSensorBlock::new(&vanilla_blocks::CALIBRATED_SCULK_SENSOR);
        let level: &World = world.as_ref();

        assert_eq!(
            behavior.get_signal(
                active,
                level,
                pos,
                Direction::North,
                SignalQueryContext::DEFAULT
            ),
            0
        );
        assert_eq!(
            behavior.get_signal(
                active,
                level,
                pos,
                Direction::South,
                SignalQueryContext::DEFAULT
            ),
            7
        );
    }

    /// The redstone strength entering the back of a calibrated sensor is the frequency it
    /// listens for, so it has to come from the block behind the calibration face rather
    /// than from the face itself.
    #[test]
    fn a_calibrated_sensor_reads_its_filter_from_the_block_behind_it() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let facing_north = vanilla_blocks::CALIBRATED_SCULK_SENSOR
            .default_state()
            .set_value(FACING, Direction::North);
        let world = world_with_sensor("calibrated_sculk_sensor_back", pos, facing_north);
        assert_eq!(
            CalibratedSculkSensorBlock::back_signal(&world, pos, facing_north),
            0
        );

        assert!(world.set_block(
            pos.south(),
            vanilla_blocks::REDSTONE_BLOCK.default_state(),
            UpdateFlags::UPDATE_ALL
        ));
        assert_eq!(
            CalibratedSculkSensorBlock::back_signal(&world, pos, facing_north),
            15
        );
    }
}
