//! The four illagers, and what they have in common.
//!
//! Vanilla parity: `AbstractIllager`. Very little of the class is behavior --
//! it adds one arm pose, one alliance rule and one door goal on top of
//! [`crate::entity::Raider`] -- but the alliance rule is what keeps a
//! pillager's bolt from killing the vindicator standing in front of it, and the
//! arm pose is the whole of what a player sees an illager doing.

use steel_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_entity_type_tags::EntityTypeTag};

use crate::entity::Entity;
use crate::entity::raider::Raider;

/// What an illager is doing with its arms.
///
/// Vanilla parity: `AbstractIllager.IllagerArmPose`. Vanilla never sends this:
/// the client derives it from the synced flags every tick and picks a model
/// pose. Steel keeps the derivation server-side anyway, because it is the one
/// place the meaning of those flags is written down, and because a mob whose
/// pose says `CROSSBOW_CHARGE` while nothing is charging is a bug worth
/// catching in a test rather than in a screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IllagerArmPose {
    /// Arms folded: the idle pose.
    Crossed,
    /// Mid-swing.
    Attacking,
    /// Hands raised, casting.
    Spellcasting,
    /// Drawing a bow.
    BowAndArrow,
    /// Holding a loaded crossbow.
    CrossbowHold,
    /// Winding a crossbow.
    CrossbowCharge,
    /// Jumping over a fallen village.
    Celebrating,
    /// Arms down: the pose a pillager without a crossbow falls back to.
    Neutral,
}

/// An illager.
///
/// Vanilla parity: the `AbstractIllager` class.
pub trait AbstractIllager: Raider {
    /// Returns what this illager is doing with its arms.
    ///
    /// Vanilla parity: `getArmPose`, which defaults to folded arms.
    fn arm_pose(&self) -> IllagerArmPose {
        IllagerArmPose::Crossed
    }

    /// Returns vanilla `AbstractIllager.considersEntityAsAlly`.
    ///
    /// The tag holds the four illagers, so a pillager, a vindicator, an evoker
    /// and an illusioner never target one another. Vanilla also requires both
    /// entities to be teamless, which is free here: Steel puts no entity on a
    /// scoreboard team.
    ///
    /// Vanilla's `canAttack` override -- refusing to hit a baby villager -- is
    /// absent with the villagers it names.
    fn considers_entity_as_ally_illager(&self, other: &dyn Entity) -> bool {
        REGISTRY
            .entity_types
            .is_in_tag(other.entity_type(), &EntityTypeTag::ILLAGER_FRIENDS)
    }
}
