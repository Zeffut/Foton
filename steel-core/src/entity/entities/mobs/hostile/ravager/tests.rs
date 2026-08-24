//! Ravager behavior worth pinning.

use std::sync::Weak;

use glam::DVec3;
use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;
use crate::entity::next_entity_id;

const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn ravager() -> RavagerEntity {
    init_vanilla_registry();
    RavagerEntity::new(
        &vanilla_entities::RAVAGER,
        next_entity_id(),
        TEST_POSITION,
        Weak::new(),
    )
}

/// A stagger runs its two seconds out and then arms the roar, and the ravager
/// is frozen and blind for the whole of it. Losing any one of those three
/// turns the ravager's only weakness into a formality.
#[test]
fn a_stunned_ravager_is_frozen_blind_and_ends_by_arming_a_roar() {
    let ravager = ravager();
    ravager.timers.lock().stunned = STUN_DURATION;

    assert!(ravager.is_immobile());
    assert!(!ravager.has_line_of_sight(&ravager));

    for _ in 0..STUN_DURATION {
        ravager.ravager_ai_step();
    }

    assert_eq!(ravager.stunned_tick(), 0);
    assert_eq!(
        ravager.roar_tick(),
        ROAR_DURATION,
        "the stun ends by winding up the roar"
    );
    assert!(ravager.is_immobile(), "the wind-up keeps it frozen");
}

/// The roar lands halfway through its wind-up, not at the end.
#[test]
fn the_roar_lands_halfway_through_its_wind_up() {
    let ravager = ravager();
    ravager.timers.lock().roar = ROAR_DURATION;

    for _ in 0..(ROAR_DURATION - ROAR_TRIGGER_TICK) {
        ravager.ravager_ai_step();
    }

    assert_eq!(ravager.roar_tick(), ROAR_TRIGGER_TICK);
}

/// A ravager accelerates towards its target speed rather than snapping to it,
/// which is what gives a charge its wind-up.
#[test]
fn a_ravager_eases_up_to_its_charging_speed() {
    let ravager = ravager();
    ravager
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::MOVEMENT_SPEED, BASE_MOVEMENT_SPEED);

    ravager.ravager_ai_step();

    let speed = ravager
        .attributes()
        .lock()
        .get_base_value(vanilla_attributes::MOVEMENT_SPEED)
        .expect("a ravager has a movement speed attribute");
    assert!(
        (speed - BASE_MOVEMENT_SPEED).abs() < 1.0e-9,
        "with no target the ravager stays at its base speed, got {speed}"
    );
}

/// A ravager never carries the ominous banner, so a patrol it walks with still
/// has an illager captain.
#[test]
fn a_ravager_refuses_the_captains_banner() {
    let ravager = ravager();

    assert!(!ravager.can_be_leader());
}
