//! The two behaviors that carry a mob's inventory to somebody else.
//!
//! Vanilla parity: `GoAndGiveItemsToTarget` and `StayCloseToTarget`. They are a
//! pair: the first walks up to the deposit point and throws one item at it, and
//! the second is what keeps the mob near that point while its hands are empty.

use glam::DVec3;
use steel_registry::item_stack::ItemStack;
use steel_utils::BlockPos;

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, Trigger, utils};
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::inventory::container::Container as _;

/// Vanilla parity: `GoAndGiveItemsToTarget.CLOSE_ENOUGH_DISTANCE_TO_TARGET`.
const CLOSE_ENOUGH_DISTANCE_TO_TARGET: f64 = 3.0;
/// Vanilla parity: `GoAndGiveItemsToTarget.ITEM_PICKUP_COOLDOWN_AFTER_THROWING`.
const ITEM_PICKUP_COOLDOWN_AFTER_THROWING: i32 = 60;
/// Vanilla parity: the `new Vec3(0.2F, 0.3F, 0.2F)` throw velocity.
const THROW_VELOCITY: DVec3 = DVec3::new(
    0.200_000_002_980_232_24,
    0.300_000_011_920_928_96,
    0.200_000_002_980_232_24,
);
/// Vanilla parity: the `0.2F` hand-below-eye distance of the throw.
const THROW_HAND_Y_DISTANCE_FROM_EYE: f64 = 0.200_000_002_980_232_24;
/// Vanilla parity: the `3` close-enough distance the walk target is set with.
const WALK_CLOSE_ENOUGH: i32 = 3;
/// Vanilla parity: the `add(0.0, 1.0, 0.0)` the throw aims above the target.
const THROW_AIM_ABOVE: f64 = 1.0;

/// Where the mob should take its inventory.
type TargetPositionGetter = Box<dyn Fn(&dyn PathfinderMob) -> Option<PositionTracker> + Send>;
/// What to do each time an item is actually thrown.
type ItemThrower = Box<dyn Fn(&BrainContext<'_>, &ItemStack, BlockPos) + Send>;
/// Whether the mob should bother staying close.
type ShouldRun = Box<dyn Fn(&dyn PathfinderMob) -> bool + Send>;

/// Walks the mob to a deposit point and throws its inventory at it, one item at
/// a time.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.GoAndGiveItemsToTarget`.
pub struct GoAndGiveItemsToTarget {
    target_position_getter: TargetPositionGetter,
    speed_modifier: f64,
    timeout_duration: i32,
    item_thrower: ItemThrower,
}

impl GoAndGiveItemsToTarget {
    /// Vanilla parity: the `GoAndGiveItemsToTarget(getter, speed, timeout, thrower)`
    /// constructor.
    #[must_use]
    pub fn new(
        target_position_getter: impl Fn(&dyn PathfinderMob) -> Option<PositionTracker> + Send + 'static,
        speed_modifier: f64,
        timeout_duration: i32,
        item_thrower: impl Fn(&BrainContext<'_>, &ItemStack, BlockPos) + Send + 'static,
    ) -> Self {
        Self {
            target_position_getter: Box::new(target_position_getter),
            speed_modifier,
            timeout_duration,
            item_thrower: Box::new(item_thrower),
        }
    }

    /// Vanilla parity: `GoAndGiveItemsToTarget.canThrowItemToTarget`.
    fn can_throw_item_to_target(&self, ctx: &BrainContext<'_>) -> bool {
        let Some(carrier) = ctx.mob().as_inventory_carrier() else {
            return false;
        };
        if carrier.carried_inventory().lock().is_empty() {
            return false;
        }
        (self.target_position_getter)(ctx.mob()).is_some()
    }
}

impl TimedBehavior for GoAndGiveItemsToTarget {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        const CONDITIONS: &[(MemoryModuleId, MemoryStatus)] = &[
            (
                memory_module_types::LOOK_TARGET.id(),
                MemoryStatus::Registered,
            ),
            (
                memory_module_types::WALK_TARGET.id(),
                MemoryStatus::Registered,
            ),
            (
                memory_module_types::ITEM_PICKUP_COOLDOWN_TICKS.id(),
                MemoryStatus::Registered,
            ),
        ];
        CONDITIONS
    }

    fn duration(&self) -> (i32, i32) {
        (self.timeout_duration, self.timeout_duration)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.can_throw_item_to_target(ctx)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.can_throw_item_to_target(ctx)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = (self.target_position_getter)(ctx.mob()) else {
            return;
        };
        utils::set_walk_and_look_target_memories(
            ctx.brain(),
            target,
            self.speed_modifier,
            WALK_CLOSE_ENOUGH,
        );
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = (self.target_position_getter)(ctx.mob()) else {
            return;
        };
        let Some(deposit_position) = target.current_position() else {
            return;
        };
        let body = ctx.mob();
        let eye_position = DVec3::new(body.position().x, body.get_eye_y(), body.position().z);
        if deposit_position.distance(eye_position) >= CLOSE_ENOUGH_DISTANCE_TO_TARGET {
            return;
        }
        let Some(carrier) = body.as_inventory_carrier() else {
            return;
        };

        let item = carrier.carried_inventory().lock().remove_item(0, 1);
        if item.is_empty() {
            return;
        }

        utils::throw_item_with_velocity(
            body,
            item.copy_with_count(item.count()),
            deposit_position + DVec3::new(0.0, THROW_AIM_ABOVE, 0.0),
            THROW_VELOCITY,
            THROW_HAND_Y_DISTANCE_FROM_EYE,
        );
        if let Some(target_block) = target.current_block_position() {
            (self.item_thrower)(ctx, &item, target_block);
        }
        ctx.brain().set_memory(
            memory_module_types::ITEM_PICKUP_COOLDOWN_TICKS,
            ITEM_PICKUP_COOLDOWN_AFTER_THROWING,
        );
    }

    fn debug_name(&self) -> &'static str {
        "GoAndGiveItemsToTarget"
    }
}

/// Keeps the mob within `too_far` of a point it should be hanging around.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.StayCloseToTarget`.
pub struct StayCloseToTarget {
    target_position_getter: TargetPositionGetter,
    should_run: ShouldRun,
    close_enough: i32,
    too_far: i32,
    speed_modifier: f64,
}

impl StayCloseToTarget {
    /// Vanilla parity: `StayCloseToTarget.create`.
    #[must_use]
    pub fn new(
        target_position_getter: impl Fn(&dyn PathfinderMob) -> Option<PositionTracker> + Send + 'static,
        should_run: impl Fn(&dyn PathfinderMob) -> bool + Send + 'static,
        close_enough: i32,
        too_far: i32,
        speed_modifier: f64,
    ) -> Self {
        Self {
            target_position_getter: Box::new(target_position_getter),
            should_run: Box::new(should_run),
            close_enough,
            too_far,
            speed_modifier,
        }
    }
}

impl Trigger for StayCloseToTarget {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let body = ctx.mob();
        let Some(target) = (self.target_position_getter)(body) else {
            return false;
        };
        if !(self.should_run)(body) {
            return false;
        }
        let Some(target_position) = target.current_position() else {
            return false;
        };
        // Vanilla parity: `closerThan(..., tooFar)` returns early -- a mob that
        // is already close enough has nothing to do.
        if body.position().distance(target_position) < f64::from(self.too_far) {
            return false;
        }

        ctx.brain()
            .set_memory(memory_module_types::LOOK_TARGET, target.clone());
        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::new(target, self.speed_modifier, self.close_enough),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "StayCloseToTarget"
    }
}
