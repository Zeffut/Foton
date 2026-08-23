use glam::DVec3;

use super::selector::{Goal, GoalControls};
use crate::entity::{PathfinderMob, SharedEntity};

/// Ticks between two bites.
///
/// Vanilla parity: the `this.attackTime = 20` of `OcelotAttackGoal.tick`.
const ATTACK_INTERVAL_TICKS: i32 = 20;

/// Distance past which the cat gives up the chase.
///
/// Vanilla parity: the `225.0` of `OcelotAttackGoal`, fifteen blocks.
const GIVE_UP_DISTANCE_SQR: f64 = 225.0;

/// Distance inside which the cat sprints rather than stalks.
///
/// Vanilla parity: the `16.0` of `OcelotAttackGoal.tick`.
const SPRINT_DISTANCE_SQR: f64 = 16.0;

/// Speed while walking toward prey that is still far off.
const WALK_SPEED_MODIFIER: f64 = 0.8;

/// Speed while closing the last few blocks.
const SPRINT_SPEED_MODIFIER: f64 = 1.33;

/// Speed while creeping up on prey.
const CROUCH_SPEED_MODIFIER: f64 = 0.6;

/// The stalk-and-pounce attack a cat or an ocelot uses on chickens.
///
/// Vanilla parity: `OcelotAttackGoal`. The three speeds are what the cat's
/// `customServerAiStep` reads back to decide between crouching, walking and
/// sprinting, so they are the goal's whole visible output.
pub struct OcelotAttackGoal {
    target: Option<SharedEntity>,
    attack_time: i32,
}

impl OcelotAttackGoal {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            target: None,
            attack_time: 0,
        }
    }
}

impl Goal for OcelotAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.target = mob.target();
        self.target.is_some()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(target) = self.target.clone() else {
            return false;
        };
        if !target.is_alive() {
            return false;
        }
        if mob.position().distance_squared(target.position()) > GIVE_UP_DISTANCE_SQR {
            return false;
        }

        !mob.mob_base().navigation().lock().is_done() || self.can_use(mob)
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.target = None;
        mob.mob_base().navigation().lock().stop();
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = self.target.clone() else {
            return;
        };

        let target_position = target.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(target_position.x, target.get_eye_y(), target_position.z),
            30.0,
            30.0,
        );

        let melee_radius = f64::from(mob.bounding_box().width() * 2.0);
        let melee_radius_sqr = melee_radius * melee_radius;
        let distance_sqr = mob.position().distance_squared(target_position);
        let speed_modifier =
            if distance_sqr > melee_radius_sqr && distance_sqr < SPRINT_DISTANCE_SQR {
                SPRINT_SPEED_MODIFIER
            } else if distance_sqr < GIVE_UP_DISTANCE_SQR {
                CROUCH_SPEED_MODIFIER
            } else {
                WALK_SPEED_MODIFIER
            };

        mob.move_to_pos(target_position, speed_modifier);
        self.attack_time = (self.attack_time - 1).max(0);
        if distance_sqr > melee_radius_sqr || self.attack_time > 0 {
            return;
        }

        self.attack_time = ATTACK_INTERVAL_TICKS;
        if let Some(world) = mob.level() {
            let _ = mob.do_hurt_target(&world, &target);
        }
    }
}
