use std::io::Cursor;
use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;
use crate::entity::entities::{HorseEntity, PigEntity};

fn happy_ghast() -> HappyGhastEntity {
    HappyGhastEntity::new(
        &vanilla_entities::HAPPY_GHAST,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// Vanilla parity: `HappyGhast.ageBoundaryReached`, which swaps the whole
/// control set. A ghastling is a brain mob with no goals at all; the adult it
/// grows into is a goal mob. Nothing else in the mob notices the change, so if
/// the swap is not reached a ghastling keeps flying an adult's goals.
#[test]
fn growing_up_and_back_down_swaps_the_goal_set() {
    init_vanilla_registry();
    let ghast = happy_ghast();

    let adult_goals = ghast.mob_base.goal_selector().lock().goal_count();
    assert!(adult_goals > 0, "a happy ghast is born adult, with goals");

    // Through `set_age`, which is the path a save load and a growth spurt both
    // take -- not through `age_boundary_changed` directly.
    ghast.set_age(-1);
    assert!(AgeableMob::is_baby(&ghast));
    assert_eq!(ghast.mob_base.goal_selector().lock().goal_count(), 0);

    ghast.set_age(0);
    assert!(!AgeableMob::is_baby(&ghast));
    assert_eq!(
        ghast.mob_base.goal_selector().lock().goal_count(),
        adult_goals
    );
}

/// Vanilla parity: `HappyGhast.notifyLeashHolder`, which arms the flag only for
/// a leashable that hangs four ropes. A pig on one lead leaves the harness's
/// rope anchors dark.
#[test]
fn only_a_quad_leashable_arms_the_leash_holder_flag() {
    init_vanilla_registry();
    let ghast = happy_ghast();
    let horse = HorseEntity::new(&vanilla_entities::HORSE, 2, DVec3::ZERO, Weak::new());
    let pig = PigEntity::new(&vanilla_entities::PIG, 3, DVec3::ZERO, Weak::new());

    ghast.notify_leash_holder(&pig);
    assert_eq!(*ghast.leash_holder_time.lock(), 0);

    ghast.notify_leash_holder(&horse);
    assert_eq!(*ghast.leash_holder_time.lock(), LEASH_HOLDER_TIME);
}

/// Vanilla parity: the `Mth.wrapDegrees90` branch of
/// `HappyGhast.HappyGhastLookControl.tick`. A happy ghast holding still for its
/// riders squares up with the world so the platform they stand on stops
/// drifting under them.
#[test]
fn holding_still_squares_the_ghast_up_with_the_world() {
    init_vanilla_registry();
    let ghast = happy_ghast();
    ghast.set_rotation((100.0, 0.0));

    // Not squared while it is free to look around.
    ghast.tick_look_control();
    assert!((ghast.rotation().0 - 100.0).abs() > f32::EPSILON);

    ghast.set_rotation((100.0, 0.0));
    ghast.set_server_still_timeout(MAX_STILL_TIMEOUT);
    ghast.tick_look_control();

    assert!((ghast.rotation().0 - 90.0).abs() < 1.0e-4);
    assert!((ghast.y_head_rot() - 90.0).abs() < 1.0e-4);
}

/// Vanilla parity: `HappyGhast.addAdditionalSaveData`. The still timeout is the
/// only field of its own a happy ghast writes, and a ghast that forgot it would
/// come back from a reload free to drift out from under whoever was standing on
/// it.
#[test]
fn the_still_timeout_survives_a_save_and_load_round_trip() {
    init_vanilla_registry();
    let ghast = happy_ghast();
    ghast.set_server_still_timeout(MAX_STILL_TIMEOUT);

    let mut nbt = NbtCompound::new();
    ghast.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("happy ghast save data should reborrow: {error}"));

    let loaded = happy_ghast();
    loaded.load_additional((&borrowed).into());

    assert!(loaded.is_on_still_timeout());
    assert!(loaded.stays_still());
}
