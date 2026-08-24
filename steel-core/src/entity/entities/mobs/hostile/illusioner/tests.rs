//! Illusioner behavior worth pinning.

use std::sync::Weak;

use glam::DVec3;
use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;
use crate::entity::{IllagerSpell, next_entity_id};

const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn illusioner() -> IllusionerEntity {
    init_vanilla_registry();
    IllusionerEntity::new(
        &vanilla_entities::ILLUSIONER,
        next_entity_id(),
        TEST_POSITION,
        Weak::new(),
    )
}

/// The illusioner is the one spellcaster whose arm pose puts the bow where the
/// other casters put the celebration, so the two orderings must not be shared.
#[test]
fn a_shooting_illusioner_raises_a_bow_where_an_evoker_would_celebrate() {
    let illusioner = illusioner();
    illusioner.set_celebrating(true);
    illusioner.set_aggressive(true);

    assert_eq!(illusioner.arm_pose(), IllagerArmPose::BowAndArrow);

    illusioner.set_is_casting_spell(IllagerSpell::Disappear);
    illusioner.set_spell_casting_time(1);

    assert_eq!(illusioner.arm_pose(), IllagerArmPose::Spellcasting);
}
