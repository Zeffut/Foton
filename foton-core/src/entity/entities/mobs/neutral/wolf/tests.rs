use std::io::Cursor;
use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};
use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;
use crate::entity::entities::CreeperEntity;

fn wolf() -> WolfEntity {
    init_vanilla_registry();
    WolfEntity::new(&vanilla_entities::WOLF, 1, DVec3::ZERO, Weak::new())
}

/// Vanilla parity: `Wolf.applyTamingSideEffects`, which is the whole reason a
/// tamed wolf survives a fight a wild one loses.
#[test]
fn taming_raises_max_health_from_eight_to_forty() {
    let wolf = wolf();

    assert!((wolf.get_max_health() - START_HEALTH as f32).abs() < f32::EPSILON);

    wolf.set_tame(true, true);

    assert!((wolf.get_max_health() - TAME_HEALTH as f32).abs() < f32::EPSILON);
    assert!((wolf.get_health() - TAME_HEALTH as f32).abs() < f32::EPSILON);

    wolf.set_tame(false, true);

    assert!((wolf.get_max_health() - START_HEALTH as f32).abs() < f32::EPSILON);
}

/// The tame flag and the sitting pose share one synced byte, so setting one
/// must not disturb the other.
#[test]
fn the_sitting_pose_and_the_tame_flag_do_not_disturb_each_other() {
    let wolf = wolf();

    wolf.set_tame(true, false);
    wolf.set_in_sitting_pose(true);
    assert!(wolf.is_tame());
    assert!(wolf.is_in_sitting_pose());

    wolf.set_in_sitting_pose(false);
    assert!(wolf.is_tame());
    assert!(!wolf.is_in_sitting_pose());

    wolf.set_tame(false, false);
    assert!(!wolf.is_tame());
    assert!(!wolf.is_in_sitting_pose());
}

/// Vanilla parity: `Wolf.canMate`. An untamed wolf never breeds, and a sitting
/// partner refuses even when both are in love.
#[test]
fn only_two_standing_tamed_wolves_may_mate() {
    init_vanilla_registry();
    let first = WolfEntity::new(&vanilla_entities::WOLF, 1, DVec3::ZERO, Weak::new());
    let second = WolfEntity::new(&vanilla_entities::WOLF, 2, DVec3::ZERO, Weak::new());
    first.set_in_love_time(600);
    second.set_in_love_time(600);

    assert!(!Animal::can_mate(&first, &second));

    first.set_tame(true, false);
    second.set_tame(true, false);
    assert!(Animal::can_mate(&first, &second));

    second.set_in_sitting_pose(true);
    assert!(!Animal::can_mate(&first, &second));
}

/// Vanilla parity: `Wolf.getMaxHeadXRot`, which is what keeps a sitting wolf
/// from craning its neck at the ceiling.
#[test]
fn a_sitting_wolf_looks_less_far_up_and_down() {
    let wolf = wolf();

    assert!((Mob::max_head_x_rot(&wolf) - 30.0).abs() < f32::EPSILON);

    wolf.set_in_sitting_pose(true);

    assert!((Mob::max_head_x_rot(&wolf) - SITTING_MAX_HEAD_X_ROT).abs() < f32::EPSILON);
}

/// Vanilla parity: `Wolf.canBeLeashed`, which is why an angry wolf cannot be
/// led away from the fight.
#[test]
fn an_angry_wolf_refuses_a_lead() {
    let wolf = wolf();

    assert!(Mob::can_be_leashed(&wolf));

    wolf.set_persistent_anger_end_time(i64::MAX);

    assert!(!Mob::can_be_leashed(&wolf));
}

/// The collar color is a synced integer; the default has to be red or every
/// freshly tamed wolf wears the wrong collar.
#[test]
fn a_wolf_starts_with_a_red_collar_and_keeps_a_dyed_one_through_a_save() {
    let wolf = wolf();
    assert_eq!(wolf.collar_color(), DEFAULT_COLLAR_COLOR);

    wolf.set_collar_color(DyeColor::Lime);
    let mut nbt = NbtCompound::new();
    wolf.save_additional(&mut nbt);

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("wolf save data should reborrow: {error}"));

    let reloaded = WolfEntity::new(&vanilla_entities::WOLF, 2, DVec3::ZERO, Weak::new());
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.collar_color(), DyeColor::Lime);
}

/// Vanilla parity: `Wolf.wantsToAttack`. A wolf must never be talked into
/// attacking a creeper, and must never turn on its owner's other pets.
#[test]
fn a_wolf_refuses_the_targets_vanilla_excludes() {
    init_vanilla_registry();
    let wolf = WolfEntity::new(&vanilla_entities::WOLF, 1, DVec3::ZERO, Weak::new());
    let owner = WolfEntity::new(&vanilla_entities::WOLF, 2, DVec3::ZERO, Weak::new());
    let creeper = CreeperEntity::new(&vanilla_entities::CREEPER, 3, DVec3::ZERO, Weak::new());
    let other_pet = WolfEntity::new(&vanilla_entities::WOLF, 4, DVec3::ZERO, Weak::new());

    assert!(!wolf.wants_to_attack(&creeper, &owner));

    other_pet.set_tame(true, false);
    other_pet.set_owner_uuid(Some(owner.uuid()));
    assert!(!wolf.wants_to_attack(&other_pet, &owner));

    other_pet.set_owner_uuid(Some(uuid::Uuid::from_u128(1)));
    assert!(wolf.wants_to_attack(&other_pet, &owner));
}
