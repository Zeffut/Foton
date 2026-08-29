//! Vanilla `StartAttacking`.

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::{Mob, SharedEntity};

/// Whether this mob may pick a fight at all.
type AttackCondition = Box<dyn Fn(&BrainContext<'_>) -> bool + Send>;
/// Who to fight.
type TargetFinder = Box<dyn Fn(&BrainContext<'_>) -> Option<SharedEntity> + Send>;

/// Writes `ATTACK_TARGET` when the mob finds something to fight.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.StartAttacking`.
pub struct StartAttacking {
    can_attack: AttackCondition,
    find_target: TargetFinder,
}

impl StartAttacking {
    /// Attacks whatever `find_target` picks.
    ///
    /// Vanilla parity: `StartAttacking.create(TargetFinder)`.
    #[must_use]
    pub fn new(
        find_target: impl Fn(&BrainContext<'_>) -> Option<SharedEntity> + Send + 'static,
    ) -> Self {
        Self::conditional(|_| true, find_target)
    }

    /// Attacks only while `can_attack` agrees.
    ///
    /// Vanilla parity: `StartAttacking.create(StartAttackingCondition, TargetFinder)`.
    #[must_use]
    pub fn conditional(
        can_attack: impl Fn(&BrainContext<'_>) -> bool + Send + 'static,
        find_target: impl Fn(&BrainContext<'_>) -> Option<SharedEntity> + Send + 'static,
    ) -> Self {
        Self {
            can_attack: Box::new(can_attack),
            find_target: Box::new(find_target),
        }
    }
}

impl Trigger for StartAttacking {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ATTACK_TARGET.id())
            || !(self.can_attack)(ctx)
        {
            return false;
        }

        let Some(target) = (self.find_target)(ctx) else {
            return false;
        };
        let Some(living) = target.as_living_entity() else {
            return false;
        };
        if !Mob::can_attack(ctx.mob(), living) {
            return false;
        }

        brain.set_memory(
            memory_module_types::ATTACK_TARGET,
            EntityMemory::new(&target),
        );
        brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
        true
    }

    fn debug_name(&self) -> &'static str {
        "StartAttacking"
    }
}
