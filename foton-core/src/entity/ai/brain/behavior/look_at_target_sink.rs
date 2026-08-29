//! Vanilla `LookAtTargetSink`.

use super::{BrainContext, TimedBehavior};

use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryStatus, memory_module_types};
use crate::entity::ai::control::DEFAULT_LOOK_Y_MAX_ROT_SPEED;

/// Turns the head toward whatever is in `LOOK_TARGET`.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.LookAtTargetSink`.
pub struct LookAtTargetSink {
    entry_condition: [(MemoryModuleId, MemoryStatus); 1],
    min_duration: i32,
    max_duration: i32,
}

impl LookAtTargetSink {
    /// Holds a gaze for between `min_duration` and `max_duration` ticks.
    #[must_use]
    pub const fn new(min_duration: i32, max_duration: i32) -> Self {
        Self {
            entry_condition: [(
                memory_module_types::LOOK_TARGET.id(),
                MemoryStatus::ValuePresent,
            )],
            min_duration,
            max_duration,
        }
    }
}

impl TimedBehavior for LookAtTargetSink {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (self.min_duration, self.max_duration)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .get_memory(memory_module_types::LOOK_TARGET)
            .is_some_and(|target| target.is_visible_by(ctx.mob()))
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .erase_memory(memory_module_types::LOOK_TARGET.id());
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(position) = ctx
            .brain()
            .get_memory(memory_module_types::LOOK_TARGET)
            .and_then(|target| target.current_position())
        else {
            return;
        };
        let mob = ctx.mob();
        // Vanilla parity: `LookControl.setLookAt(Vec3)`, which fills in
        // `Mob.getHeadRotSpeed()` and `Mob.getMaxHeadXRot()`.
        mob.mob_base().controls().lock().look_control.set_look_at(
            position,
            DEFAULT_LOOK_Y_MAX_ROT_SPEED,
            mob.max_head_x_rot(),
        );
    }

    fn debug_name(&self) -> &'static str {
        "LookAtTargetSink"
    }
}
