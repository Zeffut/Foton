//! Keeping a distance and shooting from it.
//!
//! Vanilla parity: `RangedAttackGoal`. Shared by every mob that fights without
//! closing -- the witch, the snow golem, the blaze, the llama -- so what it
//! fires is the caller's business and everything else is here.

use glam::DVec3;

use super::selector::{Goal, GoalControls};
use crate::entity::{PathfinderMob, SharedEntity};

/// Ticks of unbroken sight before the mob stops closing in.
///
/// Vanilla parity: the `seeTime >= 5` of `RangedAttackGoal.tick`. It is why a
/// mob that has just spotted you still walks a step before it fires.
const SIGHT_TICKS_BEFORE_HOLDING: i32 = 5;

/// How fast the head turns to track the target.
///
/// Vanilla parity: the `setLookAt(target, 30.0F, 30.0F)` of the same method.
const LOOK_TURN_RATE: f32 = 30.0;

/// Fires one shot at the target.
///
/// `power` is the pull, between a tenth and one, scaled by how far away the
/// target is; vanilla hands the same number to every ranged mob and lets each
/// decide what it means.
pub(crate) type FireAtTarget = fn(&dyn PathfinderMob, &SharedEntity, f32);

/// Backs off to a chosen range and shoots from there.
///
/// Vanilla parity: `RangedAttackGoal`.
pub(crate) struct RangedAttackGoal {
    /// Ticks until the next shot, or negative before the first.
    attack_time: i32,
    /// Ticks the target has been continuously visible.
    see_time: i32,
    /// Shortest gap between shots, used point blank.
    attack_interval_min: i32,
    /// Longest gap between shots, used at the edge of the range.
    attack_interval_max: i32,
    /// How far the mob is willing to shoot from.
    attack_radius: f32,
    /// How fast it walks while closing.
    speed_modifier: f64,
    /// What it fires.
    fire: FireAtTarget,
}

impl RangedAttackGoal {
    /// Creates a ranged attack with one fixed interval.
    #[must_use]
    pub(crate) fn new(
        speed_modifier: f64,
        attack_interval: i32,
        attack_radius: f32,
        fire: FireAtTarget,
    ) -> Self {
        Self::with_interval_range(
            speed_modifier,
            attack_interval,
            attack_interval,
            attack_radius,
            fire,
        )
    }

    /// Creates a ranged attack that fires faster up close.
    #[must_use]
    pub(crate) fn with_interval_range(
        speed_modifier: f64,
        attack_interval_min: i32,
        attack_interval_max: i32,
        attack_radius: f32,
        fire: FireAtTarget,
    ) -> Self {
        Self {
            attack_time: -1,
            see_time: 0,
            attack_interval_min,
            attack_interval_max,
            attack_radius,
            speed_modifier,
            fire,
        }
    }
}

impl Goal for RangedAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.can_use(mob) || !mob.mob_base().navigation().lock().is_done()
    }

    fn stop(&mut self, _mob: &dyn PathfinderMob) {
        self.see_time = 0;
        self.attack_time = -1;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };

        let distance_sqr = mob.position().distance_squared(target.position());
        let has_line_of_sight = mob.has_line_of_sight_cached(target.as_ref());
        if has_line_of_sight {
            self.see_time += 1;
        } else {
            self.see_time = 0;
        }

        let radius_sqr = f64::from(self.attack_radius * self.attack_radius);
        if distance_sqr <= radius_sqr && self.see_time >= SIGHT_TICKS_BEFORE_HOLDING {
            mob.mob_base().navigation().lock().stop();
        } else {
            let position = target.position();
            mob.mob_base()
                .controls()
                .lock()
                .move_control
                .set_wanted_position(position, self.speed_modifier);
        }

        let look_at = target.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(look_at.x, target.get_eye_y(), look_at.z),
            LOOK_TURN_RATE,
            LOOK_TURN_RATE,
        );

        self.attack_time -= 1;
        if self.attack_time == 0 {
            if !has_line_of_sight {
                return;
            }

            // Vanilla scales both the shot's power and the wait after it by how
            // far the target is: a mob at the edge of its range fires weakly and
            // waits the full interval, one at point blank fires hard and fast.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a distance ratio is small and vanilla floors it"
            )]
            let ratio = (distance_sqr.sqrt() / f64::from(self.attack_radius)) as f32;
            let power = ratio.clamp(0.1, 1.0);
            (self.fire)(mob, &target, power);

            #[expect(
                clippy::cast_possible_truncation,
                reason = "an attack interval is a small tick count"
            )]
            let interval = ratio.mul_add(
                (self.attack_interval_max - self.attack_interval_min) as f32,
                self.attack_interval_min as f32,
            ) as i32;
            self.attack_time = interval;
        } else if self.attack_time < 0 {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an attack interval is a small tick count"
            )]
            let ratio =
                (distance_sqr.sqrt() / f64::from(self.attack_radius)).clamp(0.0, 1.0) as f32;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an attack interval is a small tick count"
            )]
            let interval = ratio.mul_add(
                (self.attack_interval_max - self.attack_interval_min) as f32,
                self.attack_interval_min as f32,
            ) as i32;
            self.attack_time = interval;
        }
    }
}
