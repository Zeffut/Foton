//! The vibration layer driven through a real world and a real sculk sensor.
//!
//! The three things that separate a vibration from a game event are the travel delay, the
//! one-vibration-per-tick selection and the occlusion test. Each is checked here against a
//! live sensor rather than against the listener alone, because the wiring between the chunk
//! registry, the block-entity ticker and the block behavior is where the layer can be
//! present and still do nothing.

use std::sync::Arc;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, SculkSensorPhase};
use steel_registry::game_events::GameEventRef;
use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_game_events};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos, Direction, Downcast as _};

use super::*;
use crate::behavior::blocks::sculk_sensor_phase;
use crate::behavior::init_behaviors;
use crate::block_entity::entities::SculkSensorBlockEntity;
use crate::block_entity::init_block_entities;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Where the sensor sits. Its own chunk and the eight around it have to be block-ticking,
/// because a sensor refuses a vibration whose destination is on the edge of the world.
const SENSOR: BlockPos = BlockPos::new(8, 64, 8);

fn sensor_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    for x in -1..=1 {
        for z in -1..=1 {
            insert_ready_full_chunk(&world, ChunkPos::new(x, z));
        }
    }
    assert!(world.set_block(
        SENSOR,
        vanilla_blocks::SCULK_SENSOR.default_state(),
        UpdateFlags::UPDATE_ALL,
    ));
    world
}

fn sensor_listener(world: &Arc<World>) -> Arc<VibrationListener> {
    let block_entity = world
        .get_block_entity(SENSOR)
        .expect("placing a sculk sensor creates its block entity");
    let sensor = block_entity
        .downcast_ref::<SculkSensorBlockEntity>()
        .expect("the sculk sensor block entity is the one that was created");
    Arc::clone(sensor.listener())
}

fn last_frequency(world: &Arc<World>) -> i32 {
    world
        .get_block_entity(SENSOR)
        .and_then(|block_entity| {
            block_entity
                .downcast_ref::<SculkSensorBlockEntity>()
                .map(SculkSensorBlockEntity::last_vibration_frequency)
        })
        .expect("the sculk sensor block entity is still there")
}

fn advance_game_time(world: &Arc<World>, ticks: i64) {
    let now = world.game_time();
    world.level_data.write().set_game_time(now + ticks);
}

/// Runs the sensor's block-entity ticker, the way the world ticker would.
fn tick_sensor(world: &Arc<World>) {
    let block_entity = world
        .get_block_entity(SENSOR)
        .expect("the sensor block entity outlives the tick");
    block_entity.tick(world);
    advance_game_time(world, 1);
}

fn is_active(world: &Arc<World>) -> bool {
    sculk_sensor_phase(world.get_block_state(SENSOR)) == SculkSensorPhase::Active
}

fn emit(world: &Arc<World>, event: GameEventRef, pos: BlockPos) {
    world.game_event(event, pos, &GameEventContext::new(None, None));
}

/// This is the whole point of the layer: a sensor three blocks from a footstep does not fire
/// on the tick it hears it, it fires three ticks later. Without the delay a sculk sensor is
/// a pressure plate with a bigger radius.
#[test]
fn a_sensor_fires_one_tick_per_block_after_it_hears_a_step() {
    let world = sensor_world("vibration_travel_delay");
    emit(&world, &vanilla_game_events::STEP, BlockPos::new(11, 64, 8));
    advance_game_time(&world, 1);

    // The tick that selects the candidate also starts its three-tick journey.
    tick_sensor(&world);
    assert!(
        !is_active(&world),
        "the vibration has not arrived on the tick it was selected"
    );
    assert_eq!(sensor_listener(&world).travel_time_in_ticks(), 2);

    tick_sensor(&world);
    assert!(!is_active(&world), "still one block of travel to go");

    tick_sensor(&world);
    assert!(is_active(&world), "three blocks away means three ticks");
    assert_eq!(
        last_frequency(&world),
        1,
        "a step is frequency one, which is what a comparator would read"
    );
    assert_eq!(
        world
            .get_block_state(SENSOR)
            .get_value(&BlockStateProperties::POWER),
        redstone_strength_for_distance(3.0, 8),
        "the redstone output encodes how far away the step was"
    );
}

/// A tick full of events produces exactly one vibration, and it is the nearest one. A sensor
/// that took the first event dispatched would be at the mercy of chunk iteration order.
#[test]
fn only_the_nearest_of_a_tick_of_events_becomes_a_vibration() {
    let world = sensor_world("vibration_one_per_tick");
    emit(&world, &vanilla_game_events::STEP, BlockPos::new(14, 64, 8));
    emit(&world, &vanilla_game_events::STEP, BlockPos::new(9, 64, 8));
    emit(&world, &vanilla_game_events::STEP, BlockPos::new(12, 64, 8));
    advance_game_time(&world, 1);

    tick_sensor(&world);
    assert_eq!(
        sensor_listener(&world).travel_time_in_ticks(),
        0,
        "the block-away step won, so its one tick of travel is already spent"
    );

    tick_sensor(&world);
    assert!(is_active(&world));
    assert_eq!(
        world
            .get_block_state(SENSOR)
            .get_value(&BlockStateProperties::POWER),
        redstone_strength_for_distance(1.0, 8),
        "the power says one block, not four"
    );
}

/// Between two events the same distance away, the louder one wins. This is the tiebreak the
/// selector applies after distance, and it is what lets a redstone build hear a chest open
/// over a footstep that landed at the same range in the same tick.
#[test]
fn a_distance_tie_is_broken_by_the_louder_event() {
    let world = sensor_world("vibration_frequency_tiebreak");
    emit(&world, &vanilla_game_events::STEP, BlockPos::new(11, 64, 8));
    emit(
        &world,
        &vanilla_game_events::CONTAINER_OPEN,
        BlockPos::new(5, 64, 8),
    );
    advance_game_time(&world, 1);

    for _ in 0..4 {
        tick_sensor(&world);
    }

    assert!(is_active(&world));
    assert_eq!(
        last_frequency(&world),
        game_event_frequency(&vanilla_game_events::CONTAINER_OPEN),
        "both were three blocks away, so the higher frequency was chosen"
    );
}

/// Wool between the source and the sensor deafens it, and it has to be wool the vibration
/// cannot get around: the occlusion test only reports a block when all six rays out of the
/// source block are stopped.
#[test]
fn wool_sealing_the_source_stops_the_vibration_but_one_wool_block_does_not() {
    let world = sensor_world("vibration_occlusion");
    let source = BlockPos::new(11, 64, 8);

    assert!(world.set_block(
        source.above(),
        vanilla_blocks::WHITE_WOOL.default_state(),
        UpdateFlags::UPDATE_ALL,
    ));
    emit(&world, &vanilla_game_events::STEP, source);
    advance_game_time(&world, 1);
    for _ in 0..4 {
        tick_sensor(&world);
    }
    assert!(
        is_active(&world),
        "a single wool block beside the source leaves five clear rays"
    );

    let world = sensor_world("vibration_occlusion_sealed");
    for direction in Direction::ALL {
        assert!(world.set_block(
            source.relative(direction),
            vanilla_blocks::WHITE_WOOL.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));
    }
    emit(&world, &vanilla_game_events::STEP, source);
    advance_game_time(&world, 1);
    for _ in 0..4 {
        tick_sensor(&world);
    }
    assert!(
        !is_active(&world),
        "wool on all six sides stops every ray, so the sensor never hears it"
    );
}

/// An active sensor is deaf until it has cooled down. Vanilla checks that at the moment the
/// event is heard, not at the moment it arrives, so a second step during the active phase
/// must never even be scheduled.
#[test]
fn an_active_sensor_does_not_schedule_a_second_vibration() {
    let world = sensor_world("vibration_active_is_deaf");
    emit(&world, &vanilla_game_events::STEP, BlockPos::new(9, 64, 8));
    advance_game_time(&world, 1);
    tick_sensor(&world);
    tick_sensor(&world);
    assert!(is_active(&world));

    emit(
        &world,
        &vanilla_game_events::CONTAINER_OPEN,
        BlockPos::new(9, 64, 8),
    );
    advance_game_time(&world, 1);
    tick_sensor(&world);
    assert!(
        !sensor_listener(&world).has_vibration_in_flight(),
        "the sensor was already active, so nothing was scheduled"
    );
    assert_eq!(
        last_frequency(&world),
        1,
        "the frequency still reports the step it actually heard"
    );
}

/// A vibration that was still travelling when the chunk unloaded has to come back with its
/// remaining delay. Dropping it would silently swallow the event; dropping the delay would
/// make it arrive instantly on load.
#[test]
fn a_vibration_in_flight_survives_a_save_and_load() {
    use simdnbt::borrow::{
        NbtCompound as NbtCompoundView, read_compound as read_borrowed_compound,
    };
    use simdnbt::owned::NbtCompound;
    use std::io::Cursor;

    let world = sensor_world("vibration_save_in_flight");
    emit(&world, &vanilla_game_events::STEP, BlockPos::new(14, 64, 8));
    advance_game_time(&world, 1);
    tick_sensor(&world);
    let listener = sensor_listener(&world);
    assert_eq!(listener.travel_time_in_ticks(), 5);

    let mut saved = NbtCompound::new();
    listener.save(&mut saved);
    let mut bytes = Vec::new();
    saved.write(&mut bytes);
    let borrowed =
        read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");
    let reloaded = VibrationListener::new(Arc::clone(listener.user()));
    let view: NbtCompoundView<'_, '_> = (&borrowed).into();
    reloaded.load(Some(&view));

    assert!(reloaded.has_vibration_in_flight());
    assert_eq!(reloaded.travel_time_in_ticks(), 5);
}

/// The frequency table is what a comparator on a sculk sensor reads, and what the
/// selector uses to break a distance tie. A shifted table would give every redstone
/// build the wrong filter, so the two ends and one middle row are pinned.
#[test]
fn every_vibration_carries_its_vanilla_frequency() {
    init_vanilla_registry();
    assert_eq!(game_event_frequency(&vanilla_game_events::STEP), 1);
    assert_eq!(
        game_event_frequency(&vanilla_game_events::CONTAINER_OPEN),
        10
    );
    assert_eq!(game_event_frequency(&vanilla_game_events::EXPLODE), 15);
    assert_eq!(
        game_event_frequency(&vanilla_game_events::SHRIEK),
        NO_VIBRATION_FREQUENCY,
        "a shriek is a game event the shrieker emits, not one a sensor measures"
    );
}

/// Resonance re-emits a vibration at the frequency amethyst measured, so the index has
/// to be one-based the way vanilla's list is.
#[test]
fn resonance_events_are_indexed_from_frequency_one() {
    init_vanilla_registry();
    assert!(resonance_event_by_frequency(0).is_none());
    assert_eq!(
        resonance_event_by_frequency(1),
        Some(&vanilla_game_events::RESONATE_1)
    );
    assert_eq!(
        resonance_event_by_frequency(15),
        Some(&vanilla_game_events::RESONATE_15)
    );
    assert!(resonance_event_by_frequency(16).is_none());
    assert_eq!(
        game_event_frequency(&vanilla_game_events::RESONATE_7),
        7,
        "a resonated vibration keeps the frequency it was resonated at"
    );
}

/// The redstone output is how far away the vibration was, inverted. A sensor must never
/// answer zero while it is active, or a comparator could not tell it from an idle one.
#[test]
fn redstone_strength_falls_off_with_distance_but_never_reaches_zero() {
    assert_eq!(redstone_strength_for_distance(0.0, 8), 15);
    assert_eq!(redstone_strength_for_distance(1.0, 8), 14);
    assert_eq!(redstone_strength_for_distance(4.0, 8), 8);
    assert_eq!(redstone_strength_for_distance(8.0, 8), 1);
    assert_eq!(redstone_strength_for_distance(100.0, 8), 1);
    assert_eq!(
        redstone_strength_for_distance(8.0, 16),
        8,
        "a calibrated sensor hears twice as far, so the same distance reads stronger"
    );
}
