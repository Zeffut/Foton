use std::sync::Weak;

use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};

use super::*;
use crate::entity::next_entity_id;

fn dolphin() -> DolphinEntity {
    init_vanilla_registry();
    DolphinEntity::new(
        &vanilla_entities::DOLPHIN,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn a_dolphin_out_of_the_water_dries_out_and_drowns_on_no_timer_of_its_own() {
    // Vanilla's `Dolphin.handleAirSupply` is empty: the air supply never runs
    // the dolphin down, moistness does. A dolphin held under water forever is
    // fine; one left on a beach is not.
    let dolphin = dolphin();

    assert_eq!(dolphin.moistness_level(), TOTAL_MOISTNESS_LEVEL);
    assert_eq!(dolphin.max_air_supply(), TOTAL_AIR_SUPPLY);
    assert_eq!(dolphin.increase_air_supply(0), TOTAL_AIR_SUPPLY);

    dolphin.tick_moistness();

    assert_eq!(dolphin.moistness_level(), TOTAL_MOISTNESS_LEVEL - 1);
}

#[test]
fn only_a_fish_interests_a_dolphin() {
    init_vanilla_registry();

    assert!(DolphinEntity::is_fish(&ItemStack::new(&vanilla_items::COD)));
    assert!(DolphinEntity::is_fish(&ItemStack::new(
        &vanilla_items::TROPICAL_FISH
    )));
    assert!(!DolphinEntity::is_fish(&ItemStack::new(
        &vanilla_items::WHEAT
    )));
    assert!(!DolphinEntity::is_fish(&ItemStack::empty()));
}

#[test]
fn a_dolphin_flees_both_kinds_of_guardian_and_nothing_else() {
    init_vanilla_registry();

    assert!(is_guardian(&vanilla_entities::GUARDIAN));
    assert!(is_guardian(&vanilla_entities::ELDER_GUARDIAN));
    assert!(!is_guardian(&vanilla_entities::COD));
}

#[test]
fn a_calf_never_starts_a_fight() {
    // Vanilla's `canAttack` refuses outright while the dolphin is a baby, which
    // is what keeps a pod's young out of a guardian fight.
    let calf = dolphin();
    let other = dolphin();

    calf.set_baby(true);
    assert!(!Mob::can_attack(&calf, &other));

    calf.set_baby(false);
    assert!(Mob::can_attack(&calf, &other));
}
