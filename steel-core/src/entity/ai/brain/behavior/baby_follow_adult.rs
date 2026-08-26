//! Vanilla `BabyFollowAdult`.

use steel_utils::value_providers::UniformIntProvider;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::EntityMemory;
use crate::entity::ai::brain::memory::{
    MemoryModuleId, MemoryModuleType, WalkTarget, memory_module_types,
};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Keeps a baby trailing the nearest adult of its kind.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.BabyFollowAdult`.
pub struct BabyFollowAdult {
    follow_range: UniformIntProvider,
    speed_modifier: f64,
    nearest_visible_adult: MemoryModuleType<EntityMemory>,
}

impl BabyFollowAdult {
    /// Vanilla parity: `BabyFollowAdult.create(UniformInt, float)`.
    #[must_use]
    pub const fn new(follow_range: UniformIntProvider, speed_modifier: f64) -> Self {
        Self {
            follow_range,
            speed_modifier,
            nearest_visible_adult: memory_module_types::NEAREST_VISIBLE_ADULT,
        }
    }

    /// Follows the adult a different memory points at.
    ///
    /// Vanilla parity: the `nearestVisibleType` argument of the four-argument
    /// `BabyFollowAdult.create`.
    #[must_use]
    pub const fn following(mut self, memory: MemoryModuleType<EntityMemory>) -> Self {
        self.nearest_visible_adult = memory;
        self
    }
}

impl Trigger for BabyFollowAdult {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            self.nearest_visible_adult.id(),
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }
        if !ctx.mob().is_baby() {
            return false;
        }
        let Some(adult) = brain
            .get_memory(self.nearest_visible_adult)
            .and_then(|memory| memory.get())
        else {
            return false;
        };

        let distance_sqr = adult.position().distance_squared(ctx.mob().position());
        let outer = f64::from(self.follow_range.max_inclusive + 1);
        let inner = f64::from(self.follow_range.min_inclusive);
        if distance_sqr >= outer * outer || distance_sqr < inner * inner {
            return false;
        }

        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&adult, true),
        );
        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_entity(
                &adult,
                self.speed_modifier,
                self.follow_range.min_inclusive - 1,
            ),
        );
        // Touch the helper module so a future eye-height variant keeps a home.
        let _ = utils::in_same_world;
        true
    }

    fn debug_name(&self) -> &'static str {
        "BabyFollowAdult"
    }
}
