use std::sync::Weak;

use foton_registry::{
    init_vanilla_registry, vanilla_damage_types, vanilla_entities, vanilla_items,
};

use super::*;
use crate::entity::next_entity_id;
use crate::test_support::test_world;

fn polar_bear() -> PolarBearEntity {
    init_vanilla_registry();
    PolarBearEntity::new(
        &vanilla_entities::POLAR_BEAR,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn a_polar_bear_refuses_every_food_there_is() {
    // Vanilla's `isFood` returns false outright, which is what makes the polar
    // bear the one animal that cannot be bred or tempted.
    let bear = polar_bear();

    for item in [
        &vanilla_items::COD,
        &vanilla_items::SALMON,
        &vanilla_items::WHEAT,
        &vanilla_items::SWEET_BERRIES,
    ] {
        assert!(!Animal::is_food(&bear, &ItemStack::new(item)));
    }
}

#[test]
fn a_warning_growl_waits_two_seconds_before_it_can_sound_again() {
    let bear = polar_bear();

    assert_eq!(*bear.warning_sound_ticks.lock(), 0);

    bear.play_warning_sound();
    assert_eq!(*bear.warning_sound_ticks.lock(), WARNING_SOUND_INTERVAL);

    // A second growl inside the window must not restart the timer.
    *bear.warning_sound_ticks.lock() = 5;
    bear.play_warning_sound();
    assert_eq!(*bear.warning_sound_ticks.lock(), 5);
}

#[test]
fn a_cub_and_a_grown_bear_panic_at_different_things() {
    // Vanilla hands `PanicGoal` a function of the mob, so the same goal answers
    // differently as the bear grows up. A cub runs from a sword; an adult does
    // not, because it is meant to fight back.
    let bear = polar_bear();
    let goal = PanicGoal::with_panic_causing_damage_types(PANIC_SPEED_MOD, |mob| {
        let is_baby = mob.as_ageable_mob().is_some_and(AgeableMob::is_baby);
        if is_baby {
            DamageTypeTag::PANIC_CAUSES
        } else {
            DamageTypeTag::PANIC_ENVIRONMENTAL_CAUSES
        }
    });

    assert!(bear.hurt_server(
        test_world(),
        &DamageSource::environment(&vanilla_damage_types::PLAYER_ATTACK),
        1.0
    ));

    bear.set_baby(false);
    assert!(!goal.should_panic(&bear));

    bear.set_baby(true);
    assert!(goal.should_panic(&bear));
}
