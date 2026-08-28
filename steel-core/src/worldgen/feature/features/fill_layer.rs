use steel_registry::feature::LayerConfiguration;

use super::super::prelude::*;
use super::super::runner::FeatureDecorationRunner;

impl FeatureDecorationRunner {
    /// Fills one horizontal layer of the chunk wherever it is still air.
    ///
    /// Vanilla parity: `FillLayerFeature`. Only the flat generator reaches it,
    /// for a layer that does not block motion: those are pulled out of the
    /// layer stack and placed here instead, after every other feature, so a
    /// decoration that wants the space keeps it.
    pub(in crate::worldgen::feature) fn place_fill_layer_feature(
        region: &impl WorldGenLevel,
        origin: BlockPos,
        config: LayerConfiguration,
    ) -> bool {
        let y = region.min_y() + config.height;

        for dx in 0..16 {
            for dz in 0..16 {
                let pos = BlockPos::new(origin.x() + dx, y, origin.z() + dz);
                if region.get_block_state(pos).is_air() {
                    let _ = region.set_block_state(pos, config.state, UpdateFlags::UPDATE_CLIENTS);
                }
            }
        }

        true
    }
}
