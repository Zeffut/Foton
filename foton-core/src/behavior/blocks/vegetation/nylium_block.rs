//! Vanilla `NyliumBlock` behavior.

use std::sync::{Arc, LazyLock};

use foton_macros::block_behavior;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::Direction;
use foton_registry::feature::{ConfiguredFeature, ConfiguredFeatureKind};
use foton_registry::{REGISTRY, vanilla_blocks, vanilla_configured_features};
use foton_utils::random::worldgen_random::WorldgenRandom;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId};
use rand::{Rng, RngExt as _};

use super::BlockRef;
use super::bonemealable::{BonemealAction, Bonemealable};
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::chunk::light::{MAX_LIGHT_LEVEL, get_light_block_into};
use crate::world::{LevelReader, World};
use crate::worldgen::feature::FeatureDecorationRunner;

/// Vanilla `NyliumBlock`: crimson and warped nylium.
///
/// Nylium dies back to netherrack once something above it blocks all light, and
/// bone meal on it grows the matching nether forest vegetation.
#[block_behavior]
pub struct NyliumBlock {
    block: BlockRef,
}

/// Vanilla `random.nextInt(8) == 0`: the chance warped nylium also grows a
/// twisting vine.
const TWISTING_VINES_CHANCE: i32 = 8;

impl NyliumBlock {
    /// Creates a nylium behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `NyliumBlock.canBeNylium`.
    fn can_be_nylium(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = world.get_block_state(pos.above());
        let dampening_through_top_face = get_light_block_into(
            state,
            above_state,
            Direction::Up,
            above_state.get_light_dampening(),
        );

        dampening_through_top_face < MAX_LIGHT_LEVEL
    }

    /// Vanilla `NyliumBlock.place`: one configured feature, skipped when the
    /// target is out of the world.
    fn place(
        world: &Arc<World>,
        random: &mut WorldgenRandom,
        pos: BlockPos,
        feature: &LazyLock<ConfiguredFeature>,
    ) {
        if world.is_outside_build_height(pos.y()) {
            return;
        }

        match &feature.kind {
            ConfiguredFeatureKind::NetherForestVegetation(config) => {
                FeatureDecorationRunner::place_nether_forest_vegetation_feature(
                    world, &REGISTRY, random, config, pos,
                );
            }
            ConfiguredFeatureKind::TwistingVines(config) => {
                FeatureDecorationRunner::place_twisting_vines_feature(world, random, config, pos);
            }
            _ => log::warn!(
                "nylium bone meal names {}, which is not a nether vegetation feature",
                feature.key
            ),
        }
    }
}

impl BlockBehavior for NyliumBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !Self::can_be_nylium(state, world.as_ref(), pos) {
            world.set_block(
                pos,
                vanilla_blocks::NETHERRACK.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }

    fn as_bonemealable(&self) -> Option<&dyn Bonemealable> {
        Some(self)
    }
}

impl Bonemealable for NyliumBlock {
    fn is_valid_bonemeal_target(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> bool {
        let above = pos.above();
        world.get_block_state(above).is_air() && !world.is_outside_build_height(above.y())
    }

    fn perform_bonemeal(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        rng: &mut dyn Rng,
        pos: BlockPos,
    ) {
        let above = pos.above();
        let mut random = WorldgenRandom::from_seed(rng.random());

        if self.block == &vanilla_blocks::CRIMSON_NYLIUM {
            Self::place(
                world,
                &mut random,
                above,
                &vanilla_configured_features::CRIMSON_FOREST_VEGETATION_BONEMEAL,
            );
            return;
        }

        if self.block != &vanilla_blocks::WARPED_NYLIUM {
            return;
        }

        Self::place(
            world,
            &mut random,
            above,
            &vanilla_configured_features::WARPED_FOREST_VEGETATION_BONEMEAL,
        );
        Self::place(
            world,
            &mut random,
            above,
            &vanilla_configured_features::NETHER_SPROUTS_BONEMEAL,
        );
        if rng.random_range(0..TWISTING_VINES_CHANCE) == 0 {
            Self::place(
                world,
                &mut random,
                above,
                &vanilla_configured_features::TWISTING_VINES_BONEMEAL,
            );
        }
    }

    fn bonemeal_action_type(&self) -> BonemealAction {
        BonemealAction::NeighborSpreader
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::init_vanilla_registry;
    use foton_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::{TestLevel, fresh_test_world, insert_ready_full_chunk};

    #[test]
    fn nylium_survives_under_air_and_dies_under_a_light_blocking_neighbour() {
        init_vanilla_registry();

        let pos = BlockPos::new(0, 64, 0);
        let state = vanilla_blocks::CRIMSON_NYLIUM.default_state();

        let open = TestLevel::default();
        assert!(NyliumBlock::can_be_nylium(state, &open, pos));

        // Glass dampens nothing, so nylium lives under it even though the face
        // above is covered.
        let glazed =
            TestLevel::default().with_block(pos.above(), vanilla_blocks::GLASS.default_state());
        assert!(NyliumBlock::can_be_nylium(state, &glazed, pos));

        let buried = TestLevel::default()
            .with_block(pos.above(), vanilla_blocks::NETHERRACK.default_state());
        assert!(!NyliumBlock::can_be_nylium(state, &buried, pos));
    }

    #[test]
    fn a_buried_nylium_random_tick_turns_it_back_into_netherrack() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("nylium_dies_back");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let behavior = NyliumBlock::new(&vanilla_blocks::WARPED_NYLIUM);
        let state = vanilla_blocks::WARPED_NYLIUM.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));

        behavior.random_tick(state, &world, pos);
        assert_eq!(world.get_block_state(pos), state);

        assert!(world.set_block(
            pos.above(),
            vanilla_blocks::NETHERRACK.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        behavior.random_tick(state, &world, pos);
        assert_eq!(
            world.get_block_state(pos).get_block(),
            &vanilla_blocks::NETHERRACK
        );
    }

    #[test]
    fn nylium_only_accepts_bonemeal_with_air_above() {
        init_vanilla_registry();

        let pos = BlockPos::new(0, 64, 0);
        let behavior = NyliumBlock::new(&vanilla_blocks::CRIMSON_NYLIUM);
        let state = vanilla_blocks::CRIMSON_NYLIUM.default_state();

        assert!(behavior.is_valid_bonemeal_target(state, &TestLevel::default(), pos));

        let covered = TestLevel::default()
            .with_block(pos.above(), vanilla_blocks::NETHERRACK.default_state());
        assert!(!behavior.is_valid_bonemeal_target(state, &covered, pos));
    }

    #[test]
    fn bonemeal_on_warped_nylium_grows_nether_vegetation_above_it() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("nylium_bonemeal");
        let center = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(center));

        // The feature scatters over a five by five patch, so the whole patch is
        // nylium and every landing spot is a valid one.
        let state = vanilla_blocks::WARPED_NYLIUM.default_state();
        for dx in -2..=2 {
            for dz in -2..=2 {
                world.set_block(center.offset(dx, 0, dz), state, UpdateFlags::UPDATE_NONE);
            }
        }

        let behavior = NyliumBlock::new(&vanilla_blocks::WARPED_NYLIUM);
        behavior.perform_bonemeal(state, &world, &mut rand::rng(), center);

        let grown = (-2..=2)
            .flat_map(|dx| (-2..=2).map(move |dz| (dx, dz)))
            .filter(|(dx, dz)| !world.get_block_state(center.offset(*dx, 1, *dz)).is_air())
            .count();
        assert!(grown > 0, "bone meal should grow something on the nylium");
    }
}
