use super::super::prelude::*;
use super::super::runner::FeatureDecorationRunner;

impl FeatureDecorationRunner {
    pub(in crate::worldgen::feature) fn place_end_platform_feature(
        region: &impl WorldGenLevel,
        origin: BlockPos,
    ) -> bool {
        Self::create_end_platform(region, origin);
        true
    }

    pub(in crate::worldgen::feature) fn create_end_platform(
        region: &impl WorldGenLevel,
        origin: BlockPos,
    ) {
        let obsidian = vanilla_blocks::OBSIDIAN.default_state();
        let air = vanilla_blocks::AIR.default_state();

        for dz in -2..=2 {
            for dx in -2..=2 {
                for dy in -1..3 {
                    let pos = origin.offset(dx, dy, dz);
                    let state = if dy == -1 { obsidian } else { air };
                    if region.get_block_state(pos).get_block() != state.get_block() {
                        let _ = region.set_block_state(pos, state, UpdateFlags::UPDATE_ALL);
                    }
                }
            }
        }
    }
}
