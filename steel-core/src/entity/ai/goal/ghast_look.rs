//! Turning a ghast to face where it is going.
//!
//! Vanilla parity: `Ghast.GhastLookGoal` and the `Ghast.faceMovementDirection`
//! static it calls, both public and both shared with the happy ghast. A ghast
//! has no look control worth the name -- it simply points its whole body along
//! its heading, or at its target when it has one.

use super::selector::{Goal, GoalControls};
use crate::entity::{Mob, PathfinderMob};

/// Squared distance beyond which a ghast stops turning toward its target.
///
/// Vanilla parity: the `distanceToSqr(ghast) < 4096.0` of
/// `Ghast.faceMovementDirection`, sixty-four blocks squared.
const FACE_TARGET_RANGE_SQR: f64 = 4096.0;

/// Points a ghast along its heading, or at its target when it has one.
///
/// Vanilla parity: `Ghast.faceMovementDirection`.
pub(crate) fn face_movement_direction(mob: &dyn Mob) {
    let heading = match mob.target() {
        None => {
            let movement = mob.velocity();
            Some((movement.x, movement.z))
        }
        Some(target) => {
            let position = mob.position();
            let target_position = target.position();
            // A target further off than sixty-four blocks leaves the ghast
            // facing wherever it already was.
            (target_position.distance_squared(position) < FACE_TARGET_RANGE_SQR).then(|| {
                (
                    target_position.x - position.x,
                    target_position.z - position.z,
                )
            })
        }
    };

    let Some((xd, zd)) = heading else {
        return;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "vanilla stores entity rotation as a float"
    )]
    let y_rot = -(xd.atan2(zd).to_degrees() as f32);
    let (_, pitch) = mob.rotation();
    mob.set_rotation((y_rot, pitch));
    mob.set_y_body_rot(y_rot);
}

/// Keeps a ghast pointed along its heading.
///
/// Vanilla parity: `Ghast.GhastLookGoal`.
pub(crate) struct GhastLookGoal;

impl Goal for GhastLookGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK
    }

    /// Vanilla parity: `GhastLookGoal.canUse` returns true unconditionally, so
    /// the goal is always running and always the one holding the look control.
    fn can_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        true
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        face_movement_direction(mob);
    }
}
