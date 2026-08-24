//! Vanilla `TriggerGate`.

use super::gate_behavior::{OrderPolicy, RunningPolicy, ShufflingList};
use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::MemoryModuleId;

/// Runs one of several triggers, by weight, in a single tick.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.TriggerGate`. Unlike
/// [`super::GateBehavior`] this never stays running: it is a trigger itself, so
/// it always reports success and is scheduled through [`super::OneShot`].
pub struct TriggerGate {
    triggers: ShufflingList<Box<dyn Trigger>>,
    order_policy: OrderPolicy,
    running_policy: RunningPolicy,
}

impl TriggerGate {
    /// Runs one weighted trigger, reshuffling first.
    ///
    /// Vanilla parity: `TriggerGate.triggerOneShuffled`.
    #[must_use]
    pub fn trigger_one_shuffled(weighted_triggers: Vec<(Box<dyn Trigger>, i32)>) -> Self {
        Self::new(
            weighted_triggers,
            OrderPolicy::Shuffled,
            RunningPolicy::RunOne,
        )
    }

    /// Vanilla parity: `TriggerGate.triggerGate`.
    #[must_use]
    pub fn new(
        weighted_triggers: Vec<(Box<dyn Trigger>, i32)>,
        order_policy: OrderPolicy,
        running_policy: RunningPolicy,
    ) -> Self {
        let mut triggers = ShufflingList::new();
        for (trigger, weight) in weighted_triggers {
            triggers.add(trigger, weight);
        }
        Self {
            triggers,
            order_policy,
            running_policy,
        }
    }
}

impl Trigger for TriggerGate {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        // Vanilla's builder reports no memories here, because the gate itself
        // declares none and its children are only reached through `trigger`.
        // Steel reports the children's, so the brain registers every memory
        // they read; a gate whose child memory was never registered would
        // silently never fire.
        self.triggers
            .iter()
            .flat_map(|trigger| trigger.required_memories())
            .collect()
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if self.order_policy == OrderPolicy::Shuffled {
            self.triggers.shuffle();
        }
        for trigger in self.triggers.iter_mut() {
            if trigger.trigger(ctx) && self.running_policy == RunningPolicy::RunOne {
                break;
            }
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "TriggerGate"
    }
}
