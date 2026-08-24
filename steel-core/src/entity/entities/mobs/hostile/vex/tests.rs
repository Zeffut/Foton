use std::io::Cursor;
use std::sync::Weak;

use simdnbt::borrow::read_compound as read_borrowed_compound;
use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;

fn vex() -> VexEntity {
    VexEntity::new(&vanilla_entities::VEX, 1, DVec3::ZERO, Weak::<World>::new())
}

/// Vanilla parity: the `--this.limitedLifeTicks <= 0` of `Vex.tick`. The
/// counter is rearmed rather than cleared, so a spent vex keeps taking a point
/// of damage every twenty ticks until it dies.
#[test]
fn a_spent_vex_starves_on_a_twenty_tick_cycle() {
    init_vanilla_registry();
    let mob = vex();
    mob.set_limited_life(2);

    mob.tick_limited_life();
    assert_eq!(*mob.limited_life_ticks.lock(), Some(1));

    mob.tick_limited_life();
    assert_eq!(
        *mob.limited_life_ticks.lock(),
        Some(LIMITED_LIFE_DEATH_INTERVAL)
    );
}

/// A vex that was never given a lifetime lives forever, which is what
/// distinguishes a summoned vex from one placed by a command.
#[test]
fn a_vex_without_a_borrowed_lifetime_never_starves() {
    init_vanilla_registry();
    let mob = vex();

    for _ in 0..100 {
        mob.tick_limited_life();
    }

    assert_eq!(*mob.limited_life_ticks.lock(), None);
}

/// Vanilla parity: `Vex.setIsCharging`, which the charge goal flips on and off
/// and the client reads to draw the vex's arm back.
#[test]
fn charging_a_vex_toggles_only_its_charge_bit() {
    init_vanilla_registry();
    let mob = vex();
    mob.entity_data.lock().flags.set(FLAG_IS_CHARGING | 0b0010);

    mob.set_is_charging(false);
    assert!(!mob.is_charging());
    assert_eq!(*mob.entity_data.lock().flags.get(), 0b0010);

    mob.set_is_charging(true);
    assert!(mob.is_charging());
    assert_eq!(
        *mob.entity_data.lock().flags.get(),
        FLAG_IS_CHARGING | 0b0010
    );
}

/// The bound origin, the borrowed lifetime and the summoner all have to
/// survive a reload, or a saved vex swarm scatters and never expires.
#[test]
fn the_summoned_state_survives_a_save_and_load_round_trip() {
    init_vanilla_registry();
    let mob = vex();
    let owner = Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
    mob.set_bound_origin(Some(BlockPos::new(-3, 71, 12)));
    mob.set_limited_life(345);
    *mob.owner.lock() = Some(owner);

    let mut nbt = NbtCompound::new();
    mob.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("vex save data should reborrow: {error}"));

    let loaded = vex();
    loaded.load_additional((&borrowed).into());

    assert_eq!(loaded.bound_origin(), Some(BlockPos::new(-3, 71, 12)));
    assert_eq!(*loaded.limited_life_ticks.lock(), Some(345));
    assert_eq!(loaded.owner_uuid(), Some(owner));
}

/// A vex that was never given a lifetime must not gain one from a save that
/// carries no `life_ticks`.
#[test]
fn an_unbounded_vex_stays_unbounded_across_a_round_trip() {
    init_vanilla_registry();
    let mob = vex();

    let mut nbt = NbtCompound::new();
    mob.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("vex save data should reborrow: {error}"));

    let loaded = vex();
    loaded.set_limited_life(10);
    loaded.load_additional((&borrowed).into());

    assert_eq!(*loaded.limited_life_ticks.lock(), None);
    assert_eq!(loaded.bound_origin(), None);
    assert_eq!(loaded.owner_uuid(), None);
}

/// Vanilla parity: `Vex.VexMoveControl.tick` stops rather than overshooting
/// once it is inside its own bounding box, and halves what speed is left.
#[test]
fn a_vex_that_reaches_its_wanted_position_stops_and_halves_its_speed() {
    init_vanilla_registry();
    let mob = vex();
    mob.set_velocity(DVec3::new(1.0, 1.0, 1.0));
    mob.mob_base()
        .controls()
        .lock()
        .move_control
        .set_wanted_position(DVec3::ZERO, 1.0);

    Mob::tick_move_control(&mob);

    assert!(!mob.mob_base().controls().lock().move_control.has_wanted());
    assert!((mob.velocity().y - 0.5).abs() < 1.0e-9);
}

/// Vanilla parity: the else branch of the same method, which accelerates the
/// vex toward its target instead of pathing to it.
#[test]
fn a_vex_accelerates_toward_a_distant_wanted_position() {
    init_vanilla_registry();
    let mob = vex();
    mob.mob_base()
        .controls()
        .lock()
        .move_control
        .set_wanted_position(DVec3::new(0.0, 0.0, 20.0), 1.0);

    Mob::tick_move_control(&mob);

    assert!(mob.mob_base().controls().lock().move_control.has_wanted());
    assert!(mob.velocity().z > 0.0);
}
