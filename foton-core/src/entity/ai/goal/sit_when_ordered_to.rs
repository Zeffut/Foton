use super::selector::{Goal, GoalControls};
use crate::entity::{PathfinderMob, TELEPORT_WHEN_DISTANCE_IS_SQ, TamableAnimal};

/// Keeps a pet sitting where its owner told it to.
///
/// Vanilla parity: `SitWhenOrderedToGoal`.
pub struct SitWhenOrderedToGoal;

impl SitWhenOrderedToGoal {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }

    fn can_sit(pet: &dyn TamableAnimal) -> bool {
        let ordered_to_sit = pet.is_ordered_to_sit();
        if !ordered_to_sit && !pet.is_tame() {
            return false;
        }
        if pet.is_in_water() || !pet.on_ground() {
            return false;
        }

        let Some(owner) = pet.owner() else {
            return true;
        };
        // A pet in a different world cannot be near an owner being attacked.
        let Some(owner_living) = owner.as_living_entity() else {
            return true;
        };
        let owner_in_trouble = pet.position().distance_squared(owner.position())
            < TELEPORT_WHEN_DISTANCE_IS_SQ
            && owner_living.last_hurt_by_mob().is_some();

        !owner_in_trouble && ordered_to_sit
    }
}

impl Goal for SitWhenOrderedToGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.as_tamable_animal().is_some_and(Self::can_sit)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.as_tamable_animal()
            .is_some_and(TamableAnimal::is_ordered_to_sit)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
        if let Some(pet) = mob.as_tamable_animal() {
            pet.set_in_sitting_pose(true);
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(pet) = mob.as_tamable_animal() {
            pet.set_in_sitting_pose(false);
        }
    }
}
