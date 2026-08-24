//! Vindicator behavior worth pinning.

use std::sync::Weak;

use glam::DVec3;
use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;
use crate::entity::next_entity_id;

const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn vindicator() -> VindicatorEntity {
    init_vanilla_registry();
    VindicatorEntity::new(
        &vanilla_entities::VINDICATOR,
        next_entity_id(),
        TEST_POSITION,
        Weak::new(),
    )
}

/// Johnny latches on the exact name and never unlatches, which is the whole
/// of the easter egg: renaming the mob back does not calm it down.
#[test]
fn johnny_latches_on_the_exact_name_and_never_lets_go() {
    let vindicator = vindicator();

    vindicator.set_custom_name(Some(TextComponent::plain("johnny")));
    assert!(!vindicator.is_johnny(), "the name is case sensitive");

    vindicator.set_custom_name(Some(TextComponent::plain("Johnny")));
    assert!(vindicator.is_johnny());

    vindicator.set_custom_name(Some(TextComponent::plain("Steve")));
    assert!(
        vindicator.is_johnny(),
        "vanilla only ever sets the flag, so a renamed Johnny stays Johnny"
    );
}

/// The arm pose orders swinging above celebrating, and folded arms is the
/// idle. A vindicator that celebrated while attacking would look wrong.
#[test]
fn a_swinging_vindicator_outranks_a_celebrating_one() {
    let vindicator = vindicator();

    assert_eq!(vindicator.arm_pose(), IllagerArmPose::Crossed);

    vindicator.set_celebrating(true);
    assert_eq!(vindicator.arm_pose(), IllagerArmPose::Celebrating);

    vindicator.set_aggressive(true);
    assert_eq!(vindicator.arm_pose(), IllagerArmPose::Attacking);
}
