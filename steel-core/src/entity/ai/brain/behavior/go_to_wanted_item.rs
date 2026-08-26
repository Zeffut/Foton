//! Vanilla `GoToWantedItem`.

use super::{BrainContext, Trigger};
use crate::entity::Mob;
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Whether the body is allowed to chase an item right now.
type WalkCondition = Box<dyn Fn(&BrainContext<'_>) -> bool + Send>;

/// Walks toward the nearest item the body wants.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.GoToWantedItem`.
pub struct GoToWantedItem {
    predicate: WalkCondition,
    speed_modifier: f64,
    interrupt_ongoing_walk: bool,
    max_dist_to_walk: f64,
}

impl GoToWantedItem {
    /// Vanilla parity: `GoToWantedItem.create(float, boolean, int)`.
    #[must_use]
    pub fn new(speed_modifier: f64, interrupt_ongoing_walk: bool, max_dist_to_walk: i32) -> Self {
        Self::conditional(
            |_| true,
            speed_modifier,
            interrupt_ongoing_walk,
            max_dist_to_walk,
        )
    }

    /// Vanilla parity: `GoToWantedItem.create(Predicate<E>, float, boolean, int)`.
    #[must_use]
    pub fn conditional(
        predicate: impl Fn(&BrainContext<'_>) -> bool + Send + 'static,
        speed_modifier: f64,
        interrupt_ongoing_walk: bool,
        max_dist_to_walk: i32,
    ) -> Self {
        Self {
            predicate: Box::new(predicate),
            speed_modifier,
            interrupt_ongoing_walk,
            max_dist_to_walk: f64::from(max_dist_to_walk),
        }
    }
}

impl Trigger for GoToWantedItem {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::NEAREST_VISIBLE_WANTED_ITEM.id(),
            memory_module_types::ITEM_PICKUP_COOLDOWN_TICKS.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        // Vanilla asks for `WALK_TARGET` absent when it may not interrupt an
        // ongoing walk, and merely registered when it may.
        if !self.interrupt_ongoing_walk
            && brain.has_memory_value(memory_module_types::WALK_TARGET.id())
        {
            return false;
        }
        if brain.has_memory_value(memory_module_types::ITEM_PICKUP_COOLDOWN_TICKS.id()) {
            return false;
        }
        let Some(item) = brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_WANTED_ITEM)
            .and_then(|memory| memory.get())
        else {
            return false;
        };
        if !(self.predicate)(ctx)
            || item.position().distance_squared(ctx.mob().position())
                > self.max_dist_to_walk * self.max_dist_to_walk
            || !ctx
                .world()
                .is_block_within_world_border(item.block_position())
            || !Mob::can_pick_up_loot(ctx.mob())
        {
            return false;
        }

        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&item, true),
        );
        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_entity(&item, self.speed_modifier, 0),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "GoToWantedItem"
    }
}
