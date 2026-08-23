use glam::DVec3;

use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::ai::path::PathType;
use crate::entity::{PathfinderMob, SharedEntity};

/// How often the pet re-paths toward its owner.
///
/// Vanilla parity: the `adjustedTickDelay(10)` of `FollowOwnerGoal.tick`.
const RECALC_PATH_INTERVAL: i32 = 10;

/// Walks a pet back to the player that tamed it.
///
/// Vanilla parity: `FollowOwnerGoal`.
pub struct FollowOwnerGoal {
    owner: Option<SharedEntity>,
    speed_modifier: f64,
    time_to_recalc_path: i32,
    start_distance: f32,
    stop_distance: f32,
    old_water_cost: f32,
}

impl FollowOwnerGoal {
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64, start_distance: f32, stop_distance: f32) -> Self {
        Self {
            owner: None,
            speed_modifier,
            time_to_recalc_path: 0,
            start_distance,
            stop_distance,
            old_water_cost: 0.0,
        }
    }
}

impl Goal for FollowOwnerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(pet) = mob.as_tamable_animal() else {
            return false;
        };
        let Some(owner) = pet.owner() else {
            return false;
        };
        if pet.unable_to_move_to_owner() {
            return false;
        }
        if mob.position().distance_squared(owner.position())
            < f64::from(self.start_distance * self.start_distance)
        {
            return false;
        }

        self.owner = Some(owner);
        true
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if mob.mob_base().navigation().lock().is_done() {
            return false;
        }
        let Some(pet) = mob.as_tamable_animal() else {
            return false;
        };
        if pet.unable_to_move_to_owner() {
            return false;
        }
        let Some(owner) = &self.owner else {
            return false;
        };

        mob.position().distance_squared(owner.position())
            > f64::from(self.stop_distance * self.stop_distance)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.time_to_recalc_path = 0;
        self.old_water_cost = mob.get_pathfinding_malus(PathType::Water);
        mob.set_pathfinding_malus(PathType::Water, 0.0);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.owner = None;
        mob.mob_base().navigation().lock().stop();
        mob.set_pathfinding_malus(PathType::Water, self.old_water_cost);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(owner) = self.owner.clone() else {
            return;
        };
        let Some(pet) = mob.as_tamable_animal() else {
            return;
        };

        let owner_far_away = pet.should_try_teleport_to_owner();
        if !owner_far_away {
            let position = owner.position();
            mob.mob_base().controls().lock().look_control.set_look_at(
                DVec3::new(position.x, owner.get_eye_y(), position.z),
                10.0,
                mob.max_head_x_rot(),
            );
        }

        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path > 0 {
            return;
        }

        self.time_to_recalc_path = reduced_tick_delay(RECALC_PATH_INTERVAL);
        if owner_far_away {
            pet.try_to_teleport_to_owner();
        } else {
            mob.move_to_pos(owner.position(), self.speed_modifier);
        }
    }
}
