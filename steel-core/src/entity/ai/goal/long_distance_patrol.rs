//! Long-distance patrol goal.
//!
//! Vanilla parity: `PatrollingMonster.LongDistancePatrolGoal`. This is why an
//! illager patrol is found walking in the middle of nowhere: the captain picks
//! a point up to five hundred blocks away, and the group re-aims at a spot ten
//! blocks ahead of itself every time the path runs out. The aim is deliberately
//! skewed sideways so the line drifts rather than marching straight, and a
//! patroller whose companions have all died stops patrolling.

use glam::DVec3;
use steel_utils::BlockPos;

use steel_math::trig;

use super::selector::{Goal, GoalControls};
use crate::chunk::heightmap::HeightmapType;
use crate::entity::{PathfinderMob, PatrollingMonster, SharedEntity};

/// Ticks a patroller waits after a failed path before trying again.
///
/// Vanilla parity: `LongDistancePatrolGoal.NAVIGATION_FAILED_COOLDOWN`.
const NAVIGATION_FAILED_COOLDOWN: i64 = 200;

/// How far a patroller looks for the rest of its group.
///
/// Vanilla parity: the `inflate(16.0)` of `findPatrolCompanions`.
const COMPANION_SEARCH_RANGE: f64 = 16.0;

/// How close the captain gets before choosing the next waypoint.
///
/// Vanilla parity: the `closerToCenterThan(position, 10.0)` of `tick`.
const WAYPOINT_ARRIVAL_DISTANCE: f64 = 10.0;

/// How far ahead the group aims each time the path runs out.
///
/// Vanilla parity: the `scale(10.0)` of `tick`.
const STEP_DISTANCE: f64 = 10.0;

/// How hard the aim is skewed sideways.
///
/// Vanilla parity: the `scale(0.4)` applied to the rotated offset.
const SIDEWAYS_DRIFT: f64 = 0.4;

/// The angle vanilla rotates the offset by, in radians.
///
/// Vanilla parity: `distance.yRot(90.0F)`. `Vec3.yRot` takes radians, so the
/// literal `90.0F` really is ninety radians -- a little over fourteen turns.
/// The result is a fixed skew of about 153 degrees rather than the quarter turn
/// the constant looks like, and reproducing it is the difference between
/// vanilla's drifting patrol line and a straight one.
const PATROL_DRIFT_ROTATION: f64 = 90.0;

/// Half-width of the box a patroller looks for companions in, on Y.
///
/// Vanilla parity: `inflate(16.0)` inflates all three axes equally.
const COMPANION_SEARCH_HEIGHT: f64 = COMPANION_SEARCH_RANGE;

/// Walks a patrol from waypoint to waypoint.
///
/// Vanilla parity: `PatrollingMonster.LongDistancePatrolGoal`.
pub(crate) struct LongDistancePatrolGoal {
    /// Speed a follower walks at.
    speed_modifier: f64,
    /// Speed the captain walks at, slightly slower so the group keeps up.
    leader_speed_modifier: f64,
    /// Game time before which the goal refuses to run after a failed path.
    cooldown_until: i64,
}

impl LongDistancePatrolGoal {
    /// Creates the goal with vanilla's two speeds.
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64, leader_speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            leader_speed_modifier,
            cooldown_until: -1,
        }
    }

    /// Returns the other patrollers close enough to march together.
    ///
    /// Vanilla parity: `findPatrolCompanions`.
    fn find_patrol_companions(mob: &dyn PatrollingMonster) -> Vec<SharedEntity> {
        let Some(world) = mob.level() else {
            return Vec::new();
        };
        let search_box = mob.bounding_box().inflate_xyz(
            COMPANION_SEARCH_RANGE,
            COMPANION_SEARCH_HEIGHT,
            COMPANION_SEARCH_RANGE,
        );
        let self_id = mob.id();
        world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.id() != self_id
                && entity
                    .as_patrolling_monster()
                    .is_some_and(PatrollingMonster::can_join_patrol)
        })
    }

    /// Picks a spot near the mob to walk to when the real path failed.
    ///
    /// Vanilla parity: `moveRandomly`.
    fn move_randomly(&self, mob: &dyn PathfinderMob, patroller: &dyn PatrollingMonster) -> bool {
        let Some(world) = patroller.level() else {
            return false;
        };
        let offset = patroller.block_position().offset(
            rand::random_range(-8..8),
            0,
            rand::random_range(-8..8),
        );
        let target = world.heightmap_pos(HeightmapType::MotionBlockingNoLeaves, offset);
        mob.move_to_pos(block_pos_as_target(target), self.speed_modifier)
    }
}

impl Goal for LongDistancePatrolGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(patroller) = mob.as_patrolling_monster() else {
            return false;
        };
        let on_cooldown = mob
            .level()
            .is_some_and(|world| world.game_time() < self.cooldown_until);
        patroller.is_patrolling()
            && mob.target().is_none()
            && mob.controlling_passenger_mob().is_none()
            && patroller.has_patrol_target()
            && !on_cooldown
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(patroller) = mob.as_patrolling_monster() else {
            return;
        };
        if !mob.mob_base().navigation().lock().is_done() {
            return;
        }
        let Some(world) = mob.level() else {
            return;
        };
        let Some(patrol_target) = patroller.patrol_target() else {
            return;
        };

        let is_leader = patroller.is_patrol_leader();
        let companions = Self::find_patrol_companions(patroller);
        if patroller.is_patrolling() && companions.is_empty() {
            patroller.set_patrolling(false);
            return;
        }

        let waypoint = block_pos_as_target(patrol_target);
        if is_leader
            && waypoint.distance_squared(mob.position()) < square(WAYPOINT_ARRIVAL_DISTANCE)
        {
            patroller.find_patrol_target();
            return;
        }

        let position = mob.position();
        let drift = y_rot(position - waypoint, PATROL_DRIFT_ROTATION) * SIDEWAYS_DRIFT + waypoint;
        let step = (drift - position).normalize_or_zero() * STEP_DISTANCE + position;
        let path_target = world.heightmap_pos(
            HeightmapType::MotionBlockingNoLeaves,
            BlockPos::containing(step.x, step.y, step.z),
        );

        let speed = if is_leader {
            self.leader_speed_modifier
        } else {
            self.speed_modifier
        };
        if mob.move_to_pos(block_pos_as_target(path_target), speed) {
            if is_leader {
                for companion in companions {
                    if let Some(companion) = companion.as_patrolling_monster() {
                        companion.set_patrol_target(path_target);
                    }
                }
            }
            return;
        }

        self.move_randomly(mob, patroller);
        self.cooldown_until = world.game_time() + NAVIGATION_FAILED_COOLDOWN;
    }
}

/// Returns the bottom center of `pos`.
///
/// Vanilla parity: `Vec3.atBottomCenterOf`.
fn block_pos_as_target(pos: BlockPos) -> DVec3 {
    let (x, y, z) = pos.get_bottom_center();
    DVec3::new(x, y, z)
}

/// Rotates `vector` around the Y axis by `radians`.
///
/// Vanilla parity: `Vec3.yRot`, including its use of the sine table so the
/// result matches vanilla's rather than the exact trigonometric value.
fn y_rot(vector: DVec3, radians: f64) -> DVec3 {
    let cos = f64::from(trig::cos(radians));
    let sin = f64::from(trig::sin(radians));
    DVec3::new(
        vector.x.mul_add(cos, vector.z * sin),
        vector.y,
        vector.z.mul_add(cos, -(vector.x * sin)),
    )
}

const fn square(value: f64) -> f64 {
    value * value
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The drift is the whole shape of a patrol line, and a reader who assumes
    /// `yRot(90.0F)` is a quarter turn would write it as a perpendicular offset
    /// and never notice. Ninety radians lands elsewhere entirely.
    #[test]
    fn the_patrol_drift_is_ninety_radians_and_not_a_quarter_turn() {
        let east = DVec3::new(1.0, 0.0, 0.0);
        let rotated = y_rot(east, PATROL_DRIFT_ROTATION);

        assert!(
            rotated.x < -0.4 && rotated.x > -0.5,
            "ninety radians points back-and-left, got {rotated:?}"
        );
        assert!(
            rotated.z < -0.8,
            "a quarter turn would leave x at zero, got {rotated:?}"
        );
    }
}
