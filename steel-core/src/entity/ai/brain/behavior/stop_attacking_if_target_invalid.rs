//! Vanilla `StopAttackingIfTargetInvalid`.

use std::sync::Arc;

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::{LivingEntity, Mob, SharedEntity};

/// Vanilla parity: `StopAttackingIfTargetInvalid.TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE`.
const TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE: i64 = 200;

/// An extra reason to give up on a target.
type StopAttackCondition = Box<dyn Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send>;
/// What to do once the target is dropped.
type TargetErasedCallback = Box<dyn Fn(&BrainContext<'_>, &SharedEntity) + Send>;

/// Erases `ATTACK_TARGET` once it stops being worth attacking.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.StopAttackingIfTargetInvalid`.
pub struct StopAttackingIfTargetInvalid {
    stop_attacking_when: StopAttackCondition,
    on_target_erased: TargetErasedCallback,
    can_grow_tired_of_trying_to_reach_target: bool,
}

impl Default for StopAttackingIfTargetInvalid {
    fn default() -> Self {
        Self::new()
    }
}

impl StopAttackingIfTargetInvalid {
    /// Drops a target that dies, leaves, or stays unreachable too long.
    ///
    /// Vanilla parity: `StopAttackingIfTargetInvalid.create()`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stop_attacking_when: Box::new(|_, _| false),
            on_target_erased: Box::new(|_, _| {}),
            can_grow_tired_of_trying_to_reach_target: true,
        }
    }

    /// Adds a reason of the mob's own to give up.
    ///
    /// Vanilla parity: `StopAttackingIfTargetInvalid.create(StopAttackCondition)`.
    #[must_use]
    pub fn when(
        mut self,
        stop_attacking_when: impl Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send + 'static,
    ) -> Self {
        self.stop_attacking_when = Box::new(stop_attacking_when);
        self
    }

    /// Runs `on_target_erased` as the target is dropped.
    ///
    /// Vanilla parity: `StopAttackingIfTargetInvalid.create(TargetErasedCallback)`.
    #[must_use]
    pub fn on_erased(
        mut self,
        on_target_erased: impl Fn(&BrainContext<'_>, &SharedEntity) + Send + 'static,
    ) -> Self {
        self.on_target_erased = Box::new(on_target_erased);
        self
    }

    /// Keeps chasing a target it cannot reach.
    ///
    /// Vanilla parity: `canGrowTiredOfTryingToReachTarget = false`.
    #[must_use]
    pub const fn never_tiring(mut self) -> Self {
        self.can_grow_tired_of_trying_to_reach_target = false;
        self
    }

    /// Vanilla parity: `StopAttackingIfTargetInvalid.isTiredOfTryingToReachTarget`.
    fn is_tired_of_trying(ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .get_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE)
            .is_some_and(|since| ctx.game_time() - since > TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE)
    }
}

impl Trigger for StopAttackingIfTargetInvalid {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(remembered) = brain.get_memory(memory_module_types::ATTACK_TARGET) else {
            return false;
        };

        let Some(target) = remembered.get() else {
            // The target left the world entirely, so there is nothing to hand
            // to the callback.
            brain.erase_memory(memory_module_types::ATTACK_TARGET.id());
            return true;
        };

        let still_valid = target.as_living_entity().is_some_and(|living| {
            Mob::can_attack(ctx.mob(), living) && LivingEntity::is_alive(living)
        }) && !(self.can_grow_tired_of_trying_to_reach_target
            && Self::is_tired_of_trying(ctx))
            && target
                .level()
                .is_some_and(|level| Arc::ptr_eq(&level, ctx.world()))
            && !(self.stop_attacking_when)(ctx, &target);
        if still_valid {
            return true;
        }

        (self.on_target_erased)(ctx, &target);
        brain.erase_memory(memory_module_types::ATTACK_TARGET.id());
        true
    }

    fn debug_name(&self) -> &'static str {
        "StopAttackingIfTargetInvalid"
    }
}
