//! Vanilla `CountDownCooldownTicks`.

use super::{BrainContext, TimedBehavior};
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryModuleType, MemoryStatus};

/// Counts one cooldown memory down to zero, then erases it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.CountDownCooldownTicks`.
pub struct CountDownCooldownTicks {
    cooldown_ticks: MemoryModuleType<i32>,
    entry_condition: [(MemoryModuleId, MemoryStatus); 1],
}

impl CountDownCooldownTicks {
    /// Counts down `cooldown_ticks`.
    #[must_use]
    pub const fn new(cooldown_ticks: MemoryModuleType<i32>) -> Self {
        Self {
            cooldown_ticks,
            entry_condition: [(cooldown_ticks.id(), MemoryStatus::ValuePresent)],
        }
    }
}

impl TimedBehavior for CountDownCooldownTicks {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn times_out(&self) -> bool {
        false
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .get_memory(self.cooldown_ticks)
            .is_some_and(|remaining| remaining > 0)
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(remaining) = ctx.brain().get_memory(self.cooldown_ticks) else {
            return;
        };
        ctx.brain().set_memory(self.cooldown_ticks, remaining - 1);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain().erase_memory(self.cooldown_ticks.id());
    }

    fn debug_name(&self) -> &'static str {
        "CountDownCooldownTicks"
    }
}
