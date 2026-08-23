//! Where a parrot chooses to fly next.
//!
//! Vanilla parity: `Parrot.ParrotWanderGoal`, which is a
//! `WaterAvoidingRandomFlyingGoal` that first looks for a branch to land on.

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::BlockPos;

use crate::entity::PathfinderMob;
use crate::entity::ai::goal::{
    WATER_AVOIDING_RANDOM_STROLL_PROBABILITY, flying_stroll_position, land_random_pos,
};

/// How far a swimming parrot looks for dry land.
///
/// Vanilla parity: the `LandRandomPos.getPos(mob, 15, 15)` of `getPosition`.
const WATER_ESCAPE_RANGE: i32 = 15;

/// How far to the side a parrot looks for a branch.
///
/// Vanilla parity: the `+/- 3.0` of `ParrotWanderGoal.getTreePos`.
const TREE_SEARCH_HORIZONTAL: i32 = 3;

/// How far up and down a parrot looks for a branch.
///
/// Vanilla parity: the `+/- 6.0` of the same method.
const TREE_SEARCH_VERTICAL: i32 = 6;

/// Vanilla parity: `Parrot.ParrotWanderGoal.getPosition`.
#[must_use]
pub(super) fn parrot_wander_position(mob: &dyn PathfinderMob) -> Option<DVec3> {
    let mut position = if mob.is_in_water() {
        land_random_pos(mob, WATER_ESCAPE_RANGE, WATER_ESCAPE_RANGE)
    } else {
        None
    };

    if rand::random::<f32>() >= WATER_AVOIDING_RANDOM_STROLL_PROBABILITY {
        position = tree_position(mob);
    }

    position.or_else(|| flying_stroll_position(mob))
}

/// Finds a perch: an empty block, empty above it, sitting on leaves or a log.
///
/// Vanilla parity: `ParrotWanderGoal.getTreePos`.
fn tree_position(mob: &dyn PathfinderMob) -> Option<DVec3> {
    let world = mob.level()?;
    let mob_pos = mob.block_position();
    let origin = mob.position();
    let min = BlockPos::containing(
        origin.x - f64::from(TREE_SEARCH_HORIZONTAL),
        origin.y - f64::from(TREE_SEARCH_VERTICAL),
        origin.z - f64::from(TREE_SEARCH_HORIZONTAL),
    );
    let max = BlockPos::containing(
        origin.x + f64::from(TREE_SEARCH_HORIZONTAL),
        origin.y + f64::from(TREE_SEARCH_VERTICAL),
        origin.z + f64::from(TREE_SEARCH_HORIZONTAL),
    );

    for x in min.x()..=max.x() {
        for y in min.y()..=max.y() {
            for z in min.z()..=max.z() {
                let pos = BlockPos::new(x, y, z);
                if pos == mob_pos {
                    continue;
                }

                let below = world.get_block_state(pos.below()).get_block();
                let can_sit_on = below.has_tag(&BlockTag::LEAVES) || below.has_tag(&BlockTag::LOGS);
                if !can_sit_on
                    || !world.get_block_state(pos).is_air()
                    || !world.get_block_state(pos.above()).is_air()
                {
                    continue;
                }

                let (bottom_x, bottom_y, bottom_z) = pos.get_bottom_center();
                return Some(DVec3::new(bottom_x, bottom_y, bottom_z));
            }
        }
    }

    None
}
