//! Vanilla `GateBehavior`, `RunOne` and the `ShufflingList` behind them.

use super::{BehaviorControl, BehaviorStatus, BrainContext};
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryStatus};

/// A weighted list that reshuffles itself without losing its weights.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.ShufflingList`.
pub struct ShufflingList<T> {
    entries: Vec<WeightedEntry<T>>,
}

struct WeightedEntry<T> {
    data: T,
    weight: i32,
    rand_weight: f64,
}

impl<T> Default for ShufflingList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ShufflingList<T> {
    /// Creates an empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds `data` with the given weight.
    pub fn add(&mut self, data: T, weight: i32) {
        self.entries.push(WeightedEntry {
            data,
            weight,
            rand_weight: 0.0,
        });
    }

    /// Reorders the list, favoring heavier entries.
    ///
    /// Vanilla parity: `ShufflingList.shuffle`, which sorts on
    /// `-pow(random, 1 / weight)` -- the standard weighted-without-replacement
    /// trick, so the exact expression matters.
    pub fn shuffle(&mut self) {
        for entry in &mut self.entries {
            entry.rand_weight =
                -f64::from(rand::random::<f32>()).powf(1.0 / f64::from(entry.weight));
        }
        self.entries
            .sort_by(|left, right| left.rand_weight.total_cmp(&right.rand_weight));
    }

    /// Walks the entries in their current order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|entry| &entry.data)
    }

    /// Walks the entries mutably in their current order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.entries.iter_mut().map(|entry| &mut entry.data)
    }
}

/// Whether a gate reshuffles its children before running them.
///
/// Vanilla parity: `GateBehavior.OrderPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderPolicy {
    Ordered,
    Shuffled,
}

/// How many children a gate starts.
///
/// Vanilla parity: `GateBehavior.RunningPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningPolicy {
    /// Starts children until one of them accepts.
    RunOne,
    /// Starts every child that accepts.
    TryAll,
}

/// Runs a group of behaviors behind one priority slot.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.GateBehavior`.
pub struct GateBehavior {
    entry_condition: Vec<(MemoryModuleId, MemoryStatus)>,
    exit_erased_memories: Vec<MemoryModuleId>,
    order_policy: OrderPolicy,
    running_policy: RunningPolicy,
    behaviors: ShufflingList<Box<dyn BehaviorControl>>,
    status: BehaviorStatus,
}

impl GateBehavior {
    /// Builds a gate from weighted children.
    #[must_use]
    pub fn new(
        entry_condition: Vec<(MemoryModuleId, MemoryStatus)>,
        exit_erased_memories: Vec<MemoryModuleId>,
        order_policy: OrderPolicy,
        running_policy: RunningPolicy,
        behaviors: Vec<(Box<dyn BehaviorControl>, i32)>,
    ) -> Self {
        let mut list = ShufflingList::new();
        for (behavior, weight) in behaviors {
            list.add(behavior, weight);
        }
        Self {
            entry_condition,
            exit_erased_memories,
            order_policy,
            running_policy,
            behaviors: list,
            status: BehaviorStatus::Stopped,
        }
    }

    fn has_required_memories(&self, ctx: &BrainContext<'_>) -> bool {
        self.entry_condition
            .iter()
            .all(|&(memory, status)| ctx.brain().check_memory(memory, status))
    }
}

impl BehaviorControl for GateBehavior {
    fn status(&self) -> BehaviorStatus {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        let mut memories: Vec<MemoryModuleId> = self
            .entry_condition
            .iter()
            .map(|&(memory, _)| memory)
            .collect();
        for behavior in self.behaviors.iter() {
            memories.extend(behavior.required_memories());
        }
        memories
    }

    fn try_start(&mut self, ctx: &BrainContext<'_>) -> bool {
        if !self.has_required_memories(ctx) {
            return false;
        }
        self.status = BehaviorStatus::Running;
        if self.order_policy == OrderPolicy::Shuffled {
            self.behaviors.shuffle();
        }

        for behavior in self.behaviors.iter_mut() {
            if behavior.status() != BehaviorStatus::Stopped {
                continue;
            }
            let started = behavior.try_start(ctx);
            if started && self.running_policy == RunningPolicy::RunOne {
                break;
            }
        }
        true
    }

    fn tick_or_stop(&mut self, ctx: &BrainContext<'_>) {
        for behavior in self.behaviors.iter_mut() {
            if behavior.status() == BehaviorStatus::Running {
                behavior.tick_or_stop(ctx);
            }
        }
        if self
            .behaviors
            .iter()
            .all(|behavior| behavior.status() != BehaviorStatus::Running)
        {
            self.do_stop(ctx);
        }
    }

    fn do_stop(&mut self, ctx: &BrainContext<'_>) {
        self.status = BehaviorStatus::Stopped;
        for behavior in self.behaviors.iter_mut() {
            if behavior.status() == BehaviorStatus::Running {
                behavior.do_stop(ctx);
            }
        }
        for memory in &self.exit_erased_memories {
            ctx.brain().erase_memory(*memory);
        }
    }

    fn debug_name(&self) -> &'static str {
        "GateBehavior"
    }
}

/// Picks one of several behaviors at random, by weight.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.RunOne`.
pub struct RunOne;

impl RunOne {
    /// Runs one of `weighted_behaviors` whenever the gate is reached.
    ///
    /// Vanilla parity: `new RunOne<>(List<Pair<BehaviorControl, Integer>>)`. It is
    /// not called `new` because it hands back the [`GateBehavior`] vanilla makes
    /// `RunOne` a subclass of.
    #[must_use]
    pub fn unconditional(weighted_behaviors: Vec<(Box<dyn BehaviorControl>, i32)>) -> GateBehavior {
        Self::gated(Vec::new(), weighted_behaviors)
    }

    /// Runs one of `weighted_behaviors`, but only while `entry_condition` holds.
    ///
    /// Vanilla parity: `new RunOne<>(Map<MemoryModuleType, MemoryStatus>, List<...>)`.
    #[must_use]
    pub fn gated(
        entry_condition: Vec<(MemoryModuleId, MemoryStatus)>,
        weighted_behaviors: Vec<(Box<dyn BehaviorControl>, i32)>,
    ) -> GateBehavior {
        GateBehavior::new(
            entry_condition,
            Vec::new(),
            OrderPolicy::Shuffled,
            RunningPolicy::RunOne,
            weighted_behaviors,
        )
    }
}
