//! Vanilla `BackUpIfTooClose`.

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::ai::control::rotate_if_necessary;

/// Strafes away from an attack target that has got too close.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.BackUpIfTooClose`.
pub struct BackUpIfTooClose {
    too_close_distance: f64,
    strafe_speed: f32,
}

impl BackUpIfTooClose {
    /// Backs off when the target is within `too_close_distance`.
    #[must_use]
    pub const fn new(too_close_distance: f64, strafe_speed: f32) -> Self {
        Self {
            too_close_distance,
            strafe_speed,
        }
    }
}

impl Trigger for BackUpIfTooClose {
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
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
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

        let mob = ctx.mob();
        let distance_sqr = target.position().distance_squared(mob.position());
        if distance_sqr >= self.too_close_distance * self.too_close_distance
            || !visible.contains_entity(remembered.id())
        {
            return false;
        }

        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );
        mob.mob_base()
            .controls()
            .lock()
            .move_control
            .strafe(-self.strafe_speed, 0.0);
        let (y_rot, x_rot) = mob.rotation();
        mob.set_rotation((rotate_if_necessary(y_rot, mob.y_head_rot(), 0.0), x_rot));
        true
    }

    fn debug_name(&self) -> &'static str {
        "BackUpIfTooClose"
    }
}
