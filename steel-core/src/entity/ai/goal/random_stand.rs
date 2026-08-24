//! A horse rearing up for no reason at all.
//!
//! Vanilla parity: `RandomStandGoal`. The counter runs on every tick and only
//! clears once it beats a thousand-sided roll, which is why a paddock of horses
//! rears every so often rather than on a fixed beat.

use super::selector::{Goal, GoalControls};
use crate::entity::{AbstractHorse, LivingEntity, PathfinderMob};

/// The die the stand counter is rolled against.
///
/// Vanilla parity: the `random.nextInt(1000)` of `RandomStandGoal.canUse`.
const STAND_ROLL_SIDES: i32 = 1000;

/// The second roll a horse still has to win once its counter is up.
///
/// Vanilla parity: the `random.nextInt(10) == 0` of the same method.
const STAND_CONFIRM_SIDES: i32 = 10;

/// Rears up at random.
///
/// Vanilla parity: `RandomStandGoal`.
pub struct RandomStandGoal {
    next_stand: i32,
    /// Vanilla reads `getAmbientStandInterval()` off the horse in the goal's
    /// constructor; Steel builds goals before the mob exists, so the interval is
    /// read from the mob on the first reset instead.
    initialized: bool,
}

impl RandomStandGoal {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            next_stand: 0,
            initialized: false,
        }
    }

    fn reset_stand_interval(&mut self, horse: &dyn AbstractHorse) {
        self.next_stand = -horse.ambient_stand_interval();
        self.initialized = true;
    }
}

impl Goal for RandomStandGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(horse) = mob.as_abstract_horse() else {
            return false;
        };
        if !self.initialized {
            self.reset_stand_interval(horse);
        }

        self.next_stand += 1;
        if self.next_stand <= 0 || rand::random_range(0..STAND_ROLL_SIDES) >= self.next_stand {
            return false;
        }

        self.reset_stand_interval(horse);
        !LivingEntity::is_immobile(horse) && rand::random_range(0..STAND_CONFIRM_SIDES) == 0
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(horse) = mob.as_abstract_horse() else {
            return;
        };
        horse.stand_if_possible();
        if let Some(sound) = horse.ambient_stand_sound() {
            // Vanilla calls `Entity.playSound(SoundEvent)`, which is a flat 1/1.
            horse.play_sound(sound, 1.0, 1.0);
        }
    }
}
