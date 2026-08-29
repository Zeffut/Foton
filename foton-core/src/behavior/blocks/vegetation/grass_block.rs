use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::REGISTRY;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::{vanilla_blocks, vanilla_placed_features};
use foton_utils::random::worldgen_random::WorldgenRandom;
use foton_utils::{BlockPos, BlockStateId, Direction};
use rand::{Rng, RngExt as _};

use super::bonemealable::{BonemealAction, Bonemealable};
use super::snowy_block::{snowy_placement_state, update_snowy_shape};
use super::spreading_snowy_block;
use crate::behavior::BLOCK_BEHAVIORS;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::world::{LevelReader, ScheduledTickAccess, World};
use crate::worldgen::feature::FeatureDecorationRunner;

/// How many positions bone meal tries before giving up.
///
/// Vanilla parity: the outer loop of `GrassBlock.performBonemeal`.
const BONEMEAL_ATTEMPTS: i32 = 128;

/// Behavior for grass blocks.
#[block_behavior]
pub struct GrassBlock {
    block: BlockRef,
}

impl GrassBlock {
    /// Creates a new grass block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Walks the wandering offset vanilla uses to pick one bone meal target.
    ///
    /// Returns `None` when the walk leaves the patch of grass -- vanilla's
    /// `continue label48`, which abandons this attempt rather than planting
    /// somewhere the grass does not reach.
    fn wander(
        &self,
        world: &Arc<World>,
        start: BlockPos,
        steps: i32,
        rng: &mut dyn Rng,
    ) -> Option<BlockPos> {
        let mut pos = start;
        for _ in 0..steps {
            pos = pos.offset(
                rng.random_range(0..3) - 1,
                (rng.random_range(0..3) - 1) * rng.random_range(0..3) / 2,
                rng.random_range(0..3) - 1,
            );
            let state = world.get_block_state(pos);
            if world.get_block_state(pos.below()).get_block() != self.block
                || world.is_collision_shape_full_block_at(pos, state)
            {
                return None;
            }
        }
        Some(pos)
    }
}

impl BlockBehavior for GrassBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(snowy_placement_state(self.block, context))
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        _world: &dyn ScheduledTickAccess,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        update_snowy_shape(state, direction, neighbor_state)
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        spreading_snowy_block::random_tick(self.block, &vanilla_blocks::DIRT, state, world, pos);
    }

    fn as_bonemealable(&self) -> Option<&dyn Bonemealable> {
        Some(self)
    }
}

impl Bonemealable for GrassBlock {
    fn is_valid_bonemeal_target(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> bool {
        let above = pos.above();
        world.get_block_state(above).is_air() && !world.is_outside_build_height(above.y())
    }

    /// Vanilla parity: `GrassBlock.performBonemeal`.
    ///
    /// A hundred and twenty-eight attempts, each wandering a little further from
    /// the block than the last, so one handful thickens the patch it lands on
    /// rather than stacking everything on one square.
    fn perform_bonemeal(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        rng: &mut dyn Rng,
        pos: BlockPos,
    ) {
        let above = pos.above();
        let short_grass = &vanilla_blocks::SHORT_GRASS;

        for attempt in 0..BONEMEAL_ATTEMPTS {
            let Some(test_pos) = self.wander(world, above, attempt / 16, rng) else {
                continue;
            };

            let test_state = world.get_block_state(test_pos);
            // A tuft that is already there sometimes grows into tall grass
            // instead, which is how a bone-mealed field ends up uneven.
            if test_state.get_block() == short_grass && rng.random_range(0..10) == 0 {
                let behavior = BLOCK_BEHAVIORS.get_behavior(short_grass);
                if let Some(bonemealable) = behavior.as_bonemealable()
                    && bonemealable.is_valid_bonemeal_target(test_state, world.as_ref(), test_pos)
                {
                    bonemealable.perform_bonemeal(test_state, world, rng, test_pos);
                }
            }

            if !test_state.is_air() || world.is_outside_build_height(test_pos.y()) {
                continue;
            }

            // TODO: the one-in-eight branch is vanilla's flower roll. It draws
            // from `BiomeGenerationSettings.getBoneMealFeatures`, which is the
            // biome's placed features filtered by the
            // `minecraft:can_spawn_from_bone_meal` configured-feature tag --
            // SteelExtractor does not emit worldgen feature tags yet, so the
            // list is empty here and vanilla plants nothing for an empty list
            // either. Grass still comes out of the other seven.
            if rng.random_range(0..8) == 0 {
                continue;
            }

            let mut random = WorldgenRandom::from_seed(rng.random());
            FeatureDecorationRunner::place_placed_feature_data(
                world,
                &REGISTRY,
                &mut random,
                test_pos,
                &vanilla_placed_features::GRASS_BONEMEAL.data,
                None,
                world.biome_zoom_seed(),
            );
        }
    }

    fn bonemeal_action_type(&self) -> BonemealAction {
        BonemealAction::NeighborSpreader
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::blocks::block_state_ext::BlockStateExt;
    use foton_registry::blocks::properties::BlockStateProperties;
    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use foton_utils::{BlockPos, Direction};

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::TestLevel;

    #[test]
    fn grass_block_updates_snowy_state() {
        init_vanilla_registry();
        init_behaviors();

        let level = TestLevel::default();
        let pos = BlockPos::new(0, 64, 0);
        let behavior = GrassBlock::new(&vanilla_blocks::GRASS_BLOCK);

        let non_snowy = vanilla_blocks::GRASS_BLOCK.default_state();
        let snowy = behavior.update_shape(
            non_snowy,
            &level,
            pos,
            Direction::Up,
            pos.above(),
            vanilla_blocks::SNOW.default_state(),
        );
        assert!(snowy.get_value(&BlockStateProperties::SNOWY));

        let cleared = behavior.update_shape(
            snowy,
            &level,
            pos,
            Direction::Up,
            pos.above(),
            vanilla_blocks::AIR.default_state(),
        );
        assert!(!cleared.get_value(&BlockStateProperties::SNOWY));
    }
}
