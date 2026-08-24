use std::sync::Weak;

use steel_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};

use super::*;

fn guardian() -> GuardianEntity {
    GuardianEntity::new(
        &vanilla_entities::GUARDIAN,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// The beam target id is what the client draws the beam from, and zero is the
/// "no beam" sentinel, so the goal has to be able to clear it as well as set it.
#[test]
fn the_beam_target_is_published_and_cleared_through_the_synced_data() {
    init_vanilla_registry();
    let mob = guardian();
    assert!(!mob.has_active_attack_target());

    mob.set_active_attack_target(42);
    assert!(mob.has_active_attack_target());
    assert_eq!(mob.active_attack_target_id(), 42);

    mob.set_active_attack_target(0);
    assert!(!mob.has_active_attack_target());
}

/// Vanilla parity: the guardian re-rolls its wander target on every hit,
/// whether or not the thorns fired. The flag has to be consumed exactly once.
#[test]
fn a_pending_wander_reroll_is_taken_exactly_once() {
    init_vanilla_registry();
    let mob = guardian();
    assert!(!mob.guardian_state().lock().trigger_stroll);

    let hooks = guardian_common::hooks_for::<GuardianEntity>();
    (hooks.trigger_stroll)(&mob);

    assert!(mob.guardian_state().lock().trigger_stroll);
    assert!((hooks.take_stroll_trigger)(&mob));
    assert!(!(hooks.take_stroll_trigger)(&mob));
}

/// The shared attack goal reads the charge length and the elder flag through
/// the hook table, so a wrong table would give an ordinary guardian the elder's
/// beam.
#[test]
fn the_hook_table_reports_an_ordinary_guardian() {
    init_vanilla_registry();
    let mob = guardian();
    let hooks = guardian_common::hooks_for::<GuardianEntity>();

    assert!(!(hooks.is_elder)(&mob));
    assert_eq!((hooks.attack_duration)(&mob), ATTACK_TIME);
}

/// Vanilla parity: a guardian scores anything that is not water at nothing,
/// which is what keeps it in the sea.
#[test]
fn dry_land_scores_nothing_as_a_walk_target() {
    init_vanilla_registry();
    let mob = guardian();

    assert!(mob.get_walk_target_value(BlockPos::new(0, 64, 0)).abs() < f32::EPSILON);
}

/// The moving flag gates both the thorns and the idle sink, so it has to
/// round-trip through the synchronized data.
#[test]
fn the_moving_flag_round_trips_through_the_synced_data() {
    init_vanilla_registry();
    let mob = guardian();
    assert!(!mob.is_moving());

    mob.set_moving(true);

    assert!(mob.is_moving());
}

/// Vanilla parity: the thorns branch is skipped for thorns damage itself, so
/// two guardians cannot spike each other forever -- but the wander re-roll
/// still happens, because it sits outside the branch.
#[test]
fn thorns_damage_never_reflects_but_still_rerolls_the_wander() {
    init_vanilla_registry();
    let world = crate::test_support::fresh_test_world("guardian_thorns");
    let mob = guardian();
    let source = DamageSource::environment(&vanilla_damage_types::THORNS).with_direct_entity(7);

    guardian_common::on_hurt(&mob, world.as_ref(), &source);

    assert!(mob.guardian_state().lock().trigger_stroll);
}
