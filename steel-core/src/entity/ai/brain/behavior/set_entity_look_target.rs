//! Vanilla `SetEntityLookTarget` and `SetEntityLookTargetSometimes`.

use std::ptr;

use steel_registry::entity_type::EntityTypeRef;
use steel_utils::value_providers::UniformIntProvider;

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::{LivingEntity, SharedEntity};

/// Which nearby entities are worth looking at.
///
/// Vanilla's predicate takes only the candidate; the warden's also needs the brain, to ask
/// whether the candidate is the entity it is already attacking.
type LookFilter = Box<dyn Fn(&BrainContext<'_>, &dyn LivingEntity) -> bool + Send>;

/// Looks at the nearest visible entity matching a filter.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetEntityLookTarget`.
pub struct SetEntityLookTarget {
    filter: LookFilter,
    max_dist_sqr: f64,
}

impl SetEntityLookTarget {
    /// Looks at anything within `max_dist`.
    ///
    /// Vanilla parity: `SetEntityLookTarget.create(float)`.
    #[must_use]
    pub fn any_within(max_dist: f64) -> Self {
        Self::matching(|_| true, max_dist)
    }

    /// Looks at the nearest entity of one type.
    ///
    /// Vanilla parity: `SetEntityLookTarget.create(EntityType<?>, float)`.
    #[must_use]
    pub fn of_type(entity_type: EntityTypeRef, max_dist: f64) -> Self {
        Self::matching(
            move |candidate| ptr::eq(candidate.entity_type(), entity_type),
            max_dist,
        )
    }

    /// Looks at the nearest entity the filter accepts.
    ///
    /// Vanilla parity: `SetEntityLookTarget.create(Predicate<LivingEntity>, float)`.
    #[must_use]
    pub fn matching(
        filter: impl Fn(&dyn LivingEntity) -> bool + Send + 'static,
        max_dist: f64,
    ) -> Self {
        Self::matching_in_context(move |_, candidate| filter(candidate), max_dist)
    }

    /// Looks at the nearest entity the filter accepts, asking the brain as well.
    ///
    /// Vanilla parity: the same `SetEntityLookTarget.create(Predicate<LivingEntity>, float)`,
    /// reached from `WardenAi` with a predicate that closes over the body's brain.
    #[must_use]
    pub fn matching_in_context(
        filter: impl Fn(&BrainContext<'_>, &dyn LivingEntity) -> bool + Send + 'static,
        max_dist: f64,
    ) -> Self {
        Self {
            filter: Box::new(filter),
            max_dist_sqr: max_dist * max_dist,
        }
    }

    fn find(&self, ctx: &BrainContext<'_>) -> Option<SharedEntity> {
        let body_position = ctx.mob().position();
        let body_id = ctx.mob().id();
        ctx.brain()
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)?
            .find_closest(|candidate| {
                candidate.id() != body_id
                    && candidate.position().distance_squared(body_position) <= self.max_dist_sqr
                    && (self.filter)(ctx, candidate)
            })
    }
}

impl Trigger for SetEntityLookTarget {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx
            .brain()
            .has_memory_value(memory_module_types::LOOK_TARGET.id())
        {
            return false;
        }
        let Some(target) = self.find(ctx) else {
            return false;
        };
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetEntityLookTarget"
    }
}

/// Looks at a nearby entity every so often rather than every tick.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetEntityLookTargetSometimes`.
/// Vanilla marks the class `@Deprecated` without replacing it, and the copper
/// golem still uses it, so it is ported as-is.
pub struct SetEntityLookTargetSometimes {
    inner: SetEntityLookTarget,
    interval: UniformIntProvider,
    ticks_until_next_start: i32,
}

impl SetEntityLookTargetSometimes {
    /// Looks at the nearest entity of one type, on `interval`.
    ///
    /// Vanilla parity: `SetEntityLookTargetSometimes.create(EntityType<?>, float, UniformInt)`.
    ///
    /// # Panics
    ///
    /// Panics when `interval` could fire on consecutive ticks, matching the
    /// `IllegalArgumentException` of vanilla's `Ticker` constructor.
    #[must_use]
    pub fn of_type(
        entity_type: EntityTypeRef,
        max_dist: f64,
        interval: UniformIntProvider,
    ) -> Self {
        Self::around(
            SetEntityLookTarget::of_type(entity_type, max_dist),
            interval,
        )
    }

    /// Looks at any nearby entity, on `interval`.
    ///
    /// Vanilla parity: `SetEntityLookTargetSometimes.create(float, UniformInt)`.
    ///
    /// # Panics
    ///
    /// Panics when `interval` could fire on consecutive ticks, matching the
    /// `IllegalArgumentException` of vanilla's `Ticker` constructor.
    #[must_use]
    pub fn any_within(max_dist: f64, interval: UniformIntProvider) -> Self {
        Self::around(SetEntityLookTarget::any_within(max_dist), interval)
    }

    fn around(inner: SetEntityLookTarget, interval: UniformIntProvider) -> Self {
        assert!(
            interval.min_inclusive > 1,
            "a look interval of {} would retrigger every tick",
            interval.min_inclusive
        );
        Self {
            inner,
            interval,
            ticks_until_next_start: 0,
        }
    }

    /// Vanilla parity: `SetEntityLookTargetSometimes.Ticker.tickDownAndCheck`.
    fn tick_down_and_check(&mut self) -> bool {
        if self.ticks_until_next_start == 0 {
            self.ticks_until_next_start =
                rand::random_range(self.interval.min_inclusive..=self.interval.max_inclusive) - 1;
            return false;
        }
        self.ticks_until_next_start -= 1;
        self.ticks_until_next_start == 0
    }
}

impl Trigger for SetEntityLookTargetSometimes {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        self.inner.required_memories()
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx
            .brain()
            .has_memory_value(memory_module_types::LOOK_TARGET.id())
        {
            return false;
        }
        // Vanilla parity: the ticker is only advanced once a target exists, so
        // the interval counts sightings rather than ticks.
        let Some(target) = self.inner.find(ctx) else {
            return false;
        };
        if !self.tick_down_and_check() {
            return false;
        }
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetEntityLookTargetSometimes"
    }
}
