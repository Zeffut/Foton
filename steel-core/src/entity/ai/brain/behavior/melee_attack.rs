//! Vanilla `MeleeAttack`.

use steel_utils::types::InteractionHand;

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::{Mob, PathfinderMob};

/// Whether this mob may swing right now.
type AttackGuard = Box<dyn Fn(&dyn PathfinderMob) -> bool + Send>;

/// Swings at `ATTACK_TARGET` when it is in range and visible.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.MeleeAttack`.
pub struct MeleeAttack {
    can_attack: AttackGuard,
    cooldown_between_attacks: i64,
}

impl MeleeAttack {
    /// Swings every `cooldown_between_attacks` ticks at most.
    ///
    /// Vanilla parity: `MeleeAttack.create(int)`.
    #[must_use]
    pub fn new(cooldown_between_attacks: i64) -> Self {
        Self::conditional(|_| true, cooldown_between_attacks)
    }

    /// Swings only while `can_attack` agrees.
    ///
    /// Vanilla parity: `MeleeAttack.create(Predicate<T>, int)`.
    #[must_use]
    pub fn conditional(
        can_attack: impl Fn(&dyn PathfinderMob) -> bool + Send + 'static,
        cooldown_between_attacks: i64,
    ) -> Self {
        Self {
            can_attack: Box::new(can_attack),
            cooldown_between_attacks,
        }
    }
}

impl Trigger for MeleeAttack {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::ATTACK_COOLING_DOWN.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ATTACK_COOLING_DOWN.id()) {
            return false;
        }
        let Some(remembered) = brain.get_memory(memory_module_types::ATTACK_TARGET) else {
            return false;
        };
        let Some(visible) = brain.get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        else {
            return false;
        };
        let Some(target) = remembered.get() else {
            return false;
        };
        let Some(living_target) = target.as_living_entity() else {
            return false;
        };

        if !(self.can_attack)(ctx.mob())
            || !ctx.mob().is_within_melee_attack_range(living_target)
            || !visible.contains_entity(remembered.id())
        {
            return false;
        }

        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );
        ctx.mob().swing(InteractionHand::MainHand, true);
        let _ = Mob::do_hurt_target(ctx.mob(), ctx.world(), &target);
        brain.set_memory_with_expiry(
            memory_module_types::ATTACK_COOLING_DOWN,
            true,
            self.cooldown_between_attacks,
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "MeleeAttack"
    }
}
