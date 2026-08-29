//! Vanilla `MoveToTargetSink`.

use std::f64::consts::FRAC_PI_2;

use foton_utils::BlockPos;
use glam::DVec3;

use super::{BrainContext, TimedBehavior};

use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{
    MemoryModuleId, MemoryStatus, WalkTarget, memory_module_types,
};
use crate::entity::ai::goal::default_random_pos_towards;
use crate::entity::ai::path::Path;

/// Vanilla parity: `MoveToTargetSink.MAX_COOLDOWN_BEFORE_RETRYING`.
const MAX_COOLDOWN_BEFORE_RETRYING: i32 = 40;
/// Vanilla parity: the `10` of the `DefaultRandomPos.getPosTowards` fallback.
const PARTIAL_STEP_HORIZONTAL_RANGE: i32 = 10;
/// Vanilla parity: the `7` of the same call.
const PARTIAL_STEP_VERTICAL_RANGE: i32 = 7;

/// Vanilla parity: `BlockPos.distSqr(Vec3i)`.
fn squared_distance(from: BlockPos, to: BlockPos) -> i64 {
    let dx = i64::from(from.x() - to.x());
    let dy = i64::from(from.y() - to.y());
    let dz = i64::from(from.z() - to.z());
    dx * dx + dy * dy + dz * dz
}

/// Walks the body to whatever is in `WALK_TARGET`.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.MoveToTargetSink`.
/// This is the behavior that turns a memory into movement; without it a brain
/// can set `WALK_TARGET` all day and the mob never leaves the spot.
pub struct MoveToTargetSink {
    entry_condition: [(MemoryModuleId, MemoryStatus); 3],
    min_timeout: i32,
    max_timeout: i32,
    remaining_cooldown: i32,
    path: Option<Path>,
    last_target_pos: Option<BlockPos>,
    speed_modifier: f64,
    extra_condition: Option<ExtraMoveCondition>,
}

/// A reason of the mob's own to refuse to walk anywhere.
type ExtraMoveCondition = Box<dyn Fn(&dyn PathfinderMob) -> bool + Send>;

impl Default for MoveToTargetSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveToTargetSink {
    /// Vanilla parity: `new MoveToTargetSink()`, which times out after 150 to
    /// 250 ticks.
    #[must_use]
    pub const fn new() -> Self {
        Self::with_timeout(150, 250)
    }

    /// Vanilla parity: `new MoveToTargetSink(int, int)`.
    #[must_use]
    pub const fn with_timeout(min_timeout: i32, max_timeout: i32) -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id(),
                    MemoryStatus::Registered,
                ),
                (memory_module_types::PATH.id(), MemoryStatus::ValueAbsent),
                (
                    memory_module_types::WALK_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
            ],
            min_timeout,
            max_timeout,
            remaining_cooldown: 0,
            path: None,
            last_target_pos: None,
            speed_modifier: 0.0,
            extra_condition: None,
        }
    }

    /// Adds a reason of the mob's own to stay put.
    ///
    /// Vanilla parity: the anonymous `MoveToTargetSink` subclass in
    /// `ArmadilloAi.initCoreActivity`, whose `checkExtraStartConditions`
    /// refuses outright while the armadillo is balled up.
    #[must_use]
    pub fn with_extra_condition(
        mut self,
        condition: impl Fn(&dyn PathfinderMob) -> bool + Send + 'static,
    ) -> Self {
        self.extra_condition = Some(Box::new(condition));
        self
    }

    /// Vanilla parity: `MoveToTargetSink.reachedTarget`.
    fn reached_target(ctx: &BrainContext<'_>, walk_target: &WalkTarget) -> bool {
        let Some(target) = walk_target.target().current_block_position() else {
            return false;
        };
        let body = ctx.mob().block_position();
        let manhattan = (target.x() - body.x()).abs()
            + (target.y() - body.y()).abs()
            + (target.z() - body.z()).abs();
        manhattan <= walk_target.close_enough_dist()
    }

    /// Vanilla parity: `MoveToTargetSink.tryComputePath`.
    fn try_compute_path(&mut self, ctx: &BrainContext<'_>, walk_target: &WalkTarget) -> bool {
        let Some(target_pos) = walk_target.target().current_block_position() else {
            return false;
        };
        self.path = ctx.mob().create_path_to(target_pos, 0);
        self.speed_modifier = walk_target.speed_modifier();

        let brain = ctx.brain();
        if Self::reached_target(ctx, walk_target) {
            brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
            return false;
        }

        let can_reach = self.path.as_ref().is_some_and(Path::can_reach);
        if can_reach {
            brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
        } else if !brain.has_memory_value(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id()) {
            brain.set_memory(
                memory_module_types::CANT_REACH_WALK_TARGET_SINCE,
                ctx.game_time(),
            );
        }

        if self.path.is_some() {
            return true;
        }

        // Vanilla parity: when no full path exists, head part of the way there.
        let (target_x, target_y, target_z) = target_pos.get_bottom_center();
        let Some(partial_step) = default_random_pos_towards(
            ctx.mob(),
            PARTIAL_STEP_HORIZONTAL_RANGE,
            PARTIAL_STEP_VERTICAL_RANGE,
            DVec3::new(target_x, target_y, target_z),
            FRAC_PI_2,
        ) else {
            return false;
        };
        self.path = ctx.mob().create_path_to(
            BlockPos::containing(partial_step.x, partial_step.y, partial_step.z),
            0,
        );
        self.path.is_some()
    }
}

impl TimedBehavior for MoveToTargetSink {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (self.min_timeout, self.max_timeout)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        if self
            .extra_condition
            .as_ref()
            .is_some_and(|condition| !condition(ctx.mob()))
        {
            return false;
        }
        if self.remaining_cooldown > 0 {
            self.remaining_cooldown -= 1;
            return false;
        }

        let brain = ctx.brain();
        let Some(walk_target) = brain.get_memory(memory_module_types::WALK_TARGET) else {
            return false;
        };

        let reached_target = Self::reached_target(ctx, &walk_target);
        if !reached_target && self.try_compute_path(ctx, &walk_target) {
            self.last_target_pos = walk_target.target().current_block_position();
            return true;
        }

        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        if reached_target {
            brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
        }
        false
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        if self.path.is_none() || self.last_target_pos.is_none() {
            return false;
        }
        let Some(walk_target) = ctx.brain().get_memory(memory_module_types::WALK_TARGET) else {
            return false;
        };
        // Vanilla also refuses to keep following a spectator; Foton's
        // `TargetingConditions` already excludes spectators before a walk
        // target is ever set from an entity, and a `PositionTracker` whose
        // entity is gone reports no position, which stops this the same way.
        let navigation_done = ctx.mob().mob_base().navigation().lock().is_done();
        !navigation_done && !Self::reached_target(ctx, &walk_target)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        ctx.brain().set_memory(memory_module_types::PATH, path);
        ctx.mob()
            .move_to_path(self.path.clone(), self.speed_modifier);
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let new_path = ctx.mob().mob_base().navigation().lock().path().cloned();
        if self.path != new_path {
            self.path.clone_from(&new_path);
            ctx.brain()
                .set_memory_or_erase(memory_module_types::PATH, new_path.clone());
        }

        let (Some(_), Some(last_target_pos)) = (new_path, self.last_target_pos) else {
            return;
        };
        let Some(walk_target) = ctx.brain().get_memory(memory_module_types::WALK_TARGET) else {
            return;
        };
        let Some(target_pos) = walk_target.target().current_block_position() else {
            return;
        };
        if squared_distance(target_pos, last_target_pos) > 4
            && self.try_compute_path(ctx, &walk_target)
        {
            self.last_target_pos = Some(target_pos);
            self.start(ctx);
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        let still_walking = brain
            .get_memory(memory_module_types::WALK_TARGET)
            .is_some_and(|walk_target| !Self::reached_target(ctx, &walk_target));
        if still_walking && ctx.mob().mob_base().navigation().lock().is_stuck() {
            self.remaining_cooldown = rand::random_range(0..MAX_COOLDOWN_BEFORE_RETRYING);
        }

        ctx.mob().mob_base().navigation().lock().stop();
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::PATH.id());
        self.path = None;
    }

    fn debug_name(&self) -> &'static str {
        "MoveToTargetSink"
    }
}
