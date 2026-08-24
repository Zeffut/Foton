//! The `WALK_TARGET` memory value.

use glam::DVec3;
use steel_utils::BlockPos;

use super::super::position_tracker::PositionTracker;
use crate::entity::SharedEntity;

/// Where a brain wants its body to walk, and how fast.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.memory.WalkTarget`.
#[derive(Debug, Clone)]
pub struct WalkTarget {
    target: PositionTracker,
    speed_modifier: f64,
    close_enough_dist: i32,
}

impl WalkTarget {
    /// Walks to a tracked point.
    #[must_use]
    pub const fn new(target: PositionTracker, speed_modifier: f64, close_enough_dist: i32) -> Self {
        Self {
            target,
            speed_modifier,
            close_enough_dist,
        }
    }

    /// Walks to a block.
    #[must_use]
    pub fn of_block(target: BlockPos, speed_modifier: f64, close_enough_dist: i32) -> Self {
        Self::new(
            PositionTracker::of_block(target),
            speed_modifier,
            close_enough_dist,
        )
    }

    /// Walks to an exact point.
    ///
    /// Vanilla parity: `new WalkTarget(Vec3, float, int)`, which rounds the
    /// point down to its block before tracking it.
    #[must_use]
    pub fn of_position(target: DVec3, speed_modifier: f64, close_enough_dist: i32) -> Self {
        Self::new(
            PositionTracker::of_block(BlockPos::containing(target.x, target.y, target.z)),
            speed_modifier,
            close_enough_dist,
        )
    }

    /// Walks to an entity.
    #[must_use]
    pub fn of_entity(target: &SharedEntity, speed_modifier: f64, close_enough_dist: i32) -> Self {
        Self::new(
            PositionTracker::of_entity(target, false),
            speed_modifier,
            close_enough_dist,
        )
    }

    /// Returns what is being walked to.
    #[must_use]
    pub const fn target(&self) -> &PositionTracker {
        &self.target
    }

    /// Returns the speed multiplier to walk at.
    #[must_use]
    pub const fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    /// Returns the Manhattan distance that already counts as arrived.
    #[must_use]
    pub const fn close_enough_dist(&self) -> i32 {
        self.close_enough_dist
    }
}
