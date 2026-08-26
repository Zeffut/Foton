//! Vanilla `BecomePassiveIfMemoryPresent`.

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};

/// Drops the attack target and goes quiet while a pacifying memory is set.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.BecomePassiveIfMemoryPresent`.
pub struct BecomePassiveIfMemoryPresent {
    pacifying_memory: MemoryModuleId,
    pacify_duration: i64,
}

impl BecomePassiveIfMemoryPresent {
    /// Vanilla parity: `BecomePassiveIfMemoryPresent.create`.
    #[must_use]
    pub const fn new(pacifying_memory: MemoryModuleId, pacify_duration: i64) -> Self {
        Self {
            pacifying_memory,
            pacify_duration,
        }
    }
}

impl Trigger for BecomePassiveIfMemoryPresent {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::PACIFIED.id(),
            self.pacifying_memory,
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::PACIFIED.id())
            || !brain.has_memory_value(self.pacifying_memory)
        {
            return false;
        }
        brain.set_memory_with_expiry(memory_module_types::PACIFIED, true, self.pacify_duration);
        brain.erase_memory(memory_module_types::ATTACK_TARGET.id());
        true
    }

    fn debug_name(&self) -> &'static str {
        "BecomePassiveIfMemoryPresent"
    }
}
