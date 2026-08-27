//! Vanilla `SpreadingSnowyBlock`: the random tick grass and mycelium share.
//!
//! Two rules, both on the block above. Cover one and it goes back to dirt;
//! leave it open and it creeps onto the dirt around it. Vanilla stopped asking
//! about the light level here -- 26.2 asks whether the block above dampens all
//! fifteen levels of light, which is why grass survives an unlit cave but not a
//! slab laid on top of it.

use std::sync::Arc;

use rand::RngExt as _;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_blocks;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use super::snowy_block::is_snowy_setting;
use crate::chunk::light::get_light_block_into;
use crate::world::{LevelReader, World};

/// Vanilla parity: `LightEngine.LIGHT_BLOCKED`, the dampening a fully occluding
/// face reports. Grass needs anything less than that above it.
const FULLY_DAMPENED: u8 = 15;

/// Light the block above needs for grass to spread onto a neighbour.
///
/// Vanilla parity: the `getMaxLocalRawBrightness(pos.above()) >= 9` of
/// `SpreadingSnowyBlock.randomTick`.
const MIN_SPREAD_LIGHT: u8 = 9;

/// Vanilla parity: `SpreadingSnowyBlock.canStayAlive`.
fn can_stay_alive(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
    let above = pos.above();
    let above_state = world.get_block_state(above);

    // A single layer of snow is the one cover grass lives under.
    if above_state.get_block() == &vanilla_blocks::SNOW
        && above_state.get_value(&BlockStateProperties::LAYERS) == 1
    {
        return true;
    }

    if above_state.get_fluid_state().is_full() {
        return false;
    }

    get_light_block_into(
        state,
        above_state,
        Direction::Up,
        above_state.get_light_dampening(),
    ) < FULLY_DAMPENED
}

/// Vanilla parity: `SpreadingSnowyBlock.canPropagate`. Grass will not climb into
/// a block that has water sitting on it, even though it would survive there.
fn can_propagate(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
    can_stay_alive(state, world, pos)
        && !world
            .get_block_state(pos.above())
            .get_fluid_state()
            .fluid_id
            .has_tag(&FluidTag::WATER)
}

/// Vanilla parity: `SpreadingSnowyBlock.randomTick`.
///
/// `base_block` is what this block falls back to when it is smothered --
/// `BlockItemIds.DIRT` for both grass and mycelium.
pub(super) fn random_tick(
    block: BlockRef,
    base_block: BlockRef,
    state: BlockStateId,
    world: &Arc<World>,
    pos: BlockPos,
) {
    if !can_stay_alive(state, world.as_ref(), pos) {
        world.set_block(pos, base_block.default_state(), UpdateFlags::UPDATE_ALL);
        return;
    }

    if world.max_local_raw_brightness(pos.above(), world.sky_darkening()) < MIN_SPREAD_LIGHT {
        return;
    }

    let spread_state = block.default_state();
    let mut rng = rand::rng();
    // Four tries at a neighbour, biased downwards: vanilla's y offset spans
    // -3..=1, so grass runs down a slope faster than it climbs one.
    for _ in 0..4 {
        let test_pos = pos.offset(
            rng.random_range(0..3) - 1,
            rng.random_range(0..5) - 3,
            rng.random_range(0..3) - 1,
        );
        if world.get_block_state(test_pos).get_block() == base_block
            && can_propagate(spread_state, world.as_ref(), test_pos)
        {
            let snowy = is_snowy_setting(world.get_block_state(test_pos.above()));
            world.set_block(
                test_pos,
                spread_state.set_value(&BlockStateProperties::SNOWY, snowy),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::blocks::block_state_ext::BlockStateExt as _;
    use steel_registry::blocks::properties::BlockStateProperties;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::BlockPos;

    use super::{can_propagate, can_stay_alive};
    use crate::test_support::TestLevel;

    const POS: BlockPos = BlockPos::new(0, 64, 0);

    fn covered_by(state: steel_utils::BlockStateId) -> TestLevel {
        TestLevel::default()
            .with_min_y(0)
            .with_block(POS.above(), state)
    }

    /// The rule 26.2 replaced the old light check with: what matters is whether
    /// the block on top blocks light outright, not how bright it is.
    #[test]
    fn a_solid_lid_smothers_grass_but_glass_does_not() {
        init_vanilla_registry();

        let grass = vanilla_blocks::GRASS_BLOCK.default_state();

        assert!(can_stay_alive(
            grass,
            &TestLevel::default().with_min_y(0),
            POS
        ));
        assert!(!can_stay_alive(
            grass,
            &covered_by(vanilla_blocks::STONE.default_state()),
            POS
        ));
        assert!(can_stay_alive(
            grass,
            &covered_by(vanilla_blocks::GLASS.default_state()),
            POS
        ));
    }

    /// One layer of snow is a lid grass lives under; more is not.
    #[test]
    fn one_layer_of_snow_is_survivable_and_a_full_block_is_not() {
        init_vanilla_registry();

        let grass = vanilla_blocks::GRASS_BLOCK.default_state();

        assert!(can_stay_alive(
            grass,
            &covered_by(vanilla_blocks::SNOW.default_state()),
            POS
        ));
        assert!(!can_stay_alive(
            grass,
            &covered_by(vanilla_blocks::SNOW_BLOCK.default_state()),
            POS
        ));
    }

    /// Standing water smothers grass outright. Water that is only running over
    /// it does not -- but grass still will not climb into it, which is the whole
    /// reason `canPropagate` asks a second question `canStayAlive` does not.
    #[test]
    fn deep_water_kills_and_shallow_water_only_blocks_spreading() {
        init_vanilla_registry();

        let grass = vanilla_blocks::GRASS_BLOCK.default_state();

        let under_a_source = covered_by(vanilla_blocks::WATER.default_state());
        assert!(!can_stay_alive(grass, &under_a_source, POS));

        let flowing = vanilla_blocks::WATER
            .default_state()
            .set_value(&BlockStateProperties::LEVEL, 1);
        let under_a_trickle = covered_by(flowing);
        assert!(can_stay_alive(grass, &under_a_trickle, POS));
        assert!(!can_propagate(grass, &under_a_trickle, POS));
    }
}
