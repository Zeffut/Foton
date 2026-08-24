//! Shared helpers behaviors reach for.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.BehaviorUtils`.

use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Points the body at `target` and sends it walking there.
///
/// Vanilla parity: `BehaviorUtils.setWalkAndLookTargetMemories`.
pub(crate) fn set_walk_and_look_target_memories(
    brain: &Brain,
    target: PositionTracker,
    speed_modifier: f64,
    close_enough_dist: i32,
) {
    brain.set_memory(
        memory_module_types::WALK_TARGET,
        WalkTarget::new(target.clone(), speed_modifier, close_enough_dist),
    );
    brain.set_memory(memory_module_types::LOOK_TARGET, target);
}
