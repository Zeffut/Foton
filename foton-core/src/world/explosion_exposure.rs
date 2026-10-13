//! The block states an explosion reads while measuring how exposed each entity is.
//!
//! Vanilla parity: `ServerExplosion.getSeenPercent` clips a grid of rays from every entity in reach
//! to the center -- 27 rays for a primed TNT. In a chain reaction of a few hundred TNT that is more
//! than ten thousand clips per explosion, all through the same few thousand blocks, and looking
//! each of them up through the chunk table again made exposure the most expensive part of a blast.
//! Each block is read once here and kept for the rest of the explosion.
//!
//! An entity's damage can change blocks before the next entity is measured (an end crystal blows
//! up inside the loop), and vanilla measures the next one against the changed world. The cache
//! therefore forgets everything as soon as the world has committed a block change since it filled.

use foton_utils::{BlockPos, BlockStateId};
use glam::DVec3;
use rustc_hash::FxHashMap;

/// Most cells the dense part covers. A blast whose reach box is bigger (a radius past 30 or so)
/// keeps every state in the hash map instead of allocating megabytes.
const MAX_DENSE_CELLS: usize = 1 << 18;

/// Blocks added around the reach box. An entity is in reach when its bounding box meets the box,
/// so its far corners, where some rays start, can sit a little outside it.
const MARGIN: f64 = 3.0;

/// Block states read so far by one explosion.
pub(super) struct ExposureStates {
    min: BlockPos,
    size_x: usize,
    size_y: usize,
    size_z: usize,
    dense: Vec<Option<BlockStateId>>,
    sparse: FxHashMap<BlockPos, BlockStateId>,
    block_writes: u64,
}

impl ExposureStates {
    /// A cache for the box of half-width `reach` around `center`, valid while the world's block
    /// write count stays at `block_writes`. Positions outside the box are cached too, in a map.
    pub(super) fn around(center: DVec3, reach: f64, block_writes: u64) -> Self {
        let half = reach.max(0.0) + MARGIN;
        let min = BlockPos::new(
            (center.x - half).floor() as i32,
            (center.y - half).floor() as i32,
            (center.z - half).floor() as i32,
        );
        let side = |low: f64, high: f64| (high.floor() - low.floor()) as usize + 1;
        let size_x = side(center.x - half, center.x + half);
        let size_y = side(center.y - half, center.y + half);
        let size_z = side(center.z - half, center.z + half);
        let cells = size_x.saturating_mul(size_y).saturating_mul(size_z);
        let dense = if cells <= MAX_DENSE_CELLS {
            vec![None; cells]
        } else {
            Vec::new()
        };
        Self {
            min,
            size_x,
            size_y,
            size_z,
            dense,
            sparse: FxHashMap::default(),
            block_writes,
        }
    }

    /// Drops every cached state if the world has changed a block since the cache filled.
    pub(super) fn sync(&mut self, block_writes: u64) {
        if block_writes == self.block_writes {
            return;
        }
        self.block_writes = block_writes;
        self.dense.fill(None);
        self.sparse.clear();
    }

    /// The state at `pos`, read through `read` the first time it is asked for.
    pub(super) fn state_at(
        &mut self,
        pos: BlockPos,
        read: impl FnOnce(BlockPos) -> BlockStateId,
    ) -> BlockStateId {
        let Some(index) = self.dense_index(pos) else {
            return *self.sparse.entry(pos).or_insert_with(|| read(pos));
        };
        *self.dense[index].get_or_insert_with(|| read(pos))
    }

    fn dense_index(&self, pos: BlockPos) -> Option<usize> {
        if self.dense.is_empty() {
            return None;
        }
        let x = usize::try_from(pos.x() - self.min.x()).ok()?;
        let y = usize::try_from(pos.y() - self.min.y()).ok()?;
        let z = usize::try_from(pos.z() - self.min.z()).ok()?;
        if x >= self.size_x || y >= self.size_y || z >= self.size_z {
            return None;
        }
        Some((y * self.size_z + z) * self.size_x + x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_state(pos: BlockPos) -> BlockStateId {
        BlockStateId((pos.x() * 7 + pos.y() * 13 + pos.z() * 31).rem_euclid(1000) as u16)
    }

    #[test]
    fn every_position_reads_what_the_world_holds_inside_and_outside_the_box() {
        let mut states = ExposureStates::around(DVec3::new(0.5, 64.0, -0.5), 8.0, 0);
        for x in -20..20 {
            for y in 44..84 {
                for z in -20..20 {
                    let pos = BlockPos::new(x, y, z);
                    assert_eq!(states.state_at(pos, fake_state), fake_state(pos), "{pos:?}");
                }
            }
        }
        // A second pass is served from the cache and still answers the same.
        for x in -20..20 {
            let pos = BlockPos::new(x, 64, 3);
            assert_eq!(
                states.state_at(pos, |_| unreachable!("cached")),
                fake_state(pos)
            );
        }
    }

    #[test]
    fn a_block_change_makes_the_cache_read_again() {
        let pos = BlockPos::new(1, 64, 1);
        let mut states = ExposureStates::around(DVec3::new(0.0, 64.0, 0.0), 8.0, 5);
        assert_eq!(states.state_at(pos, |_| BlockStateId(1)), BlockStateId(1));
        states.sync(5);
        assert_eq!(states.state_at(pos, |_| BlockStateId(2)), BlockStateId(1));
        states.sync(6);
        assert_eq!(states.state_at(pos, |_| BlockStateId(2)), BlockStateId(2));
    }

    #[test]
    fn a_reach_too_big_for_the_dense_box_still_caches() {
        let mut states = ExposureStates::around(DVec3::ZERO, 256.0, 0);
        let pos = BlockPos::new(100, 10, -100);
        assert_eq!(states.state_at(pos, |_| BlockStateId(9)), BlockStateId(9));
        assert_eq!(states.state_at(pos, |_| BlockStateId(3)), BlockStateId(9));
    }
}
