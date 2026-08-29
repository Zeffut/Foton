//! Vanilla `EraseMemoryIf`.

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::MemoryModuleId;

/// When the memory should go.
type EraseCondition = Box<dyn Fn(&BrainContext<'_>) -> bool + Send>;

/// Erases a memory as soon as a predicate agrees.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.EraseMemoryIf`.
pub struct EraseMemoryIf {
    predicate: EraseCondition,
    memory: MemoryModuleId,
}

impl EraseMemoryIf {
    /// Vanilla parity: `EraseMemoryIf.create`.
    #[must_use]
    pub fn new(
        predicate: impl Fn(&BrainContext<'_>) -> bool + Send + 'static,
        memory: MemoryModuleId,
    ) -> Self {
        Self {
            predicate: Box::new(predicate),
            memory,
        }
    }
}

impl Trigger for EraseMemoryIf {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![self.memory]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if !ctx.brain().has_memory_value(self.memory) || !(self.predicate)(ctx) {
            return false;
        }
        ctx.brain().erase_memory(self.memory);
        true
    }

    fn debug_name(&self) -> &'static str {
        "EraseMemoryIf"
    }
}
