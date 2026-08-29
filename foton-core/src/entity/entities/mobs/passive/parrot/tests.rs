use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};
use glam::DVec3;

use super::*;

fn parrot() -> ParrotEntity {
    init_vanilla_registry();
    ParrotEntity::new(&vanilla_entities::PARROT, 1, DVec3::ZERO, Weak::new())
}

/// Vanilla parity: `Parrot.Variant.byId`, whose `ByIdMap.OutOfBoundsStrategy.CLAMP`
/// is what stops a corrupted save turning a parrot invisible.
#[test]
fn an_out_of_range_variant_id_clamps_to_an_end_of_the_range() {
    assert_eq!(ParrotVariant::by_id(-5), ParrotVariant::RedBlue);
    assert_eq!(ParrotVariant::by_id(0), ParrotVariant::RedBlue);
    assert_eq!(ParrotVariant::by_id(4), ParrotVariant::Gray);
    assert_eq!(ParrotVariant::by_id(99), ParrotVariant::Gray);

    for variant in ParrotVariant::VALUES {
        assert_eq!(ParrotVariant::by_id(variant.id()), variant);
    }
}

/// Vanilla parity: `ShoulderRidingEntity.canSitOnShoulder`. Without the
/// cooldown a parrot knocked off a shoulder would climb straight back on.
#[test]
fn a_parrot_waits_out_the_ride_cooldown_before_it_may_perch_again() {
    let parrot = parrot();
    assert!(!parrot.can_sit_on_shoulder());

    for _ in 0..=RIDE_COOLDOWN_TICKS {
        Entity::tick(&parrot);
    }

    assert!(parrot.can_sit_on_shoulder());
}

/// Vanilla parity: `Parrot.isFood` returns false while `PARROT_FOOD` still
/// tames. Confusing the two would let players breed parrots, which vanilla
/// does not allow.
#[test]
fn seeds_tame_a_parrot_but_are_not_breeding_food() {
    use foton_registry::item_stack::ItemStack;
    use foton_registry::vanilla_items;

    init_vanilla_registry();
    let parrot = parrot();
    let seeds = ItemStack::new(&vanilla_items::WHEAT_SEEDS);

    assert!(ParrotEntity::is_taming_food(&seeds));
    assert!(!Animal::is_food(&parrot, &seeds));
}

/// Vanilla parity: `Parrot.canMate`, which is false even for two tamed parrots
/// in love -- parrots are the one tameable animal that never breeds.
#[test]
fn two_parrots_never_mate() {
    init_vanilla_registry();
    let first = ParrotEntity::new(&vanilla_entities::PARROT, 1, DVec3::ZERO, Weak::new());
    let second = ParrotEntity::new(&vanilla_entities::PARROT, 2, DVec3::ZERO, Weak::new());
    first.set_tame(true, false);
    second.set_tame(true, false);
    first.set_in_love_time(600);
    second.set_in_love_time(600);

    assert!(!Animal::can_mate(&first, &second));
}

/// Vanilla parity: `Parrot.createNavigation` and the `FlyingMoveControl` the
/// constructor installs. Getting either wrong leaves a parrot walking.
#[test]
fn a_parrot_navigates_and_steers_as_a_flier() {
    let parrot = parrot();

    assert_eq!(
        PathfinderMob::navigation_kind(&parrot),
        NavigationKind::Flying
    );
    assert_eq!(
        Mob::move_control_kind(&parrot),
        MoveControlKind::Flying {
            max_turn: 10.0,
            hovers_in_place: false,
        }
    );
}
