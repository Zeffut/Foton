//! Vanilla `RandomStroll`.

use std::f64::consts::FRAC_PI_2;

use glam::DVec3;

use super::{BrainContext, Trigger};

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_utils::BlockPos;

use crate::behavior::BLOCK_BEHAVIORS;
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::entity::ai::goal::{air_and_water_random_pos, default_random_pos, land_random_pos};
use crate::entity::ai::path::PathComputationType;
use crate::fluid::FluidStateExt as _;
use crate::world::LevelReader as _;

/// How many times `getRandomSwimmablePos` re-rolls before giving up.
///
/// Vanilla parity: the `attempts++ < 10` of `BehaviorUtils.getRandomSwimmablePos`.
const RANDOM_SWIMMABLE_POS_ATTEMPTS: usize = 10;

/// Vanilla parity: `RandomStroll.MAX_XZ_DIST`.
const MAX_XZ_DIST: i32 = 10;
/// Vanilla parity: `RandomStroll.MAX_Y_DIST`.
const MAX_Y_DIST: i32 = 7;
/// Vanilla parity: `RandomStroll.SWIM_XY_DISTANCE_TIERS`.
///
/// A swimming stroll walks outward through these tiers, each one aimed along
/// the direction the last found, so the mob drifts in a line rather than
/// picking one point at random and turning for it.
const SWIM_XY_DISTANCE_TIERS: [(i32, i32); 6] = [(1, 1), (3, 3), (5, 5), (6, 5), (7, 7), (10, 7)];

/// Vanilla parity: the `-2` flying height of `RandomStroll.getTargetFlyPos`,
/// which is what keeps a wandering flier drifting gently downward.
const FLY_TARGET_HEIGHT: i32 = -2;
/// Vanilla parity: the `(float) (Math.PI / 2)` of `getTargetFlyPos`.
const FLY_MAX_XZ_RADIANS_FROM_DIR: f64 = FRAC_PI_2;

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

    /// Drifts to a spot in the air ahead of where the mob is looking.
    ///
    /// Vanilla parity: `RandomStroll.fly(float)`, whose `getTargetFlyPos` aims
    /// the search along the view vector inside a quarter turn either way, so a
    /// flier wanders forward rather than doubling back on itself.
    #[must_use]
    pub fn fly(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            fetch_target_pos: Box::new(|mob| {
                // Vanilla parity: `body.getViewVector(0.0F)`, whose zero
                // partial tick reads the *previous* tick's rotation.
                let (old_yaw, old_pitch) = mob.base().old_rotation();
                let view = mob.calculate_view_vector(old_pitch, old_yaw);
                air_and_water_random_pos(
                    mob,
                    MAX_XZ_DIST,
                    MAX_Y_DIST,
                    FLY_TARGET_HEIGHT,
                    view.x,
                    view.z,
                    FLY_MAX_XZ_RADIANS_FROM_DIR,
                )
            }),
            can_run: Box::new(|_| true),
        }
    }

    /// Strolls along the water, and only while in it.
    ///
    /// Vanilla parity: `RandomStroll.swim(float)`.
    #[must_use]
    pub fn swim(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            fetch_target_pos: Box::new(target_swim_pos),
            can_run: Box::new(<dyn PathfinderMob>::is_in_water),
        }
    }
}

/// Vanilla parity: `RandomStroll.getTargetSwimPos`.
///
/// Each tier after the first extends the previous answer outward rather than
/// rolling again, and the walk stops at the first tier that leaves the water or
/// the mob's home radius -- so the target is the furthest point along a line
/// that is still swimmable.
fn target_swim_pos(mob: &dyn PathfinderMob) -> Option<DVec3> {
    let world = mob.level()?;
    let mut fallback: Option<DVec3> = None;
    let mut target_pos: Option<DVec3> = None;

    for (horizontal, vertical) in SWIM_XY_DISTANCE_TIERS {
        target_pos = match fallback {
            None => random_swimmable_pos(mob, horizontal, vertical),
            Some(previous) => {
                let step = (previous - mob.position()).normalize_or_zero()
                    * DVec3::new(
                        f64::from(horizontal),
                        f64::from(vertical),
                        f64::from(horizontal),
                    );
                Some(mob.position() + step)
            }
        };

        let leaves_water = target_pos.is_none_or(|pos| {
            !world
                .get_block_state(BlockPos::containing(pos.x, pos.y, pos.z))
                .get_fluid_state()
                .is_water()
        });
        let outside_home = target_pos.is_some_and(|pos| {
            mob.has_home()
                && block_center_distance_sqr(mob.home_position(), mob.position())
                    < (f64::from(mob.home_radius()) + f64::from(horizontal) + 1.0).powi(2)
                && !mob.is_within_home_vec(pos)
        });
        if leaves_water || outside_home {
            return fallback;
        }

        fallback = target_pos;
    }

    target_pos
}

/// Vanilla parity: `BehaviorUtils.getRandomSwimmablePos`, which re-rolls up to
/// ten times for a point a swimmer can actually path into.
fn random_swimmable_pos(
    mob: &dyn PathfinderMob,
    horizontal_dist: i32,
    vertical_dist: i32,
) -> Option<DVec3> {
    let world = mob.level()?;
    for _ in 0..=RANDOM_SWIMMABLE_POS_ATTEMPTS {
        let pos = default_random_pos(mob, horizontal_dist, vertical_dist)?;
        let state = world.get_block_state(BlockPos::containing(pos.x, pos.y, pos.z));
        if BLOCK_BEHAVIORS
            .get_behavior(state.get_block())
            .is_pathfindable(state, PathComputationType::Water)
        {
            return Some(pos);
        }
    }
    None
}

fn block_center_distance_sqr(pos: BlockPos, target: DVec3) -> f64 {
    let (x, y, z) = pos.get_center();
    DVec3::new(x, y, z).distance_squared(target)
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
