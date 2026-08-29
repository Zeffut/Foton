//! Drifting somewhere else for no reason at all.
//!
//! Vanilla parity: `Ghast.RandomFloatAroundGoal`, a public goal shared by the
//! ghast and the happy ghast. It is the whole of a ghast's idle movement: pick
//! a point within sixteen blocks, hand it to the move control, and let the move
//! control coast there.

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_utils::{BlockPos, Direction};
use glam::DVec3;

use super::selector::{Goal, GoalControls};
use crate::chunk::heightmap::HeightmapType;
use crate::entity::{Mob, PathfinderMob};
use crate::world::{LevelAccessor as _, LevelReader as _, World};

/// How many spots the goal tries before it settles for whatever it last drew.
///
/// Vanilla parity: `RandomFloatAroundGoal.MAX_ATTEMPTS`.
const MAX_ATTEMPTS: i32 = 64;

/// Half the span of one axis of the search box, in blocks.
///
/// Vanilla parity: the `16.0F` of `chooseRandomPosition`.
const SEARCH_SPAN: f64 = 16.0;

/// Squared distance below which the current destination is too close to bother
/// flying to.
///
/// Vanilla parity: the `dd < 1.0` of `RandomFloatAroundGoal.canUse`.
const TOO_CLOSE_SQR: f64 = 1.0;

/// Squared distance beyond which the current destination is written off.
///
/// Vanilla parity: the `dd > 3600.0` of the same method.
const TOO_FAR_SQR: f64 = 3600.0;

/// Speed multiplier of an idle drift.
const FLOAT_SPEED_MODIFIER: f64 = 1.0;

/// Picks somewhere for a floating mob to drift to.
///
/// Vanilla parity: `Ghast.RandomFloatAroundGoal.getSuitableFlyToPosition`.
/// `distance_to_blocks` is the happy ghast's "stay near something solid"
/// preference; a ghast passes `0`, which accepts any point.
pub(crate) fn suitable_fly_to_position(mob: &dyn Mob, distance_to_blocks: i32) -> DVec3 {
    let Some(world) = mob.level() else {
        return mob.position();
    };
    let center = mob.position();
    let mut result = None;

    for _ in 0..MAX_ATTEMPTS {
        let candidate = choose_random_position_with_restriction(mob, center);
        if candidate.is_some() {
            result = candidate;
        }
        if let Some(candidate) = candidate
            && is_good_target(&world, candidate, distance_to_blocks)
        {
            return candidate;
        }
    }

    let mut result = result.unwrap_or_else(|| choose_random_position(center));

    // Vanilla parity: a target under the terrain is mirrored back to the same
    // distance below the mob instead, so a ghast that rolls a point inside a
    // hill dives rather than swimming through it.
    let pos = BlockPos::containing(result.x, result.y, result.z);
    let height_y = world.heightmap_at(HeightmapType::MotionBlocking, pos.x(), pos.z());
    if height_y < pos.y() && height_y > world.min_y() {
        result = DVec3::new(result.x, center.y - (center.y - result.y).abs(), result.z);
    }

    result
}

/// Vanilla parity: `RandomFloatAroundGoal.isGoodTarget`.
fn is_good_target(world: &World, target: DVec3, distance_to_blocks: i32) -> bool {
    if distance_to_blocks <= 0 {
        return true;
    }

    let pos = BlockPos::containing(target.x, target.y, target.z);
    if !world.get_block_state(pos).is_air() {
        return false;
    }

    Direction::ALL.into_iter().any(|direction| {
        (1..distance_to_blocks).any(|step| {
            !world
                .get_block_state(pos.relative_n(direction, step))
                .is_air()
        })
    })
}

/// Vanilla parity: `RandomFloatAroundGoal.chooseRandomPosition`.
fn choose_random_position(center: DVec3) -> DVec3 {
    fn offset() -> f64 {
        f64::from(rand::random::<f32>().mul_add(2.0, -1.0)) * SEARCH_SPAN
    }

    DVec3::new(
        center.x + offset(),
        center.y + offset(),
        center.z + offset(),
    )
}

/// Vanilla parity: `RandomFloatAroundGoal.chooseRandomPositionWithRestriction`.
fn choose_random_position_with_restriction(mob: &dyn Mob, center: DVec3) -> Option<DVec3> {
    let target = choose_random_position(center);
    if mob.has_home() && !mob.is_within_home_vec(target) {
        return None;
    }

    Some(target)
}

/// Drifts to a random point whenever the move control has nowhere useful to be.
///
/// Vanilla parity: `Ghast.RandomFloatAroundGoal`.
pub(crate) struct RandomFloatAroundGoal {
    distance_to_blocks: i32,
}

impl RandomFloatAroundGoal {
    /// Creates the ghast's own configuration, which accepts any point.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self::with_distance_to_blocks(0)
    }

    /// Creates a drift that prefers points near solid blocks.
    ///
    /// Vanilla parity: the two-argument `RandomFloatAroundGoal(mob, distance)`.
    #[must_use]
    pub(crate) const fn with_distance_to_blocks(distance_to_blocks: i32) -> Self {
        Self { distance_to_blocks }
    }
}

impl Goal for RandomFloatAroundGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    /// Vanilla parity: `RandomFloatAroundGoal.canUse`. The mob re-rolls when it
    /// has nowhere to be, when it has already arrived, and when whatever it was
    /// aiming at has ended up sixty blocks away.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let (has_wanted, wanted_position) = {
            let controls = mob.mob_base().controls().lock();
            (
                controls.move_control.has_wanted(),
                controls.move_control.wanted_position(),
            )
        };
        if !has_wanted {
            return true;
        }

        let distance_sqr = wanted_position.distance_squared(mob.position());
        distance_sqr < TOO_CLOSE_SQR || distance_sqr > TOO_FAR_SQR
    }

    /// Vanilla parity: `RandomFloatAroundGoal.canContinueToUse` returns false,
    /// so the goal runs for exactly the one tick it takes to pick a point.
    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let target = suitable_fly_to_position(mob, self.distance_to_blocks);
        mob.mob_base()
            .controls()
            .lock()
            .move_control
            .set_wanted_position(target, FLOAT_SPEED_MODIFIER);
    }
}
