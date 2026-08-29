//! Vanilla `SculkPatchFeature`, which carves the deep dark.
//!
//! The spread algorithm itself lives with the blocks that own it, in
//! `behavior::blocks::sculk::spreader`, because a sculk catalyst runs the same walk in a
//! live world. This file is only the feature wrapper: the rounds, the catalyst roll, and
//! the extra rare shriekers.

use super::super::prelude::*;
use super::super::runner::FeatureDecorationRunner;
use crate::behavior::blocks::{SculkBehaviorKind, SculkSpreader, sculk_behavior_of};
use foton_registry::vanilla_block_entity_types;

impl FeatureDecorationRunner {
    pub(in crate::worldgen::feature) fn place_sculk_patch_feature(
        region: &impl WorldGenLevel,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &SculkPatchConfiguration,
        origin: BlockPos,
    ) -> bool {
        if !Self::can_sculk_spread_from(region, origin) {
            return false;
        }

        let mut spreader = SculkSpreader::worldgen();
        let total_rounds = config.spread_rounds + config.growth_rounds;
        for round in 0..total_rounds {
            for _ in 0..config.charge_count {
                spreader.add_cursors(origin, config.amount_per_charge);
            }

            let spread_veins = round < config.spread_rounds;
            for _ in 0..config.spread_attempts {
                spreader.update_cursors(region, registry, origin, random, spread_veins);
            }

            spreader.clear();
        }

        let below = origin.below();
        let below_state = region.get_block_state(below);
        if random.next_f32() <= config.catalyst_chance
            && shapes::is_offset_shape_full_block(below_state.get_collision_shape_at(below))
        {
            let catalyst = vanilla_blocks::SCULK_CATALYST.default_state();
            if region.set_block_state(origin, catalyst, UpdateFlags::UPDATE_ALL) {
                Self::set_empty_block_entity(
                    region,
                    origin,
                    &vanilla_block_entity_types::SCULK_CATALYST,
                    catalyst,
                );
            }
        }

        let extra_growths = config.extra_rare_growths.sample(random);
        for _ in 0..extra_growths {
            let candidate = origin.offset(
                random.next_i32_bounded(5) - 2,
                0,
                random.next_i32_bounded(5) - 2,
            );
            let below = candidate.below();
            if !region.get_block_state(candidate).is_air()
                || !region
                    .get_block_state(below)
                    .is_face_sturdy_at(below, Direction::Up)
            {
                continue;
            }

            let shrieker = vanilla_blocks::SCULK_SHRIEKER
                .default_state()
                .set_value(&BlockStateProperties::CAN_SUMMON, true);
            if region.set_block_state(candidate, shrieker, UpdateFlags::UPDATE_ALL) {
                Self::set_empty_block_entity(
                    region,
                    candidate,
                    &vanilla_block_entity_types::SCULK_SHRIEKER,
                    shrieker,
                );
            }
        }

        true
    }

    /// Vanilla `SculkPatchFeature.canSpreadFrom`.
    fn can_sculk_spread_from(region: &impl WorldGenLevel, origin: BlockPos) -> bool {
        let start = region.get_block_state(origin);
        if sculk_behavior_of(start) != SculkBehaviorKind::Default {
            return true;
        }

        if !start.is_air()
            && (start.get_block() != &vanilla_blocks::WATER
                || !get_fluid_state_from_block(start).is_source())
        {
            return false;
        }

        Self::VANILLA_DIRECTION_VALUES.iter().any(|direction| {
            let pos = origin.relative(*direction);
            let state = region.get_block_state(pos);
            shapes::is_offset_shape_full_block(state.get_collision_shape_at(pos))
        })
    }
}
