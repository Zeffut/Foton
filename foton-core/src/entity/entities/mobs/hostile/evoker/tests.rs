//! Evoker behavior worth pinning.

use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};
use glam::DVec3;

use super::*;
use crate::entity::{IllagerSpell, next_entity_id};

const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn evoker() -> EvokerEntity {
    init_vanilla_registry();
    EvokerEntity::new(
        &vanilla_entities::EVOKER,
        next_entity_id(),
        TEST_POSITION,
        Weak::new(),
    )
}

/// The spell ids go out on the wire and the client colors its particles from
/// them, so they are not free to renumber.
#[test]
fn the_spell_the_client_is_told_about_matches_the_one_being_cast() {
    let evoker = evoker();

    evoker.set_is_casting_spell(IllagerSpell::Fangs);

    assert_eq!(evoker.current_spell(), IllagerSpell::Fangs);
    assert_eq!(
        *evoker
            .entity_data
            .lock()
            .spellcaster_illager()
            .spell_casting
            .get(),
        IllagerSpell::Fangs.id()
    );
}

/// `isCastingSpell` reads the countdown, not the spell id: a caster whose
/// timer has run out has its arms down even though the id is still set.
#[test]
fn a_caster_stops_casting_when_the_countdown_runs_out_not_when_the_spell_clears() {
    let evoker = evoker();
    evoker.set_is_casting_spell(IllagerSpell::Wololo);
    evoker.set_spell_casting_time(2);

    assert!(evoker.is_casting_spell());
    assert_eq!(evoker.arm_pose(), IllagerArmPose::Spellcasting);

    evoker.spellcaster_custom_server_ai_step();
    evoker.spellcaster_custom_server_ai_step();

    assert!(!evoker.is_casting_spell());
    assert_eq!(evoker.arm_pose(), IllagerArmPose::Crossed);
    assert_eq!(
        evoker.current_spell(),
        IllagerSpell::Wololo,
        "only the casting goal's stop clears the spell"
    );
}

/// The countdown never runs below zero, which is what stops a long-dead cast
/// from wrapping into a permanent one.
#[test]
fn the_casting_countdown_stops_at_zero() {
    let evoker = evoker();

    evoker.spellcaster_custom_server_ai_step();

    assert_eq!(evoker.spell_casting_time(), 0);
}
