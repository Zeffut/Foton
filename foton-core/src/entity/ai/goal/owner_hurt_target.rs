//! The two target goals that make a pet fight its owner's battles.
//!
//! Vanilla parity: `OwnerHurtByTargetGoal` and `OwnerHurtTargetGoal`. They are
//! the same goal read in two directions -- one attacks whoever hit the owner,
//! the other attacks whoever the owner hit -- so they share a file here rather
//! than duplicating the timestamp bookkeeping twice.

use super::selector::{Goal, GoalControls};
use super::target_goal::TargetGoalBase;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::{LivingEntity, PathfinderMob, SharedEntity};

/// Which side of the owner's last fight the goal reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnerCombatSide {
    /// Vanilla parity: `OwnerHurtByTargetGoal`, reading `getLastHurtByMob`.
    HurtBy,
    /// Vanilla parity: `OwnerHurtTargetGoal`, reading `getLastHurtMob`.
    Hurt,
}

impl OwnerCombatSide {
    fn combatant(self, owner: &dyn LivingEntity) -> Option<SharedEntity> {
        match self {
            Self::HurtBy => owner.last_hurt_by_mob(),
            Self::Hurt => owner.last_hurt_mob(),
        }
    }

    fn timestamp(self, owner: &dyn LivingEntity) -> i32 {
        match self {
            Self::HurtBy => owner.last_hurt_by_mob_timestamp(),
            Self::Hurt => owner.last_hurt_mob_timestamp(),
        }
    }
}

/// Makes a pet attack whoever its owner is fighting.
pub struct OwnerHurtTargetGoal {
    target_goal: TargetGoalBase,
    side: OwnerCombatSide,
    combatant: Option<SharedEntity>,
    timestamp: i32,
}

impl OwnerHurtTargetGoal {
    /// Creates vanilla `OwnerHurtByTargetGoal`.
    #[must_use]
    pub(crate) const fn hurt_by_owner_attacker() -> Self {
        Self::new(OwnerCombatSide::HurtBy)
    }

    /// Creates vanilla `OwnerHurtTargetGoal`.
    #[must_use]
    pub(crate) const fn owners_current_victim() -> Self {
        Self::new(OwnerCombatSide::Hurt)
    }

    const fn new(side: OwnerCombatSide) -> Self {
        Self {
            target_goal: TargetGoalBase::new(false, false),
            side,
            combatant: None,
            timestamp: 0,
        }
    }
}

impl Goal for OwnerHurtTargetGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::TARGET
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(pet) = mob.as_tamable_animal() else {
            return false;
        };
        if !pet.is_tame() || pet.is_ordered_to_sit() {
            return false;
        }
        let Some(owner) = pet.owner() else {
            return false;
        };
        let Some(owner_living) = owner.as_living_entity() else {
            return false;
        };

        self.combatant = self.side.combatant(owner_living);
        let timestamp = self.side.timestamp(owner_living);
        if timestamp == self.timestamp {
            return false;
        }

        let Some(combatant) = self.combatant.clone() else {
            return false;
        };
        let Some(combatant_living) = combatant.as_living_entity() else {
            return false;
        };
        if !self.target_goal.can_attack(
            mob,
            Some(combatant_living),
            &TargetingConditions::for_combat(),
        ) {
            return false;
        }

        pet.wants_to_attack(combatant_living, owner.as_ref())
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.target_goal.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let _ = mob.set_target(self.combatant.as_ref());
        if let Some(pet) = mob.as_tamable_animal()
            && let Some(owner) = pet.owner()
            && let Some(owner_living) = owner.as_living_entity()
        {
            self.timestamp = self.side.timestamp(owner_living);
        }
        self.target_goal.start();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.target_goal.stop(mob);
        self.combatant = None;
    }
}
