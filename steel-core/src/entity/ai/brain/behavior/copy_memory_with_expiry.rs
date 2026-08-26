//! Vanilla `CopyMemoryWithExpiry`.

use steel_utils::value_providers::UniformIntProvider;

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryModuleType, MemoryValueType};

/// Whether the copy should happen this tick.
type CopyCondition = Box<dyn Fn(&BrainContext<'_>) -> bool + Send>;

/// Copies one memory into another for a while.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.CopyMemoryWithExpiry`.
pub struct CopyMemoryWithExpiry<T: MemoryValueType> {
    copy_if_true: CopyCondition,
    source_memory: MemoryModuleType<T>,
    target_memory: MemoryModuleType<T>,
    duration_of_copy: UniformIntProvider,
}

impl<T: MemoryValueType> CopyMemoryWithExpiry<T> {
    /// Vanilla parity: `CopyMemoryWithExpiry.create`.
    #[must_use]
    pub fn new(
        copy_if_true: impl Fn(&BrainContext<'_>) -> bool + Send + 'static,
        source_memory: MemoryModuleType<T>,
        target_memory: MemoryModuleType<T>,
        duration_of_copy: UniformIntProvider,
    ) -> Self {
        Self {
            copy_if_true: Box::new(copy_if_true),
            source_memory,
            target_memory,
            duration_of_copy,
        }
    }
}

impl<T: MemoryValueType> Trigger for CopyMemoryWithExpiry<T> {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![self.source_memory.id(), self.target_memory.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(self.target_memory.id()) {
            return false;
        }
        let Some(value) = brain.get_memory(self.source_memory) else {
            return false;
        };
        if !(self.copy_if_true)(ctx) {
            return false;
        }
        let duration = rand::random_range(
            self.duration_of_copy.min_inclusive..=self.duration_of_copy.max_inclusive,
        );
        brain.set_memory_with_expiry(self.target_memory, value, i64::from(duration));
        true
    }

    fn debug_name(&self) -> &'static str {
        "CopyMemoryWithExpiry"
    }
}
