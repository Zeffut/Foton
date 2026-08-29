//! Vanilla `SetWalkTargetFromLookTarget`.

use super::{BrainContext, Trigger};
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};

/// Whether the body may walk to what it is looking at, and how fast.
type WalkGuard = Box<dyn Fn(&dyn PathfinderMob) -> Option<f64> + Send>;

/// Walks toward whatever is in `LOOK_TARGET`.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetWalkTargetFromLookTarget`.
pub struct SetWalkTargetFromLookTarget {
    speed_modifier: WalkGuard,
    close_enough_distance: i32,
}

impl SetWalkTargetFromLookTarget {
    /// Always walks, at a fixed speed.
    ///
    /// Vanilla parity: `SetWalkTargetFromLookTarget.create(float, int)`.
    #[must_use]
    pub fn new(speed_modifier: f64, close_enough_distance: i32) -> Self {
        Self::conditional(move |_| Some(speed_modifier), close_enough_distance)
    }

    /// Walks only when `speed_modifier` answers with a speed.
    ///
    /// Vanilla parity: the `Predicate` plus `Function<LivingEntity, Float>`
    /// overload, collapsed into one closure because a mob that must not walk
    /// and a mob with no speed are the same case.
    #[must_use]
    pub fn conditional(
        speed_modifier: impl Fn(&dyn PathfinderMob) -> Option<f64> + Send + 'static,
        close_enough_distance: i32,
    ) -> Self {
        Self {
            speed_modifier: Box::new(speed_modifier),
            close_enough_distance,
        }
    }
}

impl Trigger for SetWalkTargetFromLookTarget {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }
        let Some(look_target) = brain.get_memory(memory_module_types::LOOK_TARGET) else {
            return false;
        };
        let Some(speed_modifier) = (self.speed_modifier)(ctx.mob()) else {
            return false;
        };

        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::new(look_target, speed_modifier, self.close_enough_distance),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetWalkTargetFromLookTarget"
    }
}
