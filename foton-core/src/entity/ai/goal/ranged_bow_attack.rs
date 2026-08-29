//! Ranged bow attack goal.
//!
//! Vanilla parity: `RangedBowAttackGoal`. A skeleton does not stand still and
//! shoot: it circles its target, backs off when crowded, closes when the target
//! runs, and flips direction now and then so it cannot be strafed into a corner.

use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;

/// Ticks a target must stay visible before the archer commits to a shot.
///
/// Vanilla parity: the `seeTime >= 20` of `RangedBowAttackGoal.tick`.
const SEEN_TIME_BEFORE_FIRING: i32 = 20;

/// Ticks of strafing before the archer reconsiders its direction.
///
/// Vanilla parity: the `strafingTime >= 20` of the same method.
const STRAFE_REVIEW_TICKS: i32 = 20;

/// Chance each review that the archer swaps the side it circles on.
///
/// Vanilla parity: the two `nextFloat() < 0.3` rolls.
const STRAFE_FLIP_CHANCE: f32 = 0.3;

/// Fraction of the attack radius past which the archer stops backing away.
///
/// Vanilla parity: the `attackRadiusSqr * 0.75F` threshold.
const STOP_BACKING_OFF_AT: f64 = 0.75;

/// Fraction of the attack radius inside which the archer backs away.
///
/// Vanilla parity: the `attackRadiusSqr * 0.25F` threshold.
const START_BACKING_OFF_AT: f64 = 0.25;

/// How hard the archer strafes.
///
/// Vanilla parity: the `0.5F` magnitudes passed to `MoveControl.strafe`.
const STRAFE_STRENGTH: f32 = 0.5;

/// Keeps distance from the target and looses arrows at it.
///
/// Vanilla parity: `RangedBowAttackGoal`. The firing itself is left to the
/// caller, since what leaves the bow differs by mob.
pub struct RangedBowAttackGoal {
    /// Ticks until the next shot.
    attack_time: i32,
    /// Ticks the target has been continuously visible.
    seen_time: i32,
    /// Ticks spent circling, or `-1` while closing the distance.
    strafing_time: i32,
    /// Which way around the target the archer is circling.
    strafing_clockwise: bool,
    /// Whether the archer is backing away rather than closing.
    strafing_backwards: bool,
    /// Ticks between two shots.
    attack_interval: i32,
    /// Range within which the archer will shoot rather than approach.
    attack_radius: f64,
    /// Speed the archer closes the distance at.
    speed_modifier: f64,
    /// What to do when the archer decides to shoot.
    fire: fn(&dyn PathfinderMob, glam::DVec3),
}

impl RangedBowAttackGoal {
    /// Creates the goal for one archer.
    #[must_use]
    pub(crate) const fn new(
        attack_interval: i32,
        attack_radius: f64,
        speed_modifier: f64,
        fire: fn(&dyn PathfinderMob, glam::DVec3),
    ) -> Self {
        Self {
            attack_time: -1,
            seen_time: 0,
            strafing_time: -1,
            strafing_clockwise: false,
            strafing_backwards: false,
            attack_interval,
            attack_radius,
            speed_modifier,
            fire,
        }
    }

    /// Reconsiders which way to circle, and how far out to stand.
    ///
    /// Vanilla parity: the strafing block of `RangedBowAttackGoal.tick`.
    fn update_strafe(&mut self, mob: &dyn PathfinderMob, distance_sqr: f64) {
        if self.strafing_time >= STRAFE_REVIEW_TICKS {
            if rand::random::<f32>() < STRAFE_FLIP_CHANCE {
                self.strafing_clockwise = !self.strafing_clockwise;
            }
            if rand::random::<f32>() < STRAFE_FLIP_CHANCE {
                self.strafing_backwards = !self.strafing_backwards;
            }
            self.strafing_time = 0;
        }

        if self.strafing_time <= -1 {
            return;
        }

        let radius_sqr = self.attack_radius * self.attack_radius;
        if distance_sqr > radius_sqr * STOP_BACKING_OFF_AT {
            self.strafing_backwards = false;
        } else if distance_sqr < radius_sqr * START_BACKING_OFF_AT {
            self.strafing_backwards = true;
        }

        let forward = if self.strafing_backwards {
            -STRAFE_STRENGTH
        } else {
            STRAFE_STRENGTH
        };
        let right = if self.strafing_clockwise {
            STRAFE_STRENGTH
        } else {
            -STRAFE_STRENGTH
        };
        mob.mob_base()
            .controls()
            .lock()
            .move_control
            .strafe(forward, right);
    }
}

impl Goal for RangedBowAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }

    fn stop(&mut self, _mob: &dyn PathfinderMob) {
        self.seen_time = 0;
        self.attack_time = -1;
        self.strafing_time = -1;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };
        let target_position = target.position();
        let distance_sqr = target_position.distance_squared(mob.position());
        let in_range = distance_sqr <= self.attack_radius * self.attack_radius;

        // Vanilla only fires once the target has stayed visible for a moment,
        // which is what stops an archer snapping off a shot the instant it
        // rounds a corner.
        if in_range {
            self.seen_time += 1;
        } else {
            self.seen_time = 0;
        }

        if in_range && self.seen_time >= SEEN_TIME_BEFORE_FIRING {
            mob.mob_base().navigation().lock().stop();
            self.strafing_time += 1;
        } else {
            mob.move_to_pos(target_position, self.speed_modifier);
            self.strafing_time = -1;
        }

        self.update_strafe(mob, distance_sqr);

        if !in_range || self.seen_time < SEEN_TIME_BEFORE_FIRING {
            return;
        }

        if self.attack_time > 0 {
            self.attack_time -= 1;
            return;
        }

        (self.fire)(mob, target_position);
        self.attack_time = self.attack_interval;
    }
}
