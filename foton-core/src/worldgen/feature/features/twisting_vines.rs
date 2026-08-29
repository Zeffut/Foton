use super::super::prelude::*;
use super::super::runner::FeatureDecorationRunner;

impl FeatureDecorationRunner {
    /// Takes any `LevelAccessor` rather than only a worldgen region: bone meal
    /// on warped nylium places this feature in a live world.
    pub(crate) fn place_twisting_vines_feature(
        level: &impl LevelAccessor,
        random: &mut WorldgenRandom,
        config: &TwistingVinesConfiguration,
        origin: BlockPos,
    ) -> bool {
        if Self::twisting_vines_invalid_placement_location(level, origin) {
            return false;
        }

        for _ in 0..config.spread_width * config.spread_width {
            let mut place_pos = origin.offset(
                random.next_i32_between(-config.spread_width, config.spread_width),
                random.next_i32_between(-config.spread_height, config.spread_height),
                random.next_i32_between(-config.spread_width, config.spread_width),
            );

            if Self::find_first_air_block_above_ground(level, &mut place_pos)
                && !Self::twisting_vines_invalid_placement_location(level, place_pos)
            {
                let mut vine_height = random.next_i32_between(1, config.max_height);
                if random.next_i32_bounded(6) == 0 {
                    vine_height *= 2;
                }

                if random.next_i32_bounded(5) == 0 {
                    vine_height = 1;
                }

                Self::place_twisting_vines_column(level, random, place_pos, vine_height, 17, 25);
            }
        }

        true
    }

    fn find_first_air_block_above_ground(
        level: &impl LevelReader,
        place_pos: &mut BlockPos,
    ) -> bool {
        loop {
            *place_pos = place_pos.below();
            if level.is_outside_build_height(place_pos.y()) {
                return false;
            }

            if !level.get_block_state(*place_pos).is_air() {
                *place_pos = place_pos.above();
                return true;
            }
        }
    }

    fn place_twisting_vines_column(
        level: &impl LevelAccessor,
        random: &mut WorldgenRandom,
        mut place_pos: BlockPos,
        total_height: i32,
        min_age: u8,
        max_age: u8,
    ) {
        for height in 1..=total_height {
            if level.get_block_state(place_pos).is_air() {
                if height == total_height || !level.get_block_state(place_pos.above()).is_air() {
                    let state = vanilla_blocks::TWISTING_VINES.default_state().set_value(
                        &BlockStateProperties::AGE_25,
                        random.next_i32_between(i32::from(min_age), i32::from(max_age)) as u8,
                    );
                    let _ = level.set_block_state(place_pos, state, UpdateFlags::UPDATE_CLIENTS);
                    break;
                }

                let _ = level.set_block_state(
                    place_pos,
                    vanilla_blocks::TWISTING_VINES_PLANT.default_state(),
                    UpdateFlags::UPDATE_CLIENTS,
                );
            }

            place_pos = place_pos.above();
        }
    }

    fn twisting_vines_invalid_placement_location(level: &impl LevelReader, pos: BlockPos) -> bool {
        if !level.get_block_state(pos).is_air() {
            return true;
        }

        let below_block = level.get_block_state(pos.below()).get_block();
        below_block != &vanilla_blocks::NETHERRACK
            && below_block != &vanilla_blocks::WARPED_NYLIUM
            && below_block != &vanilla_blocks::WARPED_WART_BLOCK
    }
}
