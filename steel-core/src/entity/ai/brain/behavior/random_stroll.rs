//! Vanilla `RandomStroll`.

use glam::DVec3;

use super::{BrainContext, Trigger};

use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::entity::ai::goal::land_random_pos;

/// Vanilla parity: `RandomStroll.MAX_XZ_DIST`.
const MAX_XZ_DIST: i32 = 10;
/// Vanilla parity: `RandomStroll.MAX_Y_DIST`.
const MAX_Y_DIST: i32 = 7;

/// Where a stroll aims for.
type TargetPicker = Box<dyn Fn(&dyn PathfinderMob) -> Option<DVec3> + Send>;
/// Whether a stroll may run at all.
type StrollGuard = Box<dyn Fn(&dyn PathfinderMob) -> bool + Send>;

/// Sets `WALK_TARGET` to somewhere nearby, once.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.RandomStroll`.
pub struct RandomStroll {
    speed_modifier: f64,
    fetch_target_pos: TargetPicker,
    can_run: StrollGuard,
}

impl RandomStroll {
    /// Strolls up to ten blocks out.
    ///
    /// Vanilla parity: `RandomStroll.stroll(float)`.
    #[must_use]
    pub fn stroll(speed_modifier: f64) -> Self {
        Self::stroll_within(speed_modifier, MAX_XZ_DIST, MAX_Y_DIST)
    }

    /// Strolls no further than the given distances.
    ///
    /// Vanilla parity: `RandomStroll.stroll(float, int, int)`.
    #[must_use]
    pub fn stroll_within(
        speed_modifier: f64,
        max_horizontal_distance: i32,
        max_vertical_distance: i32,
    ) -> Self {
        Self {
            speed_modifier,
            fetch_target_pos: Box::new(move |mob| {
                land_random_pos(mob, max_horizontal_distance, max_vertical_distance)
            }),
            can_run: Box::new(|_| true),
        }
    }

    /// Refuses to stroll out of water.
    ///
    /// Vanilla parity: the `mayStrollFromWater = false` overload.
    #[must_use]
    pub fn not_from_water(mut self) -> Self {
        self.can_run = Box::new(|mob| !mob.is_in_water());
        self
    }
}

impl Trigger for RandomStroll {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::WALK_TARGET.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        // Vanilla parity: the `i.absent(WALK_TARGET)` of the builder group.
        if ctx
            .brain()
            .has_memory_value(memory_module_types::WALK_TARGET.id())
        {
            return false;
        }
        if !(self.can_run)(ctx.mob()) {
            return false;
        }

        let target = (self.fetch_target_pos)(ctx.mob());
        ctx.brain().set_memory_or_erase(
            memory_module_types::WALK_TARGET,
            target.map(|pos| WalkTarget::of_position(pos, self.speed_modifier, 0)),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "RandomStroll"
    }
}
