use std::io::Cursor;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use steel_registry::{init_vanilla_registry, vanilla_cat_variants, vanilla_entities};

use super::*;

fn cat() -> CatEntity {
    init_vanilla_registry();
    CatEntity::new(&vanilla_entities::CAT, 1, DVec3::ZERO, Weak::new())
}

/// Vanilla parity: `Cat.canMate`. A stray cat never breeds, however in love it
/// is, and that is the whole reason cat farms need two tamed cats.
#[test]
fn only_two_tamed_cats_may_mate() {
    init_vanilla_registry();
    let first = CatEntity::new(&vanilla_entities::CAT, 1, DVec3::ZERO, Weak::new());
    let second = CatEntity::new(&vanilla_entities::CAT, 2, DVec3::ZERO, Weak::new());
    first.set_in_love_time(600);
    second.set_in_love_time(600);

    assert!(!Animal::can_mate(&first, &second));

    first.set_tame(true, false);
    assert!(!Animal::can_mate(&first, &second));

    second.set_tame(true, false);
    assert!(Animal::can_mate(&first, &second));
}

/// Vanilla parity: `Cat.removeWhenFarAway`. A stray cat is a temporary spawn
/// and a tamed one must survive the despawn sweep forever.
#[test]
fn only_a_stray_cat_despawns_when_it_gets_old() {
    let cat = cat();

    for _ in 0..=STRAY_DESPAWN_AGE_TICKS {
        cat.advance_tick_count();
    }
    assert!(Mob::remove_when_far_away(&cat, 4096.0));

    cat.set_tame(true, false);
    assert!(!Mob::remove_when_far_away(&cat, 4096.0));
}

/// The variant, the sound variant and the collar all have to survive a save,
/// or a tamed cat comes back a different cat.
#[test]
fn a_cat_keeps_its_variant_and_collar_through_a_save() {
    let cat = cat();
    cat.set_variant(&vanilla_cat_variants::SIAMESE);
    cat.set_collar_color(DyeColor::Cyan);
    cat.set_tame(true, false);
    cat.set_ordered_to_sit(true);

    let mut nbt = NbtCompound::new();
    cat.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("cat save data should reborrow: {error}"));

    let reloaded = CatEntity::new(&vanilla_entities::CAT, 2, DVec3::ZERO, Weak::new());
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.variant().key, vanilla_cat_variants::SIAMESE.key);
    assert_eq!(reloaded.collar_color(), DyeColor::Cyan);
    assert!(reloaded.is_ordered_to_sit());
    assert!(reloaded.is_in_sitting_pose());
}

/// Vanilla parity: `Cat.customServerAiStep`, which reads the speed back out of
/// the move control. The three speeds are compared exactly, so a cat told to
/// creep must end up crouching and nothing else.
#[test]
fn the_move_control_speed_decides_the_pose() {
    let cat = cat();

    cat.mob_base()
        .controls()
        .lock()
        .move_control
        .set_wanted_position(DVec3::new(1.0, 0.0, 0.0), TEMPT_SPEED_MOD);
    Mob::custom_server_ai_step(&cat);
    assert_eq!(cat.pose(), EntityPose::Sneaking);
    assert!(!cat.is_sprinting());

    cat.mob_base()
        .controls()
        .lock()
        .move_control
        .set_wanted_position(DVec3::new(1.0, 0.0, 0.0), SPRINT_SPEED_MOD);
    Mob::custom_server_ai_step(&cat);
    assert_eq!(cat.pose(), EntityPose::Standing);
    assert!(cat.is_sprinting());

    cat.mob_base().controls().lock().move_control.set_wait();
    Mob::custom_server_ai_step(&cat);
    assert_eq!(cat.pose(), EntityPose::Standing);
    assert!(!cat.is_sprinting());
}
