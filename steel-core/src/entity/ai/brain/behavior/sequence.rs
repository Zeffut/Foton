//! Vanilla `BehaviorBuilder.sequence`.

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::MemoryModuleId;

/// Runs a second trigger only when the first one fires.
///
/// Vanilla parity: `BehaviorBuilder.sequence(first, second)`. Upstream this is
/// `i.group(i.ifTriggered(first)).apply(i, unit -> second::trigger)`, and
/// `ifTriggered` failing takes the whole group -- and so the composite -- down
/// with it. The declarative wrapper is what makes that readable in Java; in
/// Rust it is one `&&`.
pub struct Sequence {
    first: Box<dyn Trigger>,
    second: Box<dyn Trigger>,
}

impl Sequence {
    /// Runs `second` only on the ticks `first` succeeds.
    #[must_use]
    pub fn new(first: Box<dyn Trigger>, second: Box<dyn Trigger>) -> Self {
        Self { first, second }
    }
}

impl Trigger for Sequence {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        // Vanilla parity: the anonymous `OneShot` returned by `sequence`, whose
        // `getRequiredMemories` is the union of both halves'.
        let mut memories = self.first.required_memories();
        memories.extend(self.second.required_memories());
        memories
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.first.trigger(ctx) && self.second.trigger(ctx)
    }

    fn debug_name(&self) -> &'static str {
        "Sequence"
    }
}
