//! Vanilla `StrollToPoi` and `StrollAroundPoi`.

use steel_utils::GlobalPos;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{
    MemoryModuleId, MemoryModuleType, WalkTarget, memory_module_types,
};
use crate::entity::ai::goal::land_random_pos;

/// Vanilla parity: the `nextOkStartTime + 80L` of `StrollToPoi`.
const MIN_TIME_BETWEEN_WALKS: i64 = 80;
/// Vanilla parity: `StrollAroundPoi.MIN_TIME_BETWEEN_STROLLS`.
const MIN_TIME_BETWEEN_STROLLS: i64 = 180;
/// Vanilla parity: `StrollAroundPoi.STROLL_MAX_XZ_DIST`.
const STROLL_MAX_XZ_DIST: i32 = 8;
/// Vanilla parity: `StrollAroundPoi.STROLL_MAX_Y_DIST`.
const STROLL_MAX_Y_DIST: i32 = 6;

/// Walks back toward a remembered point of interest.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.StrollToPoi`.
pub struct StrollToPoi {
    memory: MemoryModuleType<GlobalPos>,
    speed_modifier: f64,
    close_enough_dist: i32,
    max_distance_from_poi: f64,
    next_ok_start_time: i64,
}

impl StrollToPoi {
    /// Vanilla parity: `StrollToPoi.create`.
    #[must_use]
    pub const fn new(
        memory: MemoryModuleType<GlobalPos>,
        speed_modifier: f64,
        close_enough_dist: i32,
        max_distance_from_poi: i32,
    ) -> Self {
        Self {
            memory,
            speed_modifier,
            close_enough_dist,
            max_distance_from_poi: max_distance_from_poi as f64,
            next_ok_start_time: 0,
        }
    }
}

impl Trigger for StrollToPoi {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::WALK_TARGET.id(), self.memory.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(poi) = brain.get_memory(self.memory) else {
            return false;
        };
        if poi.dimension != ctx.world().key
            || !utils::block_closer_to_center_than(
                poi.pos,
                ctx.mob().position(),
                self.max_distance_from_poi,
            )
        {
            return false;
        }
        if ctx.game_time() <= self.next_ok_start_time {
            return true;
        }

        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_block(poi.pos, self.speed_modifier, self.close_enough_dist),
        );
        self.next_ok_start_time = ctx.game_time() + MIN_TIME_BETWEEN_WALKS;
        true
    }

    fn debug_name(&self) -> &'static str {
        "StrollToPoi"
    }
}

/// Wanders within reach of a remembered point of interest.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.StrollAroundPoi`.
pub struct StrollAroundPoi {
    memory: MemoryModuleType<GlobalPos>,
    speed_modifier: f64,
    max_distance_from_poi: f64,
    next_ok_start_time: i64,
}

impl StrollAroundPoi {
    /// Vanilla parity: `StrollAroundPoi.create`.
    #[must_use]
    pub const fn new(
        memory: MemoryModuleType<GlobalPos>,
        speed_modifier: f64,
        max_distance_from_poi: i32,
    ) -> Self {
        Self {
            memory,
            speed_modifier,
            max_distance_from_poi: max_distance_from_poi as f64,
            next_ok_start_time: 0,
        }
    }
}

impl Trigger for StrollAroundPoi {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::WALK_TARGET.id(), self.memory.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(poi) = brain.get_memory(self.memory) else {
            return false;
        };
        if poi.dimension != ctx.world().key
            || !utils::block_closer_to_center_than(
                poi.pos,
                ctx.mob().position(),
                self.max_distance_from_poi,
            )
        {
            return false;
        }
        if ctx.game_time() <= self.next_ok_start_time {
            return true;
        }

        let stroll_to = land_random_pos(ctx.mob(), STROLL_MAX_XZ_DIST, STROLL_MAX_Y_DIST);
        brain.set_memory_or_erase(
            memory_module_types::WALK_TARGET,
            stroll_to.map(|pos| WalkTarget::of_position(pos, self.speed_modifier, 1)),
        );
        self.next_ok_start_time = ctx.game_time() + MIN_TIME_BETWEEN_STROLLS;
        true
    }

    fn debug_name(&self) -> &'static str {
        "StrollAroundPoi"
    }
}
