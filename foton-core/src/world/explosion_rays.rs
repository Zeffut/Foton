//! The rays that decide which blocks an explosion breaks.
//!
//! Vanilla parity: the ray loop of `ServerExplosion.calculateExplodedPositions`. It is a function of
//! a few closures rather than of a `World` so that it can be tested against a plain reference, and
//! because the only thing worth changing about it is how it looks blocks up.
//!
//! A blast sends 1352 rays over ranges that overlap almost entirely: for a TNT that is about 26 000
//! steps over roughly a thousand distinct blocks. Looking each step up through the chunk table, and
//! asking a `Vec` whether the position was already taken, cost more than 4 ms per explosion; two
//! hundred TNT in one tick froze the server for a second. Each distinct block is looked up once here,
//! and "already taken" is a flag.

use std::mem;

use foton_math::fast_floor;
use foton_utils::BlockPos;
use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};

/// Rays are cast from the shell of a grid this many cells wide.
pub(super) const RAY_GRID_SIZE: i32 = 16;

/// Blocks a ray advances per step.
const RAY_STEP: f32 = 0.3;

/// Power a step costs on top of what the block it crosses resists.
const RAY_STEP_COST: f32 = 0.225_000_01;

/// Longest cube side the dense cache covers, in cells. Past it (a blast of radius 17 or so) the
/// cache falls back to hash maps rather than allocate megabytes per explosion.
const MAX_DENSE_CELLS: usize = 1 << 18;

/// A resistance cell that has not been looked up yet.
const UNKNOWN: f32 = f32::NAN;

/// A resistance cell for a position that opposes the blast with nothing.
const NO_RESISTANCE: f32 = -1.0;

/// How far from its center's block a blast of `radius` can reach, in blocks.
///
/// A ray's power is at most 1.3 times the radius and every step costs at least `RAY_STEP_COST`
/// while moving `RAY_STEP`, which bounds how far from the center it can get.
pub(super) fn blast_half_width(radius: f32) -> i32 {
    let power = f64::from(radius.max(0.0)) * 1.3;
    let reach = (power / f64::from(RAY_STEP_COST) + 1.0) * f64::from(RAY_STEP);
    reach.ceil() as i32 + 2
}

/// What one explosion has already learned about the blocks around it.
struct BlastCache {
    origin: BlockPos,
    half: i32,
    side: usize,
    resistance: Vec<f32>,
    taken: Vec<bool>,
    sparse_resistance: FxHashMap<BlockPos, Option<f32>>,
    sparse_taken: FxHashSet<BlockPos>,
}

impl BlastCache {
    /// A cache for a blast of `radius` centered in `origin`'s block.
    ///
    /// A ray's power is at most 1.3 times the radius and every step costs at least `RAY_STEP_COST`
    /// while moving `RAY_STEP`, which bounds how far from the center it can get. Positions outside
    /// the bound are still correct; they just use the hash maps.
    fn new(origin: BlockPos, radius: f32) -> Self {
        let half = blast_half_width(radius);
        let side = (half as usize) * 2 + 1;
        let cells = side.saturating_mul(side).saturating_mul(side);
        let dense = cells <= MAX_DENSE_CELLS;
        Self {
            origin,
            half,
            side,
            resistance: if dense {
                vec![UNKNOWN; cells]
            } else {
                Vec::new()
            },
            taken: if dense {
                vec![false; cells]
            } else {
                Vec::new()
            },
            sparse_resistance: FxHashMap::default(),
            sparse_taken: FxHashSet::default(),
        }
    }

    const fn dense_index(&self, pos: BlockPos) -> Option<usize> {
        if self.resistance.is_empty() {
            return None;
        }
        let dx = pos.x() - self.origin.x();
        let dy = pos.y() - self.origin.y();
        let dz = pos.z() - self.origin.z();
        if dx.abs() > self.half || dy.abs() > self.half || dz.abs() > self.half {
            return None;
        }
        let half = self.half;
        let side = self.side;
        Some(((dx + half) as usize * side + (dy + half) as usize) * side + (dz + half) as usize)
    }

    /// The resistance at `pos`, whose dense cell is `index`, asking `lookup` only the first time.
    fn resistance(
        &mut self,
        pos: BlockPos,
        index: Option<usize>,
        lookup: &mut impl FnMut(BlockPos) -> Option<f32>,
    ) -> Option<f32> {
        if let Some(index) = index {
            let cached = self.resistance[index];
            if !cached.is_nan() {
                return (cached != NO_RESISTANCE).then_some(cached);
            }
            let found = lookup(pos);
            self.resistance[index] = found.unwrap_or(NO_RESISTANCE);
            return found;
        }
        *self
            .sparse_resistance
            .entry(pos)
            .or_insert_with(|| lookup(pos))
    }

    /// Whether `pos` held air with no fluid when the rays crossed it.
    fn was_empty(&self, pos: BlockPos) -> bool {
        match self.dense_index(pos) {
            Some(index) => self.resistance[index] == NO_RESISTANCE,
            None => matches!(self.sparse_resistance.get(&pos), Some(None)),
        }
    }

    /// Marks `pos`, whose dense cell is `index`, as taken by the blast. True the first time only.
    fn take(&mut self, pos: BlockPos, index: Option<usize>) -> bool {
        match index {
            Some(index) => !mem::replace(&mut self.taken[index], true),
            None => self.sparse_taken.insert(pos),
        }
    }
}

/// The blocks a blast reaches, and what it saw of them on the way.
pub(super) struct Blast {
    /// The blocks the rays reached with power left, in the order they first reached them.
    pub(super) to_blow: Vec<BlockPos>,
    cache: BlastCache,
}

impl Blast {
    /// Whether `pos` held air with no fluid when the rays crossed it. Only meaningful while the
    /// world has not changed a block since.
    pub(super) fn was_empty(&self, pos: BlockPos) -> bool {
        self.cache.was_empty(pos)
    }
}

/// Casts the rays of a blast and returns the blocks they reach with power left.
///
/// `roll` supplies each ray's random power factor in `0..1`, `in_world` says whether a height is
/// inside the build limits, and `resistance_at` is what a block and its fluid oppose to the blast, or
/// `None` for nothing.
pub(super) fn cast_rays(
    center: DVec3,
    radius: f32,
    mut roll: impl FnMut() -> f32,
    mut in_world: impl FnMut(i32) -> bool,
    mut resistance_at: impl FnMut(BlockPos) -> Option<f32>,
) -> Blast {
    let mut to_blow = Vec::new();
    let mut cache = BlastCache::new(
        BlockPos::new(
            center.x.floor() as i32,
            center.y.floor() as i32,
            center.z.floor() as i32,
        ),
        radius,
    );
    let last = RAY_GRID_SIZE - 1;

    for x in 0..RAY_GRID_SIZE {
        for y in 0..RAY_GRID_SIZE {
            for z in 0..RAY_GRID_SIZE {
                // Only the shell of the grid produces rays; the interior would
                // duplicate directions already covered.
                if x != 0 && x != last && y != 0 && y != last && z != 0 && z != last {
                    continue;
                }

                let mut dx = f64::from(x) / f64::from(last) * 2.0 - 1.0;
                let mut dy = f64::from(y) / f64::from(last) * 2.0 - 1.0;
                let mut dz = f64::from(z) / f64::from(last) * 2.0 - 1.0;
                let length = dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt();
                if length == 0.0 {
                    continue;
                }
                dx /= length;
                dy /= length;
                dz /= length;

                // Vanilla varies each ray's power between 70% and 130%.
                let mut power = radius * 0.6f32.mul_add(roll(), 0.7);
                let mut position = center;

                while power > 0.0 {
                    // `Mth.floor`, as vanilla's `BlockPos.containing`; the libm
                    // call `f64::floor` compiles to costs a third of the step.
                    let pos = BlockPos::new(
                        fast_floor(position.x),
                        fast_floor(position.y),
                        fast_floor(position.z),
                    );
                    if !in_world(pos.y()) {
                        break;
                    }

                    let index = cache.dense_index(pos);
                    if let Some(resistance) = cache.resistance(pos, index, &mut resistance_at) {
                        power -= (resistance + 0.3) * RAY_STEP;
                    }

                    if power > 0.0 && cache.take(pos, index) {
                        to_blow.push(pos);
                    }

                    position.x += dx * f64::from(RAY_STEP);
                    position.y += dy * f64::from(RAY_STEP);
                    position.z += dz * f64::from(RAY_STEP);
                    power -= RAY_STEP_COST;
                }
            }
        }
    }

    Blast { to_blow, cache }
}

#[cfg(test)]
mod tests {
    use foton_utils::BlockPos;
    use glam::DVec3;
    use rustc_hash::FxHashMap;

    use super::{RAY_GRID_SIZE, RAY_STEP, RAY_STEP_COST, cast_rays};

    /// The rays as they were before the cache: one lookup per step and a linear search of the
    /// result for every position.
    fn reference(
        center: DVec3,
        radius: f32,
        mut roll: impl FnMut() -> f32,
        resistance_at: impl Fn(BlockPos) -> Option<f32>,
    ) -> Vec<BlockPos> {
        let mut to_blow = Vec::new();
        let last = RAY_GRID_SIZE - 1;
        for x in 0..RAY_GRID_SIZE {
            for y in 0..RAY_GRID_SIZE {
                for z in 0..RAY_GRID_SIZE {
                    if x != 0 && x != last && y != 0 && y != last && z != 0 && z != last {
                        continue;
                    }
                    let mut dx = f64::from(x) / f64::from(last) * 2.0 - 1.0;
                    let mut dy = f64::from(y) / f64::from(last) * 2.0 - 1.0;
                    let mut dz = f64::from(z) / f64::from(last) * 2.0 - 1.0;
                    let length = dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt();
                    if length == 0.0 {
                        continue;
                    }
                    dx /= length;
                    dy /= length;
                    dz /= length;
                    let mut power = radius * 0.6f32.mul_add(roll(), 0.7);
                    let mut position = center;
                    while power > 0.0 {
                        let pos = BlockPos::new(
                            position.x.floor() as i32,
                            position.y.floor() as i32,
                            position.z.floor() as i32,
                        );
                        if pos.y() < -64 || pos.y() >= 320 {
                            break;
                        }
                        if let Some(resistance) = resistance_at(pos) {
                            power -= (resistance + 0.3) * RAY_STEP;
                        }
                        if power > 0.0 && !to_blow.contains(&pos) {
                            to_blow.push(pos);
                        }
                        position.x += dx * f64::from(RAY_STEP);
                        position.y += dy * f64::from(RAY_STEP);
                        position.z += dz * f64::from(RAY_STEP);
                        power -= RAY_STEP_COST;
                    }
                }
            }
        }
        to_blow
    }

    /// Stone below y = 60, scattered soft blocks above it, air elsewhere.
    fn terrain(pos: BlockPos) -> Option<f32> {
        if pos.y() < 60 {
            Some(if (pos.x() + pos.z()) % 5 == 0 {
                9.0
            } else {
                6.0
            })
        } else if (pos.x() * 7 + pos.z() * 13 + pos.y()).rem_euclid(11) == 0 {
            Some(0.5)
        } else {
            None
        }
    }

    /// A fixed sequence of rolls in `0..1`, so both versions see the same random numbers.
    fn rolls() -> impl FnMut() -> f32 {
        let mut state: u32 = 0x9E37_79B9;
        move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 24) as f32
        }
    }

    #[test]
    fn the_cache_changes_nothing_about_which_blocks_break_or_in_what_order() {
        // 4 is a TNT; 10 still fits the dense cache; 40 does not and takes the hash maps.
        for radius in [0.5_f32, 4.0, 10.0, 40.0] {
            let center = DVec3::new(100.3, 66.06, -41.7);
            let expected = reference(center, radius, rolls(), terrain);
            let actual = cast_rays(
                center,
                radius,
                rolls(),
                |y| (-64..320).contains(&y),
                terrain,
            )
            .to_blow;
            assert_eq!(actual, expected, "radius {radius}");
            assert!(!actual.is_empty(), "radius {radius} should reach something");
        }
    }

    #[test]
    fn a_block_is_looked_up_once_however_many_rays_cross_it() {
        let mut calls: FxHashMap<BlockPos, u32> = FxHashMap::default();
        let center = DVec3::new(0.5, 66.0, 0.5);
        let broken = cast_rays(
            center,
            4.0,
            rolls(),
            |y| (-64..320).contains(&y),
            |pos| {
                *calls.entry(pos).or_default() += 1;
                terrain(pos)
            },
        )
        .to_blow;
        assert!(!broken.is_empty());
        assert!(
            calls.values().all(|&n| n == 1),
            "a block was looked up twice"
        );
        // The old loop made one lookup per step: thousands for a TNT. Distinct blocks are few.
        assert!(calls.len() < 3000, "{} distinct lookups", calls.len());
    }

    #[test]
    fn the_blast_remembers_which_reached_blocks_were_empty() {
        for radius in [4.0_f32, 40.0] {
            let blast = cast_rays(
                DVec3::new(0.5, 61.0, 0.5),
                radius,
                rolls(),
                |y| (-64..320).contains(&y),
                terrain,
            );
            for pos in &blast.to_blow {
                assert_eq!(blast.was_empty(*pos), terrain(*pos).is_none(), "{pos:?}");
            }
        }
    }

    #[test]
    fn no_ray_leaves_the_half_width() {
        for radius in [0.5_f32, 4.0, 10.0, 40.0] {
            let center = DVec3::new(7.99, 70.01, -0.01);
            let origin = BlockPos::new(7, 70, -1);
            let half = super::blast_half_width(radius);
            let blast = cast_rays(center, radius, || 1.0, |_| true, |_| None);
            for pos in &blast.to_blow {
                assert!(
                    (pos.x() - origin.x()).abs() <= half
                        && (pos.y() - origin.y()).abs() <= half
                        && (pos.z() - origin.z()).abs() <= half,
                    "radius {radius}: {pos:?} beyond {half}"
                );
            }
        }
    }
}
