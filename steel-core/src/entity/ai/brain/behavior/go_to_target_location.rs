//! Vanilla `GoToTargetLocation`.

use steel_utils::BlockPos;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryModuleType, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Walks to a remembered block position and mills about near it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.GoToTargetLocation`.
pub struct GoToTargetLocation {
    location_memory: MemoryModuleType<BlockPos>,
    close_enough_dist: i32,
    speed_modifier: f64,
}

impl GoToTargetLocation {
    /// Vanilla parity: `GoToTargetLocation.create`.
    #[must_use]
    pub const fn new(
        location_memory: MemoryModuleType<BlockPos>,
        close_enough_dist: i32,
        speed_modifier: f64,
    ) -> Self {
        Self {
            location_memory,
            close_enough_dist,
            speed_modifier,
        }
    }

    /// Vanilla parity: `GoToTargetLocation.getNearbyPos`, which scatters the
    /// destination by one block so a crowd does not stack on the same spot.
    fn nearby_pos(pos: BlockPos) -> BlockPos {
        BlockPos::new(
            pos.x() + rand::random_range(0..3) - 1,
            pos.y(),
            pos.z() + rand::random_range(0..3) - 1,
        )
    }
}

impl Trigger for GoToTargetLocation {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            self.location_memory.id(),
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ATTACK_TARGET.id())
            || brain.has_memory_value(memory_module_types::WALK_TARGET.id())
        {
            return false;
        }
        let Some(location) = brain.get_memory(self.location_memory) else {
            return false;
        };

        let close_enough = utils::block_closer_than(
            location,
            ctx.mob().block_position(),
            f64::from(self.close_enough_dist),
        );
        if !close_enough {
            utils::set_walk_and_look_target_memories(
                brain,
                PositionTracker::of_block(Self::nearby_pos(location)),
                self.speed_modifier,
                self.close_enough_dist,
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "GoToTargetLocation"
    }
}
