use foton_registry::vanilla_block_tags::BlockTag;

use super::super::prelude::*;
use super::super::runner::FeatureDecorationRunner;

impl FeatureDecorationRunner {
    /// Takes any `LevelAccessor` rather than only a worldgen region: bone meal
    /// on nylium places this feature in a live world.
    pub(crate) fn place_nether_forest_vegetation_feature(
        level: &impl LevelAccessor,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &NetherForestVegetationConfiguration,
        origin: BlockPos,
    ) -> bool {
        let below_state = level.get_block_state(origin.below());
        if !below_state.get_block().has_tag(&BlockTag::NYLIUM) {
            return false;
        }

        if origin.y() < level.min_y() + 1 || origin.y() + 1 > level.max_y_exclusive() - 1 {
            return false;
        }

        let mut placed = 0;
        for _ in 0..config.spread_width * config.spread_width {
            let final_pos = origin.offset(
                random.next_i32_bounded(config.spread_width)
                    - random.next_i32_bounded(config.spread_width),
                random.next_i32_bounded(config.spread_height)
                    - random.next_i32_bounded(config.spread_height),
                random.next_i32_bounded(config.spread_width)
                    - random.next_i32_bounded(config.spread_width),
            );
            let state = Self::sample_block_state_provider(
                level,
                registry,
                random,
                &config.state_provider,
                final_pos,
            );
            let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());
            if level.get_block_state(final_pos).is_air()
                && final_pos.y() > level.min_y()
                && behavior.can_survive(state, level, final_pos)
            {
                let _ = level.set_block_state(final_pos, state, UpdateFlags::UPDATE_CLIENTS);
                placed += 1;
            }
        }

        placed > 0
    }
}
