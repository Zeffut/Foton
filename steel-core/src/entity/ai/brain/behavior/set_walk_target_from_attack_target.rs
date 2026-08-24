//! Vanilla `SetWalkTargetFromAttackTargetIfTargetOutOfReach`.

use super::{BrainContext, Trigger};
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// How fast to close on a target.
type SpeedModifier = Box<dyn Fn(&dyn PathfinderMob) -> f64 + Send>;

/// Walks at `ATTACK_TARGET` until it is inside attack range.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetWalkTargetFromAttackTargetIfTargetOutOfReach`.
pub struct SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    speed_modifier: SpeedModifier,
}

impl SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    /// Closes at a fixed speed.
    #[must_use]
    pub fn new(speed_modifier: f64) -> Self {
        Self::variable(move |_| speed_modifier)
    }

    /// Closes at a speed the mob picks.
    ///
    /// Vanilla parity: the `Function<LivingEntity, Float>` overload.
    #[must_use]
    pub fn variable(speed_modifier: impl Fn(&dyn PathfinderMob) -> f64 + Send + 'static) -> Self {
        Self {
            speed_modifier: Box::new(speed_modifier),
        }
    }
}

impl Trigger for SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(remembered) = brain.get_memory(memory_module_types::ATTACK_TARGET) else {
            return false;
        };
        let Some(target) = remembered.get() else {
            return false;
        };
        let Some(living_target) = target.as_living_entity() else {
            return false;
        };

        // Vanilla parity: `BehaviorUtils.isWithinAttackRange(body, target, 1)`.
        // Its projectile-weapon branch is not ported because Steel has no
        // `canUseNonMeleeWeapon`, and no brain mob Steel drives uses a bow.
        let within_range = brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
            .is_some_and(|visible| visible.contains_entity(remembered.id()))
            && ctx.mob().is_within_melee_attack_range(living_target);

        if within_range {
            brain.erase_memory(memory_module_types::WALK_TARGET.id());
        } else {
            brain.set_memory(
                memory_module_types::LOOK_TARGET,
                PositionTracker::of_entity(&target, true),
            );
            brain.set_memory(
                memory_module_types::WALK_TARGET,
                WalkTarget::of_entity(&target, (self.speed_modifier)(ctx.mob()), 0),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetWalkTargetFromAttackTargetIfTargetOutOfReach"
    }
}
