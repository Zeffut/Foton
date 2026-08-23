use std::sync::Weak;

use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;
use crate::entity::next_entity_id;

fn goat() -> GoatEntity {
    init_vanilla_registry();
    GoatEntity::new(
        &vanilla_entities::GOAT,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn a_goat_walks_off_ten_blocks_of_any_fall() {
    let goat = goat();

    // The shared formula charges one heart per block past the safe distance.
    let plain = goat.default_calculate_fall_damage(20.0, 1.0);
    assert_eq!(
        goat.calculate_fall_damage(20.0, 1.0),
        plain - GOAT_FALL_DAMAGE_REDUCTION
    );

    // Anything a goat could jump on its own costs it nothing at all.
    assert!(goat.calculate_fall_damage(12.0, 1.0) <= 0);
}

#[test]
fn a_goat_sheds_one_horn_at_a_time_and_then_has_none_left() {
    let goat = goat();
    goat.set_age(0);

    assert!(goat.has_left_horn());
    assert!(goat.has_right_horn());

    assert!(goat.drop_horn());
    assert_ne!(goat.has_left_horn(), goat.has_right_horn());

    assert!(goat.drop_horn());
    assert!(!goat.has_left_horn());
    assert!(!goat.has_right_horn());

    assert!(!goat.drop_horn());
}

#[test]
fn a_kid_keeps_its_horns_however_hard_it_charges() {
    let goat = goat();
    goat.set_baby(true);

    assert!(!goat.drop_horn());
    assert!(goat.has_left_horn());
    assert!(goat.has_right_horn());
}

#[test]
fn the_same_goat_always_sheds_the_same_horn() {
    // Vanilla seeds the instrument choice with the goat's own UUID, so the horn
    // a goat carries is fixed from the moment it spawns.
    let goat = goat();

    let first = goat.create_horn();
    let second = goat.create_horn();

    assert_eq!(first.get(INSTRUMENT), second.get(INSTRUMENT));
    assert!(
        first.get(INSTRUMENT).is_some(),
        "the regular goat horn tag should not be empty"
    );
}

#[test]
fn a_screaming_goat_draws_from_the_screaming_horn_tag() {
    let goat = goat();

    let regular = goat.create_horn();
    goat.set_screaming_goat(true);
    let screaming = goat.create_horn();

    let regular_tag = REGISTRY
        .instruments
        .get_tag(&vanilla_instrument_tags::InstrumentTag::REGULAR_GOAT_HORNS)
        .expect("regular goat horns tag");
    let screaming_tag = REGISTRY
        .instruments
        .get_tag(&vanilla_instrument_tags::InstrumentTag::SCREAMING_GOAT_HORNS)
        .expect("screaming goat horns tag");
    assert!(
        regular_tag
            .iter()
            .all(|instrument| !screaming_tag.contains(instrument))
    );
    assert_ne!(regular.get(INSTRUMENT), screaming.get(INSTRUMENT));
}

#[test]
fn a_kid_butts_for_half_the_damage_an_adult_does() {
    let goat = goat();

    goat.set_baby(true);
    assert_eq!(
        goat.attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE)
            .to_bits(),
        BABY_ATTACK_DAMAGE.to_bits()
    );

    goat.set_baby(false);
    assert_eq!(
        goat.attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE)
            .to_bits(),
        ADULT_ATTACK_DAMAGE.to_bits()
    );
}
