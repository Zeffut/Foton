use steel_utils::Downcast as _;

use super::selector::{Goal, GoalControls};
use crate::entity::entities::ParrotEntity;
use crate::entity::{Entity, Mob, PathfinderMob, TamableAnimal};

/// Puts a tamed parrot on its owner's shoulder.
///
/// Vanilla parity: `LandOnOwnersShoulderGoal`. The goal is uninterruptible only
/// once the bird has actually landed, which is what stops another goal pulling
/// it off mid-hop.
pub struct LandOnOwnersShoulderGoal {
    is_sitting_on_shoulder: bool,
}

impl LandOnOwnersShoulderGoal {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            is_sitting_on_shoulder: false,
        }
    }
}

impl Goal for LandOnOwnersShoulderGoal {
    fn controls(&self) -> GoalControls {
        // Vanilla parity: `LandOnOwnersShoulderGoal` never calls `setFlags`.
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(parrot) = mob.downcast_ref::<ParrotEntity>() else {
            return false;
        };
        let Some(owner) = parrot.owner() else {
            return false;
        };
        let Some(player) = owner.as_player() else {
            return false;
        };

        let owner_can_be_sat_on = !player.is_spectator()
            && !player.abilities.lock().flying
            && !player.is_in_water()
            && !player.is_in_powder_snow();

        !parrot.is_ordered_to_sit() && owner_can_be_sat_on && parrot.can_sit_on_shoulder()
    }

    fn is_interruptable(&self) -> bool {
        !self.is_sitting_on_shoulder
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.is_sitting_on_shoulder = false;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if self.is_sitting_on_shoulder {
            return;
        }
        let Some(parrot) = mob.downcast_ref::<ParrotEntity>() else {
            return;
        };
        if parrot.is_in_sitting_pose() || parrot.is_leashed() {
            return;
        }
        let Some(owner) = parrot.owner() else {
            return;
        };
        let Some(player) = owner.as_player() else {
            return;
        };
        if !parrot.bounding_box().intersects(player.bounding_box()) {
            return;
        }

        self.is_sitting_on_shoulder = parrot.set_entity_on_shoulder(player);
    }
}
