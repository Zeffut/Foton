//! The exit portal at the center of the End.
//!
//! Vanilla parity: `EndPodiumFeature`. The bedrock pillar with the four torches
//! that the dragon dies over, and the portal home that opens inside it.
//!
//! This is not a worldgen feature in the ordinary sense. Nothing places it
//! during chunk generation: `EnderDragonFight` builds it directly into a live
//! level, inactive when the world is first entered and active once the dragon
//! is dead. So it takes a [`World`] rather than a `WorldGenRegion`, and it is
//! not registered in the configured-feature tables.
//!
//! [`location`] is the other half of its job, and the more widely used one: it
//! is what the dragon, its phases and the fight all aim at.

use std::sync::Arc;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_blocks;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, Direction};

use crate::world::{LevelAccessor as _, LevelReader as _, World};

/// Half-width of the podium's base.
///
/// Vanilla parity: `EndPodiumFeature.PODIUM_RADIUS`.
pub const PODIUM_RADIUS: i32 = 4;

/// Height of the bedrock pillar the portal sits on.
///
/// Vanilla parity: `EndPodiumFeature.PODIUM_PILLAR_HEIGHT`.
pub const PODIUM_PILLAR_HEIGHT: i32 = 4;

/// How far above the origin the podium clears the air out.
///
/// Vanilla parity: the `origin.getY() + 32` of the clearing loop.
const PODIUM_CLEARANCE: i32 = 32;

/// Radius inside which the podium is bedrock and portal.
///
/// Vanilla parity: the `closerThan(origin, 2.5)` of `place`.
const INNER_RIM: f64 = 2.5;

/// Radius inside which the podium is end stone and rim.
///
/// Vanilla parity: the `closerThan(origin, 3.5)` of `place`.
const OUTER_RIM: f64 = 3.5;

/// Where the exit portal stands for a fight based at `origin`.
///
/// Vanilla parity: `EndPodiumFeature.getLocation`. `END_PODIUM_LOCATION` is
/// `BlockPos.ZERO`, so this is the fight origin unchanged -- but every caller
/// in vanilla goes through this rather than using the origin directly, and the
/// dragon's phases follow suit.
#[must_use]
pub const fn location(origin: BlockPos) -> BlockPos {
    origin
}

/// Builds the podium and its portal at `origin`.
///
/// `active` is vanilla's constructor flag: an active podium opens the portal
/// and breaks whatever is in the way, dropping it; an inactive one leaves the
/// portal socket as air and only overwrites.
///
/// Vanilla parity: `EndPodiumFeature.place`.
///
/// NOT WIRED: `EnderDragonFight` is vanilla's only caller -- it places an
/// inactive podium when the End is first entered and an active one when the
/// dragon dies -- and the fight is not implemented. Nothing else in vanilla
/// places a podium, so this waits for it rather than being called from
/// somewhere that would not match.
#[expect(
    dead_code,
    reason = "EnderDragonFight is the only vanilla caller and is not implemented yet"
)]
pub fn place(world: &Arc<World>, origin: BlockPos, active: bool) {
    let bedrock = vanilla_blocks::BEDROCK.default_state();
    let end_stone = vanilla_blocks::END_STONE.default_state();
    let air = vanilla_blocks::AIR.default_state();
    let end_portal = vanilla_blocks::END_PORTAL.default_state();

    for x in origin.x() - PODIUM_RADIUS..=origin.x() + PODIUM_RADIUS {
        for y in origin.y() - 1..=origin.y() + PODIUM_CLEARANCE {
            for z in origin.z() - PODIUM_RADIUS..=origin.z() + PODIUM_RADIUS {
                let pos = BlockPos::new(x, y, z);
                let inside_rim = closer_than(pos, origin, INNER_RIM);
                if !inside_rim && !closer_than(pos, origin, OUTER_RIM) {
                    continue;
                }

                if pos.y() < origin.y() {
                    if inside_rim {
                        world.set_block_state(pos, bedrock, UpdateFlags::UPDATE_ALL);
                    } else if active {
                        drop_previous_and_set_block(world, pos, end_stone);
                    } else {
                        world.set_block_state(pos, end_stone, UpdateFlags::UPDATE_ALL);
                    }
                } else if pos.y() > origin.y() {
                    if active {
                        drop_previous_and_set_block(world, pos, air);
                    } else {
                        world.set_block_state(pos, air, UpdateFlags::UPDATE_ALL);
                    }
                } else if !inside_rim {
                    world.set_block_state(pos, bedrock, UpdateFlags::UPDATE_ALL);
                } else if active {
                    drop_previous_and_set_block(world, pos, end_portal);
                } else {
                    world.set_block_state(pos, air, UpdateFlags::UPDATE_ALL);
                }
            }
        }
    }

    for y in 0..PODIUM_PILLAR_HEIGHT {
        world.set_block_state(origin.above_n(y), bedrock, UpdateFlags::UPDATE_ALL);
    }

    let center_of_pillar = origin.above_n(2);
    for face in Direction::HORIZONTAL {
        let torch = vanilla_blocks::WALL_TORCH
            .default_state()
            .set_value(&BlockStateProperties::HORIZONTAL_FACING, face);
        world.set_block_state(
            center_of_pillar.relative(face),
            torch,
            UpdateFlags::UPDATE_ALL,
        );
    }
}

/// Vanilla parity: `EndPodiumFeature.dropPreviousAndSetBlock`. An active podium
/// breaks what it replaces rather than deleting it, so the obsidian a player
/// piled on the portal comes back to them.
fn drop_previous_and_set_block(
    world: &Arc<World>,
    pos: BlockPos,
    state: steel_utils::BlockStateId,
) {
    if world.get_block_state(pos).get_block() == state.get_block() {
        return;
    }
    world.destroy_block(pos, true);
    world.set_block_state(pos, state, UpdateFlags::UPDATE_ALL);
}

/// Vanilla `Vec3i.closerThan(Vec3i, double)` -- center-to-center.
fn closer_than(pos: BlockPos, other: BlockPos, distance: f64) -> bool {
    let dx = f64::from(pos.x() - other.x());
    let dy = f64::from(pos.y() - other.y());
    let dz = f64::from(pos.z() - other.z());
    dx.mul_add(dx, dy.mul_add(dy, dz * dz)) < distance * distance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portal_stands_where_the_fight_is_centered() {
        assert_eq!(location(BlockPos::new(0, 0, 0)), BlockPos::new(0, 0, 0));
        assert_eq!(
            location(BlockPos::new(96, 0, -32)),
            BlockPos::new(96, 0, -32)
        );
    }

    #[test]
    fn the_rim_is_a_ring_one_block_wide_around_the_portal_socket() {
        let origin = BlockPos::new(0, 64, 0);
        // Vanilla's two radii differ by exactly one block, which is what makes
        // the bedrock rim a single ring rather than a slab.
        let inner = BlockPos::new(2, 64, 0);
        let rim = BlockPos::new(3, 64, 0);
        let outside = BlockPos::new(4, 64, 0);

        assert!(closer_than(inner, origin, INNER_RIM));
        assert!(!closer_than(rim, origin, INNER_RIM));
        assert!(closer_than(rim, origin, OUTER_RIM));
        assert!(!closer_than(outside, origin, OUTER_RIM));
    }
}
