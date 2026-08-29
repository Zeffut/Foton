use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};

use super::*;

fn elder_guardian() -> ElderGuardianEntity {
    ElderGuardianEntity::new(
        &vanilla_entities::ELDER_GUARDIAN,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// Vanilla parity: the `setPersistenceRequired()` of the constructor. Without
/// it a monument empties out the first time a player walks away.
#[test]
fn an_elder_guardian_is_persistent_from_the_moment_it_exists() {
    init_vanilla_registry();
    let mob = elder_guardian();

    assert!(mob.is_persistence_required());
}

/// The elder's beam charges in sixty ticks against the ordinary guardian's
/// eighty, and the shared attack goal reads that through the hook table.
#[test]
fn the_hook_table_reports_the_elders_shorter_charge() {
    init_vanilla_registry();
    let mob = elder_guardian();
    let hooks = guardian_common::hooks_for::<ElderGuardianEntity>();

    assert!((hooks.is_elder)(&mob));
    assert_eq!((hooks.attack_duration)(&mob), ATTACK_DURATION);
}

/// Vanilla parity: `ElderGuardian.customServerAiStep` pins the elder to where
/// it woke up, sixteen blocks either way, which is what keeps one in its
/// monument wing.
#[test]
fn the_first_custom_ai_step_pins_the_elder_to_its_own_position() {
    init_vanilla_registry();
    let mob = elder_guardian();
    assert!(!mob.has_home());

    mob.custom_server_ai_step();

    assert!(mob.has_home());
    assert_eq!(mob.home_radius(), HOME_RADIUS);
    assert_eq!(mob.home_position(), mob.block_position());
}
