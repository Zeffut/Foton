//! Vanilla `Mount` and `DismountOrSkipMounting`.

use std::sync::Arc;

use super::{BrainContext, Trigger};
use crate::entity::SharedEntity;
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Vanilla parity: `Mount.CLOSE_ENOUGH_TO_START_RIDING_DIST`.
const CLOSE_ENOUGH_TO_START_RIDING_DIST: f64 = 1.0;

/// Walks to the remembered ride target and climbs on.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.Mount`.
pub struct Mount {
    speed_modifier: f64,
}

impl Mount {
    /// Vanilla parity: `Mount.create`.
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier }
    }
}

impl Trigger for Mount {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::RIDE_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }
        let Some(vehicle) = brain
            .get_memory(memory_module_types::RIDE_TARGET)
            .and_then(|memory| memory.get())
        else {
            return false;
        };
        if ctx.mob().is_passenger() {
            return false;
        }

        let distance_sqr = vehicle.position().distance_squared(ctx.mob().position());
        if distance_sqr < CLOSE_ENOUGH_TO_START_RIDING_DIST * CLOSE_ENOUGH_TO_START_RIDING_DIST {
            ctx.mob().start_riding(&vehicle);
        } else {
            brain.set_memory(
                memory_module_types::LOOK_TARGET,
                PositionTracker::of_entity(&vehicle, true),
            );
            brain.set_memory(
                memory_module_types::WALK_TARGET,
                WalkTarget::of_entity(&vehicle, self.speed_modifier, 1),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "Mount"
    }
}

/// Whether the body should refuse this vehicle.
type DontRideCondition = Box<dyn Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send>;

/// Gets off, or gives up on getting on, when the vehicle stops being suitable.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.DismountOrSkipMounting`.
pub struct DismountOrSkipMounting {
    max_walk_dist_to_ride_target: f64,
    dont_ride_if: DontRideCondition,
}

impl DismountOrSkipMounting {
    /// Vanilla parity: `DismountOrSkipMounting.create`.
    #[must_use]
    pub fn new(
        max_walk_dist_to_ride_target: i32,
        dont_ride_if: impl Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send + 'static,
    ) -> Self {
        Self {
            max_walk_dist_to_ride_target: f64::from(max_walk_dist_to_ride_target),
            dont_ride_if: Box::new(dont_ride_if),
        }
    }

    /// Vanilla parity: the private `DismountOrSkipMounting.isVehicleValid`.
    fn is_vehicle_valid(&self, ctx: &BrainContext<'_>, vehicle: &SharedEntity) -> bool {
        let same_level = vehicle
            .level()
            .is_some_and(|level| Arc::ptr_eq(&level, ctx.world()));
        vehicle.is_alive()
            && vehicle.position().distance_squared(ctx.mob().position())
                < self.max_walk_dist_to_ride_target * self.max_walk_dist_to_ride_target
            && same_level
    }
}

impl Trigger for DismountOrSkipMounting {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::RIDE_TARGET.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let current_vehicle = ctx.mob().vehicle();
        let target_vehicle = brain
            .get_memory(memory_module_types::RIDE_TARGET)
            .and_then(|memory| memory.get());
        let Some(vehicle) = current_vehicle.or(target_vehicle) else {
            return false;
        };

        if self.is_vehicle_valid(ctx, &vehicle) && !(self.dont_ride_if)(ctx, &vehicle) {
            return false;
        }

        ctx.mob().stop_riding();
        brain.erase_memory(memory_module_types::RIDE_TARGET.id());
        true
    }

    fn debug_name(&self) -> &'static str {
        "DismountOrSkipMounting"
    }
}
