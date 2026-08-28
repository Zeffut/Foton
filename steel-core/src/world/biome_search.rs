//! The nearest-biome search behind `/locate biome`.

use steel_registry::biome::BiomeRef;
use steel_utils::{BlockPos, Direction};

use crate::world::World;
use crate::worldgen::ChunkGenerator as _;

impl World {
    /// Finds the nearest sampled position whose noise biome `allowed` accepts.
    ///
    /// Vanilla parity: `ServerLevel.findClosestBiome3d` into
    /// `BiomeSource.findClosestBiome3d`. The scan spirals outwards in
    /// `sample_resolution_horizontal`-block steps and, inside each column, walks
    /// Y outwards from the origin in `sample_resolution_vertical`-block steps.
    /// The first hit wins, so this returns the nearest match in scan order --
    /// unlike `BiomeSource.findBiomeHorizontal`, which reservoir-samples one of
    /// all the matches in a fixed radius and is what stronghold rings want.
    #[must_use]
    pub(crate) fn find_closest_biome_3d(
        &self,
        origin: BlockPos,
        search_radius: i32,
        sample_resolution_horizontal: i32,
        sample_resolution_vertical: i32,
        allowed: &dyn Fn(BiomeRef) -> bool,
    ) -> Option<(BlockPos, BiomeRef)> {
        let generator = &self.chunk_map.world_gen_context.generator;
        // Vanilla filters `possibleBiomes()` first and gives up when nothing is
        // left. That is the difference between an instant "not found" and a full
        // scan of forty thousand columns for a biome this dimension cannot hold.
        if !generator.possible_biomes().into_iter().any(allowed) {
            return None;
        }

        let sample_ys = out_from_origin(
            origin.y(),
            self.get_min_y() + 1,
            self.get_max_y() + 1,
            sample_resolution_vertical,
        );
        let sample_radius = search_radius.div_euclid(sample_resolution_horizontal);

        let mut found = None;
        generator.with_noise_biomes(&mut |noise_biome| {
            for column in BlockPos::spiral_around(
                BlockPos::ZERO,
                sample_radius,
                Direction::East,
                Direction::South,
            ) {
                let block_x = origin.x() + column.x() * sample_resolution_horizontal;
                let block_z = origin.z() + column.z() * sample_resolution_horizontal;
                // QuartPos::from_block.
                let quart_x = block_x >> 2;
                let quart_z = block_z >> 2;

                for &block_y in &sample_ys {
                    let biome = noise_biome(quart_x, block_y >> 2, quart_z);
                    if allowed(biome) {
                        found = Some((BlockPos::new(block_x, block_y, block_z), biome));
                        return;
                    }
                }
            }
        });
        found
    }
}

/// The values in `[lower_bound, upper_bound]` reachable from `origin` in `step`
/// strides, nearest first and alternating below and above it.
///
/// Vanilla parity: `Mth.outFromOrigin`. The alternation is what makes a biome
/// search prefer a match near the player's own Y over one at the world ceiling.
fn out_from_origin(origin: i32, lower_bound: i32, upper_bound: i32, step: i32) -> Vec<i32> {
    debug_assert!(lower_bound <= upper_bound, "empty search range");
    debug_assert!(step >= 1, "a zero stride would never terminate");

    let clamped = origin.clamp(lower_bound, upper_bound);
    let mut values = Vec::new();
    let mut cursor = clamped;

    loop {
        let distance = (clamped - cursor).abs();
        if clamped - distance < lower_bound && clamped + distance > upper_bound {
            return values;
        }
        values.push(cursor);

        let previous_was_below = cursor <= clamped;
        let can_move_above = clamped + distance + step <= upper_bound;
        let below = (!previous_was_below || !can_move_above)
            .then(|| clamped - distance - if previous_was_below { step } else { 0 })
            .filter(|candidate| *candidate >= lower_bound);
        cursor = below.unwrap_or(clamped + distance + step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stride order is the whole point: the search has to reach the player's
    /// own Y first and then alternate outwards, so a biome just below them is
    /// not reported at the world ceiling instead.
    #[test]
    fn out_from_origin_alternates_outwards_from_the_clamped_origin() {
        assert_eq!(
            out_from_origin(64, -63, 321, 64),
            vec![64, 128, 0, 192, 256, 320]
        );
    }

    /// An origin outside the range is clamped into it rather than dropped, and
    /// a range that only extends one way still walks that way.
    #[test]
    fn out_from_origin_clamps_and_keeps_walking_one_sided_ranges() {
        assert_eq!(out_from_origin(1000, 0, 10, 4), vec![10, 6, 2]);
        assert_eq!(out_from_origin(-1000, 0, 10, 4), vec![0, 4, 8]);
    }
}
