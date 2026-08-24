use std::sync::Weak;

use steel_registry::init_vanilla_registry;

use super::*;

fn blaze() -> BlazeEntity {
    BlazeEntity::new(
        &vanilla_entities::BLAZE,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// The charge flag shares its byte with nothing today, but vanilla reads it
/// with a mask and the client draws a charged blaze alight, so setting and
/// clearing it must leave the rest of the byte alone.
#[test]
fn charging_a_blaze_toggles_only_its_charge_bit() {
    init_vanilla_registry();
    let mob = blaze();
    mob.entity_data.lock().flags.set(FLAG_CHARGED | 0b0100);

    mob.set_charged(false);

    assert!(!mob.is_charged());
    assert_eq!(*mob.entity_data.lock().flags.get(), 0b0100);

    mob.set_charged(true);

    assert!(mob.is_charged());
    assert_eq!(*mob.entity_data.lock().flags.get(), FLAG_CHARGED | 0b0100);
}

/// Vanilla parity: `Blaze.isOnFire` reports the charge flag, which is what
/// makes a charging blaze burn without ever being on fire.
#[test]
fn a_charged_blaze_reads_as_on_fire() {
    init_vanilla_registry();
    let mob = blaze();

    assert!(!mob.is_on_fire());
    mob.set_charged(true);
    assert!(mob.is_on_fire());
}

/// Vanilla parity: the `multiply(1.0, 0.6, 1.0)` of `Blaze.aiStep`, which only
/// applies while the blaze is falling and off the ground.
#[test]
fn a_falling_blaze_has_its_descent_damped_but_a_rising_one_does_not() {
    init_vanilla_registry();
    let mob = blaze();
    mob.set_on_ground(false);

    mob.set_velocity(DVec3::new(1.0, -1.0, 1.0));
    let _ = mob.ai_step();
    assert!((mob.velocity().y - -FALL_DAMPING).abs() < 1.0e-9);

    mob.set_velocity(DVec3::new(0.0, 1.0, 0.0));
    let _ = mob.ai_step();
    assert!((mob.velocity().y - 1.0).abs() < 1.0e-9);
}

/// Vanilla parity: `Blaze.customServerAiStep` rolls a fresh hover allowance
/// every hundred ticks, and the field starts at the constructor's `0.5F`.
#[test]
fn the_hover_allowance_is_rerolled_on_a_hundred_tick_cycle() {
    init_vanilla_registry();
    let mob = blaze();
    assert!(
        (*mob.allowed_height_offset.lock() - DEFAULT_ALLOWED_HEIGHT_OFFSET).abs() < f32::EPSILON
    );

    mob.tick_hover();

    assert_eq!(
        *mob.next_height_offset_change_tick.lock(),
        HEIGHT_OFFSET_CHANGE_INTERVAL
    );

    for _ in 0..HEIGHT_OFFSET_CHANGE_INTERVAL - 1 {
        mob.tick_hover();
    }
    assert_eq!(*mob.next_height_offset_change_tick.lock(), 1);

    mob.tick_hover();
    assert_eq!(
        *mob.next_height_offset_change_tick.lock(),
        HEIGHT_OFFSET_CHANGE_INTERVAL
    );
}

/// Vanilla parity: the four `setPathfindingMalus` calls of the constructor.
/// The water malus is the one that matters: it is negative, so no path a blaze
/// finds ever crosses water.
#[test]
fn a_blaze_refuses_water_and_ignores_fire_when_pathing() {
    init_vanilla_registry();
    let mob = blaze();
    let malus = mob.mob_base().pathfinding_malus().lock();

    assert!((malus.get(PathType::Water) - -1.0).abs() < f32::EPSILON);
    assert!((malus.get(PathType::Lava) - 8.0).abs() < f32::EPSILON);
    assert!(malus.get(PathType::Fire).abs() < f32::EPSILON);
    assert!(malus.get(PathType::FireInNeighbor).abs() < f32::EPSILON);
}
