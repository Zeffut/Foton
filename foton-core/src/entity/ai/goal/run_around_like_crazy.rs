//! Bucking a rider off an untamed horse.
//!
//! Vanilla parity: `RunAroundLikeCrazyGoal`. This is the whole taming loop: a
//! wild horse bolts with whoever climbed on, and every so often rolls its temper
//! against a random number. Win and the horse is tamed, lose and the rider is
//! thrown, five temper richer.

use glam::DVec3;

use super::random_pos::default_random_pos;
use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;
use foton_utils::entity_events::EntityStatus;

/// How far the horse bolts.
///
/// Vanilla parity: the `DefaultRandomPos.getPos(horse, 5, 4)` of `canUse`.
const BOLT_HORIZONTAL_RANGE: i32 = 5;

/// How far up or down the bolt destination may sit.
const BOLT_VERTICAL_RANGE: i32 = 4;

/// Average ticks between taming rolls.
///
/// Vanilla parity: the `adjustedTickDelay(50)` of `tick`.
const TAME_ROLL_INTERVAL: i32 = 50;

/// Temper a failed taming attempt is worth.
///
/// Vanilla parity: the `modifyTemper(5)` of `tick`.
const TEMPER_PER_FAILED_ATTEMPT: i32 = 5;

/// Bolts with an unwelcome rider until the horse gives in or throws them.
///
/// Vanilla parity: `RunAroundLikeCrazyGoal`.
pub struct RunAroundLikeCrazyGoal {
    speed_modifier: f64,
    destination: Option<DVec3>,
}

impl RunAroundLikeCrazyGoal {
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            destination: None,
        }
    }
}

impl Goal for RunAroundLikeCrazyGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(horse) = mob.as_abstract_horse() else {
            return false;
        };
        if horse.is_mob_controlled() || horse.is_tamed() || !mob.is_vehicle() {
            return false;
        }

        self.destination = default_random_pos(mob, BOLT_HORIZONTAL_RANGE, BOLT_VERTICAL_RANGE);
        self.destination.is_some()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(destination) = self.destination {
            mob.move_to_pos(destination, self.speed_modifier);
        }
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(horse) = mob.as_abstract_horse() else {
            return false;
        };
        !horse.is_tamed() && mob.is_path_finding() && mob.is_vehicle()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(horse) = mob.as_abstract_horse() else {
            return;
        };
        if horse.is_tamed() || rand::random_range(0..reduced_tick_delay(TAME_ROLL_INTERVAL)) != 0 {
            return;
        }

        let Some(passenger) = mob.first_passenger() else {
            return;
        };

        if let Some(player) = passenger.as_player() {
            let temper = horse.temper();
            let max_temper = horse.max_temper();
            if max_temper > 0 && rand::random_range(0..max_temper) < temper {
                horse.tame_with_name(player);
                return;
            }

            horse.modify_temper(TEMPER_PER_FAILED_ATTEMPT);
        }

        mob.eject_passengers();
        horse.make_mad();
        mob.broadcast_entity_event(EntityStatus::TamingFailed);
    }
}
