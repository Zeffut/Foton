//! Raider celebration goal.
//!
//! Vanilla parity: `Raider.RaiderCelebration`. When a raid wins, the survivors
//! stop attacking, jump on the spot and shout. It is the only place the
//! celebrating flag is ever set, and the flag is what puts a vindicator's arms
//! down and a spellcaster's hands together.
//!
//! The goal is written whole, but it can never start: its gate is a raid the
//! village lost, and Steel has no raid to lose. See
//! [`crate::entity::raider`] for why.

use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::{LivingEntity, PathfinderMob};

/// Average ticks between two celebratory shouts.
///
/// Vanilla parity: the `nextInt(adjustedTickDelay(100))` of `tick`.
const SHOUT_INTERVAL: i32 = 100;

/// Average ticks between two hops.
///
/// Vanilla parity: the `nextInt(adjustedTickDelay(50))` of `tick`.
const JUMP_INTERVAL: i32 = 50;

/// Jumps and shouts over a fallen village.
///
/// Vanilla parity: `Raider.RaiderCelebration`.
pub(crate) struct RaiderCelebrationGoal;

impl RaiderCelebrationGoal {
    /// Creates the goal.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for RaiderCelebrationGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for RaiderCelebrationGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        LivingEntity::is_alive(mob)
            && mob.target().is_none()
            && raider
                .current_raid_status()
                .is_some_and(|status| status.loss)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(raider) = mob.as_raider() {
            raider.set_celebrating(true);
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(raider) = mob.as_raider() {
            raider.set_celebrating(false);
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(raider) = mob.as_raider() else {
            return;
        };

        // The two bounds are halved because this goal does not ask to be
        // ticked every tick and a mob's goals only tick on even ticks
        // otherwise; that is vanilla's `Goal.adjustedTickDelay`.
        if !mob.is_silent() && rand::random_range(0..reduced_tick_delay(SHOUT_INTERVAL)) == 0 {
            mob.play_sound(raider.celebrate_sound(), 1.0, 1.0);
        }
        if !mob.is_passenger() && rand::random_range(0..reduced_tick_delay(JUMP_INTERVAL)) == 0 {
            mob.jump_control_jump();
        }
    }
}
