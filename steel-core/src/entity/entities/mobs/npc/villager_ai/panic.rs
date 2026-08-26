//! What a frightened villager does.
//!
//! Vanilla parity: `VillagerPanicTrigger` and `VillagerCalmDown`, the pair that
//! switch a villager into and back out of [`Activity::Panic`].

use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, Trigger,
};
use crate::entity::ai::brain::memory::memory_module_types;

/// Vanilla parity: `VillagerCalmDown.SAFE_DISTANCE_FROM_DANGER`, squared.
const SAFE_DISTANCE_FROM_DANGER_SQR: f64 = 36.0;

/// Switches a villager to PANIC while something is hurting or hunting it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.VillagerPanicTrigger`,
/// which sits at priority zero of the core package so it overrides whatever the
/// schedule had the villager doing.
///
/// MISSING FOUNDATION: vanilla's `tick` also calls
/// `Villager.spawnGolemIfNeeded` every hundred ticks, which is how a frightened
/// village summons an iron golem. That needs the `GOLEM_DETECTED` sensor and
/// the golem spawn rules, neither of which Steel has, so the panic itself is
/// ported and the golem is not.
pub struct VillagerPanicTrigger;

impl VillagerPanicTrigger {
    /// Vanilla parity: `VillagerPanicTrigger.hasHostile`.
    #[must_use]
    pub fn has_hostile(ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::NEAREST_HOSTILE.id())
    }

    /// Vanilla parity: `VillagerPanicTrigger.isHurt`.
    #[must_use]
    pub fn is_hurt(ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::HURT_BY.id())
    }
}

impl TimedBehavior for VillagerPanicTrigger {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::is_hurt(ctx) || Self::has_hostile(ctx)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if !Self::is_hurt(ctx) && !Self::has_hostile(ctx) {
            return;
        }
        let brain = ctx.brain();
        if !brain.is_active(Activity::Panic) {
            // Vanilla drops everything it was walking toward or talking to, so
            // the panic does not inherit a stale destination.
            brain.erase_memory(memory_module_types::PATH.id());
            brain.erase_memory(memory_module_types::WALK_TARGET.id());
            brain.erase_memory(memory_module_types::LOOK_TARGET.id());
            brain.erase_memory(memory_module_types::BREED_TARGET.id());
            brain.erase_memory(memory_module_types::INTERACTION_TARGET.id());
        }
        brain.set_active_activity_if_possible(Activity::Panic);
    }

    fn debug_name(&self) -> &'static str {
        "VillagerPanicTrigger"
    }
}

/// Ends the panic once nothing dangerous is left in sight.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.VillagerCalmDown`.
/// It is what puts a villager back on its schedule, which is why it re-reads it
/// on the spot rather than waiting for the PANIC package's own update.
pub struct VillagerCalmDown;

impl Trigger for VillagerCalmDown {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::HURT_BY.id(),
            memory_module_types::HURT_BY_ENTITY.id(),
            memory_module_types::NEAREST_HOSTILE.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let hurt_by_entity_close = brain
            .get_memory(memory_module_types::HURT_BY_ENTITY)
            .and_then(|memory| memory.get())
            .is_some_and(|entity| {
                entity.position().distance_squared(ctx.mob().position())
                    <= SAFE_DISTANCE_FROM_DANGER_SQR
            });
        let feel_scared = brain.has_memory_value(memory_module_types::HURT_BY.id())
            || brain.has_memory_value(memory_module_types::NEAREST_HOSTILE.id())
            || hurt_by_entity_close;
        if !feel_scared {
            brain.erase_memory(memory_module_types::HURT_BY.id());
            brain.erase_memory(memory_module_types::HURT_BY_ENTITY.id());
            brain.update_activity_from_schedule(ctx.world(), ctx.game_time());
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "VillagerCalmDown"
    }
}
