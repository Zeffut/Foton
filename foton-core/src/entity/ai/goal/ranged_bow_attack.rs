//! Ranged bow attack goal.
//!
//! Vanilla parity: `RangedBowAttackGoal`. A skeleton does not stand still and
//! shoot: it circles its target, backs off when crowded, closes when the target
//! runs, and flips direction now and then so it cannot be strafed into a corner.

use foton_registry::vanilla_items;
use foton_utils::types::Difficulty;

use super::selector::{Goal, GoalControls};
use crate::behavior::items::BowItem;
use crate::entity::PathfinderMob;
use crate::entity::projectile::weapon_holding_hand;

/// Ticks a target must stay visible before the archer commits to a shot.
///
/// Vanilla parity: the `seeTime >= 20` of `RangedBowAttackGoal.tick`.
const SEEN_TIME_BEFORE_FIRING: i32 = 20;

/// Ticks the bow is held before it looses.
///
/// Vanilla parity: the `getTicksUsingItem() >= 20` of the same method. This is
/// the draw, and it is also the animation: the client renders the pull from
/// the using-item flag `start_using_item` sets.
const FULL_DRAW_TICKS: i32 = 20;

/// How far out of sight the archer may be before it gives up on a shot.
///
/// Vanilla parity: the `seeTime < -60` and `seeTime >= -60` gates, which is why
/// `seen_time` counts down rather than resetting to zero.
const LOST_SIGHT_GRACE: i32 = -60;

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
    /// Ticks between two shots on Hard.
    hard_attack_interval: i32,
    /// What to do when the archer decides to shoot, given the draw's power.
    fire: FireFn,
}

/// What a mob does when its bow releases.
///
/// Vanilla parity: `RangedAttackMob.performRangedAttack(target, power)`, whose
/// `power` is `BowItem.getPowerForTime` over the draw just finished.
pub(crate) type FireFn = fn(&dyn PathfinderMob, glam::DVec3, f32);

impl RangedBowAttackGoal {
    /// Creates the goal for an archer that shoots at one rate on every
    /// difficulty.
    ///
    /// Vanilla parity: the plain `new RangedBowAttackGoal<>(this, .., 20, ..)`
    /// an illusioner is built with, which nothing ever reassesses.
    #[must_use]
    pub(crate) const fn new(
        attack_interval: i32,
        attack_radius: f64,
        speed_modifier: f64,
        fire: FireFn,
    ) -> Self {
        Self::by_difficulty(
            attack_interval,
            attack_interval,
            attack_radius,
            speed_modifier,
            fire,
        )
    }

    /// Creates the goal for an archer that shoots faster on Hard.
    ///
    /// Vanilla parity: `AbstractSkeleton.reassessWeaponGoal`, which feeds
    /// `setMinAttackInterval` with `getHardAttackInterval()` on Hard and
    /// `getAttackInterval()` otherwise. Vanilla re-reads that only when the
    /// difficulty or the held weapon changes; Foton reads it as the shot
    /// lands, which is the same answer at the only moment it is used.
    #[must_use]
    pub(crate) const fn by_difficulty(
        hard_attack_interval: i32,
        attack_interval: i32,
        attack_radius: f64,
        speed_modifier: f64,
        fire: FireFn,
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
            hard_attack_interval,
            fire,
        }
    }

    /// Ticks this archer waits between shots on the world's difficulty.
    fn min_attack_interval(&self, mob: &dyn PathfinderMob) -> i32 {
        let hard = mob
            .level()
            .is_some_and(|world| world.difficulty() == Difficulty::Hard);
        if hard {
            self.hard_attack_interval
        } else {
            self.attack_interval
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

    /// Vanilla parity: `canUse`, which is a target *and* a bow -- a skeleton
    /// that lost its bow falls back to the melee goal instead.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some() && is_holding_bow(mob)
    }

    /// Vanilla parity: `stop`, which lowers the bow. Without this the mob keeps
    /// the using-item flag set and the client draws it aiming at nothing.
    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.seen_time = 0;
        self.attack_time = -1;
        self.strafing_time = -1;
        mob.stop_using_item();
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

        // Vanilla counts line of sight, not range, and lets the count run
        // negative -- that is what the -60 grace below is measured against.
        let has_line_of_sight = mob.has_line_of_sight_cached(target.as_ref());
        if has_line_of_sight != (self.seen_time > 0) {
            self.seen_time = 0;
        }
        if has_line_of_sight {
            self.seen_time += 1;
        } else {
            self.seen_time -= 1;
        }

        if in_range && self.seen_time >= SEEN_TIME_BEFORE_FIRING {
            mob.mob_base().navigation().lock().stop();
            self.strafing_time += 1;
        } else {
            mob.move_to_pos(target_position, self.speed_modifier);
            self.strafing_time = -1;
        }

        self.update_strafe(mob, distance_sqr);

        // Vanilla parity: the draw. The bow is raised first and only looses
        // twenty ticks later, which is both the delay and the animation -- the
        // client renders the pull from the using-item flag.
        if mob.is_using_item() {
            if !has_line_of_sight && self.seen_time < LOST_SIGHT_GRACE {
                mob.stop_using_item();
            } else if has_line_of_sight {
                let pull_ticks = mob.ticks_using_item();
                if pull_ticks >= FULL_DRAW_TICKS {
                    mob.stop_using_item();
                    (self.fire)(mob, target_position, BowItem::power_for_time(pull_ticks));
                    self.attack_time = self.min_attack_interval(mob);
                }
            }
        } else {
            self.attack_time -= 1;
            if self.attack_time <= 0 && self.seen_time >= LOST_SIGHT_GRACE {
                mob.start_using_item(weapon_holding_hand(mob, &vanilla_items::BOW));
            }
        }
    }
}

/// Whether this archer still has a bow to draw.
///
/// Vanilla parity: `RangedBowAttackGoal.isHoldingBow`.
fn is_holding_bow(mob: &dyn PathfinderMob) -> bool {
    mob.is_holding(&mut |item| item.is(&vanilla_items::BOW))
}
