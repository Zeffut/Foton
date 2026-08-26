//! Vanilla `BabyFollowAdult`.

use steel_utils::value_providers::UniformIntProvider;

use super::{BrainContext, Trigger, utils};
use crate::entity::PathfinderMob;
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
    speed_modifier: SpeedModifier,
    nearest_visible_adult: MemoryModuleType<EntityMemory>,
}

/// How fast the baby closes the gap, which the body may decide per tick.
type SpeedModifier = Box<dyn Fn(&dyn PathfinderMob) -> f64 + Send>;

impl BabyFollowAdult {
    /// Vanilla parity: `BabyFollowAdult.create(UniformInt, float)`.
    #[must_use]
    pub fn new(follow_range: UniformIntProvider, speed_modifier: f64) -> Self {
        Self::variable(follow_range, move |_| speed_modifier)
    }

    /// Follows at a speed the baby picks.
    ///
    /// Vanilla parity: the `Function<LivingEntity, Float> speedModifier`
    /// overload -- an axolotl calf paddles after its parent faster in water
    /// than it waddles after it on land.
    #[must_use]
    pub fn variable(
        follow_range: UniformIntProvider,
        speed_modifier: impl Fn(&dyn PathfinderMob) -> f64 + Send + 'static,
    ) -> Self {
        Self {
            follow_range,
            speed_modifier: Box::new(speed_modifier),
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
                (self.speed_modifier)(ctx.mob()),
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
