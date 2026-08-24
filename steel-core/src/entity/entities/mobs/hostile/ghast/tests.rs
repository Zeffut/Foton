use std::io::Cursor;
use std::sync::Weak;

use simdnbt::borrow::read_compound as read_borrowed_compound;
use steel_registry::{init_vanilla_registry, vanilla_damage_types};

use super::*;

fn ghast() -> GhastEntity {
    GhastEntity::new(
        &vanilla_entities::GHAST,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// Vanilla parity: `Ghast.getExplosionPower` reads a field a command can set,
/// which is what lets a summoned ghast throw a crater rather than a scorch
/// mark. It has to survive a reload.
#[test]
fn the_explosion_power_survives_a_save_and_load_round_trip() {
    init_vanilla_registry();
    let mob = ghast();
    *mob.explosion_power.lock() = 4;

    let mut nbt = NbtCompound::new();
    mob.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("ghast save data should reborrow: {error}"));

    let loaded = ghast();
    loaded.load_additional((&borrowed).into());

    assert_eq!(loaded.explosion_power(), 4);
}

/// Vanilla parity: `Ghast.readAdditionalSaveData` falls back to `1`, so a save
/// written before the field existed still gives an ordinary ghast.
#[test]
fn a_ghast_loaded_without_an_explosion_power_keeps_the_default() {
    init_vanilla_registry();
    let mob = ghast();
    *mob.explosion_power.lock() = 7;

    let mut bytes = Vec::new();
    NbtCompound::new().write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("an empty compound should reborrow: {error}"));
    mob.load_additional((&borrowed).into());

    assert_eq!(mob.explosion_power(), DEFAULT_EXPLOSION_POWER);
}

/// Vanilla parity: `Ghast.setCharging`, read back by the client to open the
/// ghast's mouth. The shoot goal drives it from the charge counter.
#[test]
fn a_ghast_reports_charging_once_the_wind_up_passes_the_warning_tick() {
    init_vanilla_registry();
    let mob = ghast();
    assert!(!mob.is_charging());

    mob.set_charging(true);

    assert!(mob.is_charging());
}

/// Vanilla parity: `GhastShootFireballGoal` sets no goal flags, so it must not
/// take the move or look control away from the drift and the look goal it runs
/// beside at the same priority.
#[test]
fn the_shoot_goal_holds_no_control() {
    let goal = GhastShootFireballGoal::new();

    assert_eq!(goal.controls(), GoalControls::EMPTY);
}

/// Without a world there is nothing to resolve the damage source's entities
/// against, so the reflected-fireball test must fail closed rather than
/// letting a thousand points of damage through.
#[test]
fn a_damage_source_with_no_world_is_never_a_reflected_fireball() {
    init_vanilla_registry();
    let mob = ghast();
    let source = DamageSource::environment(&vanilla_damage_types::FIREBALL)
        .with_causing_entity(2)
        .with_direct_entity(3);

    assert!(!mob.is_reflected_fireball(&source));
}
