use std::io::Cursor;
use std::sync::Weak;

use simdnbt::borrow::read_compound as read_borrowed_compound;
use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;

fn endermite() -> EndermiteEntity {
    EndermiteEntity::new(
        &vanilla_entities::ENDERMITE,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// Vanilla parity: the `if (this.life >= 2400) this.discard()` of
/// `Endermite.aiStep`. Two minutes is the whole point of the mob.
#[test]
fn an_endermite_discards_itself_once_its_lifetime_runs_out() {
    init_vanilla_registry();
    let mob = endermite();
    *mob.life.lock() = MAX_LIFE - 1;

    mob.tick_life();

    assert_eq!(mob.life(), MAX_LIFE);
    assert_eq!(mob.removal_reason(), Some(RemovalReason::Discarded));
}

/// Vanilla parity: `if (!this.isPersistenceRequired()) this.life++`. A
/// persistent endermite never ages, so it never reaches the discard.
#[test]
fn a_persistent_endermite_never_ages() {
    init_vanilla_registry();
    let mob = endermite();
    mob.set_persistence_required();

    for _ in 0..MAX_LIFE {
        mob.tick_life();
    }

    assert_eq!(mob.life(), 0);
    assert_eq!(mob.removal_reason(), None);
}

/// Vanilla parity: `Endermite.setYBodyRot`, which drags the yaw along.
#[test]
fn setting_the_body_rotation_snaps_the_yaw_to_match() {
    init_vanilla_registry();
    let mob = endermite();

    mob.set_y_body_rot(123.0);

    assert!((mob.rotation().0 - 123.0).abs() < f32::EPSILON);
    assert!((mob.y_body_rot() - 123.0).abs() < f32::EPSILON);
}

/// The lifetime has to survive a save/load round trip, or every endermite in a
/// reloaded chunk starts its two minutes over.
#[test]
fn the_lifetime_survives_a_save_and_load_round_trip() {
    init_vanilla_registry();
    let mob = endermite();
    *mob.life.lock() = 777;

    let mut nbt = NbtCompound::new();
    mob.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("endermite save data should reborrow: {error}"));

    let loaded = endermite();
    loaded.load_additional((&borrowed).into());

    assert_eq!(loaded.life(), 777);
}
