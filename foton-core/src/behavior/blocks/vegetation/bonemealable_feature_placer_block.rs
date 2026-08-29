//! Vanilla `BonemealableFeaturePlacerBlock` behavior.

use std::sync::{Arc, LazyLock};

use foton_macros::block_behavior;
use foton_registry::REGISTRY;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::feature::ConfiguredFeature;
use foton_utils::random::worldgen_random::WorldgenRandom;
use foton_utils::{BlockPos, BlockStateId};
use rand::{Rng, RngExt as _};

use super::BlockRef;
use super::bonemealable::{BonemealAction, Bonemealable};
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::world::{LevelReader, World};
use crate::worldgen::feature::FeatureDecorationRunner;

/// Vanilla `BonemealableFeaturePlacerBlock`: the moss block and the pale moss block.
///
/// Bone meal on one of these does not grow the block itself; it places a whole
/// configured feature in the air above it -- a moss patch with its vegetation --
/// which is why the block needs the feature dispatcher to run against a live world.
#[block_behavior]
pub struct BonemealableFeaturePlacerBlock {
    block: BlockRef,
    #[json_arg(vanilla_configured_features, json = "feature")]
    feature: &'static LazyLock<ConfiguredFeature>,
}

impl BonemealableFeaturePlacerBlock {
    /// Creates a feature-placing bonemealable block behavior.
    #[must_use]
    pub const fn new(block: BlockRef, feature: &'static LazyLock<ConfiguredFeature>) -> Self {
        Self { block, feature }
    }
}

impl BlockBehavior for BonemealableFeaturePlacerBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn as_bonemealable(&self) -> Option<&dyn Bonemealable> {
        Some(self)
    }
}

impl Bonemealable for BonemealableFeaturePlacerBlock {
    fn is_valid_bonemeal_target(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> bool {
        world.get_block_state(pos.above()).is_air()
    }

    fn perform_bonemeal(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        rng: &mut dyn Rng,
        pos: BlockPos,
    ) {
        let mut random = WorldgenRandom::from_seed(rng.random());
        FeatureDecorationRunner::place_configured_feature_kind(
            world,
            &REGISTRY,
            &mut random,
            &self.feature.kind,
            pos.above(),
            world.biome_zoom_seed(),
        );
    }

    fn bonemeal_action_type(&self) -> BonemealAction {
        BonemealAction::NeighborSpreader
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_configured_features};
    use foton_utils::ChunkPos;
    use foton_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::{TestLevel, fresh_test_world, insert_ready_full_chunk};

    fn moss_behavior() -> BonemealableFeaturePlacerBlock {
        BonemealableFeaturePlacerBlock::new(
            &vanilla_blocks::MOSS_BLOCK,
            &vanilla_configured_features::MOSS_PATCH_BONEMEAL,
        )
    }

    #[test]
    fn moss_only_accepts_bonemeal_with_air_above() {
        init_vanilla_registry();

        let pos = BlockPos::new(0, 64, 0);
        let behavior = moss_behavior();
        let state = vanilla_blocks::MOSS_BLOCK.default_state();

        assert!(behavior.is_valid_bonemeal_target(state, &TestLevel::default(), pos));

        let covered =
            TestLevel::default().with_block(pos.above(), vanilla_blocks::STONE.default_state());
        assert!(!behavior.is_valid_bonemeal_target(state, &covered, pos));
    }

    #[test]
    fn bonemeal_on_a_moss_block_places_the_moss_patch_feature() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("moss_block_bonemeal");
        let center = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(center));

        // The patch replaces `moss_replaceable` ground -- stone is, through
        // `base_stone_overworld` -- out to an xz radius of at most two.
        let stone = vanilla_blocks::STONE.default_state();
        for dx in -4..=4 {
            for dz in -4..=4 {
                assert!(world.set_block(center.offset(dx, 0, dz), stone, UpdateFlags::UPDATE_NONE));
            }
        }
        let state = vanilla_blocks::MOSS_BLOCK.default_state();
        assert!(world.set_block(center, state, UpdateFlags::UPDATE_NONE));

        moss_behavior().perform_bonemeal(state, &world, &mut rand::rng(), center);

        let moss = (-4..=4)
            .flat_map(|dx| (-4..=4).map(move |dz| (dx, dz)))
            .filter(|(dx, dz)| {
                world
                    .get_block_state(center.offset(*dx, 0, *dz))
                    .get_block()
                    == &vanilla_blocks::MOSS_BLOCK
            })
            .count();
        assert!(
            moss > 1,
            "bone meal should turn the stone around the moss block into a moss patch"
        );
    }
}
