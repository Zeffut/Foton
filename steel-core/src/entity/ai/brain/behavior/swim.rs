//! Vanilla `Swim`.

use super::{BrainContext, TimedBehavior};
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryStatus};

/// Jumps to stay afloat.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.Swim`.
pub struct Swim {
    chance: f32,
}

impl Swim {
    /// Jumps on `chance` of the ticks spent swimming.
    #[must_use]
    pub const fn new(chance: f32) -> Self {
        Self { chance }
    }

    /// Vanilla parity: `Swim.shouldSwim`.
    #[must_use]
    pub fn should_swim(mob: &dyn PathfinderMob) -> bool {
        mob.is_in_water() && mob.fluid_contact().water_height() > mob.get_fluid_jump_threshold()
            || mob.is_in_lava()
    }
}

impl TimedBehavior for Swim {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::should_swim(ctx.mob())
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::should_swim(ctx.mob())
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        if rand::random::<f32>() < self.chance {
            ctx.mob().mob_base().controls().lock().jump_control.jump();
        }
    }

    fn debug_name(&self) -> &'static str {
        "Swim"
    }
}
