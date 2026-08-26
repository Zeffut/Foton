//! Vanilla `AnimalPanic`.

use std::f64::consts::FRAC_PI_2;

use glam::DVec3;
use steel_registry::vanilla_damage_type_tags;
use steel_utils::Identifier;

use super::{BrainContext, TimedBehavior};
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{
    MemoryModuleId, MemoryStatus, WalkTarget, memory_module_types,
};
use crate::entity::ai::goal::{
    air_and_water_random_pos, block_pos_corner, land_random_pos, look_for_water,
};

/// Vanilla parity: `AnimalPanic.PANIC_MIN_DURATION`.
const PANIC_MIN_DURATION: i32 = 100;
/// Vanilla parity: `AnimalPanic.PANIC_MAX_DURATION`.
const PANIC_MAX_DURATION: i32 = 120;
/// Vanilla parity: `AnimalPanic.PANIC_DISTANCE_HORIZONTAL`.
const PANIC_DISTANCE_HORIZONTAL: i32 = 5;
/// Vanilla parity: `AnimalPanic.PANIC_DISTANCE_VERTICAL`.
const PANIC_DISTANCE_VERTICAL: i32 = 4;
/// Vanilla parity: the `(float) (Math.PI / 2)` a flying panic searches within.
const PANIC_MAX_RADIANS_FROM_VIEW: f64 = FRAC_PI_2;

/// Where a panicking mob runs to.
type PanicPositionPicker = Box<dyn Fn(&dyn PathfinderMob) -> Option<DVec3> + Send>;

/// Runs away after taking damage, and remembers that it is running.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.AnimalPanic`. The
/// `IS_PANICKING` memory this sets is what
/// [`crate::entity::PathfinderMob::is_panicking`] reads for a brain mob, in
/// place of the running-panic-goal check a goal mob uses.
pub struct AnimalPanic {
    entry_condition: [(MemoryModuleId, MemoryStatus); 2],
    speed_multiplier: f64,
    panic_causing_damage_types: Identifier,
    position_getter: PanicPositionPicker,
}

impl AnimalPanic {
    /// Panics at anything in `minecraft:panic_causes`.
    ///
    /// Vanilla parity: `new AnimalPanic<>(float)`.
    #[must_use]
    pub fn new(speed_multiplier: f64) -> Self {
        Self::with_damage_types(
            speed_multiplier,
            vanilla_damage_type_tags::DamageTypeTag::PANIC_CAUSES,
        )
    }

    /// Panics into the air rather than across the ground.
    ///
    /// Vanilla parity: `new AnimalPanic<>(float, int)`, whose flight height a
    /// flier picks its escape at. A happy ghast passes zero, which is level
    /// with itself -- it bolts sideways, not upward.
    #[must_use]
    pub fn flying(speed_multiplier: f64, flying_height: i32) -> Self {
        let mut panic = Self::new(speed_multiplier);
        panic.position_getter = Box::new(move |mob| {
            let view = mob.look_angle();
            air_and_water_random_pos(
                mob,
                PANIC_DISTANCE_HORIZONTAL,
                PANIC_DISTANCE_VERTICAL,
                flying_height,
                view.x,
                view.z,
                PANIC_MAX_RADIANS_FROM_VIEW,
            )
        });
        panic
    }

    /// Panics only at damage in `panic_causing_damage_types`.
    ///
    /// Vanilla parity: `new AnimalPanic<>(float, Function<PathfinderMob, TagKey<DamageType>>)`.
    #[must_use]
    pub fn with_damage_types(
        speed_multiplier: f64,
        panic_causing_damage_types: Identifier,
    ) -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::IS_PANICKING.id(),
                    MemoryStatus::Registered,
                ),
                (memory_module_types::HURT_BY.id(), MemoryStatus::Registered),
            ],
            speed_multiplier,
            panic_causing_damage_types,
            position_getter: Box::new(|mob| {
                land_random_pos(mob, PANIC_DISTANCE_HORIZONTAL, PANIC_DISTANCE_VERTICAL)
            }),
        }
    }

    /// Vanilla parity: `AnimalPanic.getPanicPos`.
    fn panic_pos(&self, ctx: &BrainContext<'_>) -> Option<DVec3> {
        if ctx.mob().is_on_fire()
            && let Some(water) = look_for_water(ctx.mob(), PANIC_DISTANCE_HORIZONTAL)
        {
            return Some(block_pos_corner(water));
        }
        (self.position_getter)(ctx.mob())
    }
}

impl TimedBehavior for AnimalPanic {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (PANIC_MIN_DURATION, PANIC_MAX_DURATION)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        brain
            .get_memory(memory_module_types::HURT_BY)
            .is_some_and(|source| source.is(&self.panic_causing_damage_types))
            || brain.has_memory_value(memory_module_types::IS_PANICKING.id())
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .set_memory(memory_module_types::IS_PANICKING, true);
        ctx.brain()
            .erase_memory(memory_module_types::WALK_TARGET.id());
        ctx.mob().mob_base().navigation().lock().stop();
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .erase_memory(memory_module_types::IS_PANICKING.id());
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        if !ctx.mob().mob_base().navigation().lock().is_done() {
            return;
        }
        let Some(panic_to) = self.panic_pos(ctx) else {
            return;
        };
        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_position(panic_to, self.speed_multiplier, 0),
        );
    }

    fn debug_name(&self) -> &'static str {
        "AnimalPanic"
    }
}
