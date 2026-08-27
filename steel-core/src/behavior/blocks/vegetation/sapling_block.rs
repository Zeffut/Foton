use std::sync::Arc;

use rand::{Rng, RngExt};
use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, IntProperty};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::block::BlockBehavior;
use crate::behavior::blocks::vegetation::bonemealable::Bonemealable;
use crate::behavior::context::BlockPlaceContext;
use crate::world::{LevelReader, World};

use super::tree_grower::TreeGrower;
use super::{BlockRef, default_surviving_state, survives_on_tag};

/// How grown a sapling is.
///
/// Vanilla parity: `SaplingBlock.STAGE`. A sapling advances once before it
/// grows, which is why a freshly planted one never turns into a tree on its
/// first tick.
const STAGE: &IntProperty = &BlockStateProperties::STAGE;

/// Light a sapling needs above it to advance.
///
/// Vanilla parity: the `getMaxLocalRawBrightness(pos.above()) >= 9` of
/// `SaplingBlock.randomTick`.
const MIN_LIGHT: u8 = 9;

/// One-in-this-many chance of advancing on a random tick that has the light.
///
/// Vanilla parity: the `random.nextInt(7) == 0`.
const ADVANCE_CHANCE: i32 = 7;

/// Chance bonemeal advances a sapling.
///
/// Vanilla parity: `SaplingBlock.isBonemealSuccess`.
const BONEMEAL_SUCCESS_CHANCE: f32 = 0.45;

/// Vanilla `SaplingBlock`.
///
/// The tree it grows comes from the extracted `tree_grower_name`; the table
/// that turns that name into a configured feature is [`TreeGrower`].
#[block_behavior]
pub struct SaplingBlock {
    block: BlockRef,
    #[json_arg(value, json = "tree_grower_name")]
    grower_name: &'static str,
}

impl SaplingBlock {
    /// Creates a new sapling block behavior.
    #[must_use]
    pub const fn new(block: BlockRef, grower_name: &'static str) -> Self {
        Self { block, grower_name }
    }

    /// Advances the sapling one step, growing a tree from the last one.
    ///
    /// Vanilla parity: `SaplingBlock.advanceTree`.
    pub(super) fn advance_tree(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        rng: &mut dyn Rng,
    ) {
        if state.get_value(STAGE) == 0 {
            let advanced = state.set_value(STAGE, 1);
            world.set_block(pos, advanced, UpdateFlags::UPDATE_INVISIBLE);
            return;
        }

        let Some(grower) = TreeGrower::by_name(self.grower_name) else {
            log::warn!(
                "{} names tree grower {}, which Steel does not know",
                self.block.key,
                self.grower_name
            );
            return;
        };
        grower.grow_tree(world, pos, state, rng);
    }
}

impl BlockBehavior for SaplingBlock {
    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        survives_on_tag(world, pos, &BlockTag::SUPPORTS_VEGETATION)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        default_surviving_state(self.block, self, context)
    }

    /// Vanilla parity: `SaplingBlock.randomTick`. A sapling in the dark simply
    /// waits, which is why an underground one never becomes a tree.
    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if world.max_local_raw_brightness(pos.above(), world.sky_darkening()) < MIN_LIGHT {
            return;
        }

        let mut rng = rand::rng();
        if rng.random_range(0..ADVANCE_CHANCE) != 0 {
            return;
        }

        self.advance_tree(state, world, pos, &mut rng);
    }

    /// Vanilla parity: the `instanceof BonemealableBlock` test every caller of
    /// bone meal opens with. `SaplingBlock` implements it below, but nothing
    /// said so here, so bone meal on a sapling did nothing at all -- neither in
    /// a hand nor out of a dispenser.
    fn as_bonemealable(&self) -> Option<&dyn Bonemealable> {
        Some(self)
    }
}

impl Bonemealable for SaplingBlock {
    fn is_valid_bonemeal_target(
        &self,
        _state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
    ) -> bool {
        true
    }

    fn is_bonemeal_success(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        rng: &mut dyn Rng,
        _pos: BlockPos,
    ) -> bool {
        rng.random::<f32>() < BONEMEAL_SUCCESS_CHANCE
    }

    fn perform_bonemeal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        rng: &mut dyn Rng,
        pos: BlockPos,
    ) {
        self.advance_tree(state, world, pos, rng);
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::{BLOCK_BEHAVIORS, init_behaviors};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// A patch of dirt in open air with a sapling standing on it.
    fn planted(name: &'static str) -> (Arc<World>, BlockPos) {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let pos = BlockPos::new(8, 100, 8);
        for dx in -2..=2 {
            for dz in -2..=2 {
                let _ = world.set_block(
                    pos.offset(dx, -1, dz),
                    vanilla_blocks::DIRT.default_state(),
                    UpdateFlags::UPDATE_CLIENTS,
                );
            }
        }
        let _ = world.set_block(
            pos,
            vanilla_blocks::OAK_SAPLING.default_state(),
            UpdateFlags::UPDATE_CLIENTS,
        );

        (world, pos)
    }

    fn sapling_behavior() -> &'static dyn BlockBehavior {
        BLOCK_BEHAVIORS.get_behavior(&vanilla_blocks::OAK_SAPLING)
    }

    /// The first advance only ages the sapling; it must not skip to a tree.
    #[test]
    fn the_first_advance_only_ages_the_sapling() {
        let (world, pos) = planted("sapling_first_advance");
        let sapling = SaplingBlock::new(&vanilla_blocks::OAK_SAPLING, "oak");
        let state = world.get_block_state(pos);
        assert_eq!(state.get_value(STAGE), 0);

        sapling.advance_tree(state, &world, pos, &mut rand::rng());

        let grown = world.get_block_state(pos);
        assert_eq!(grown.get_block(), &vanilla_blocks::OAK_SAPLING);
        assert_eq!(grown.get_value(STAGE), 1);
    }

    /// The second advance grows a real tree.
    ///
    /// This is the whole point: a tree is a worldgen feature, and until the
    /// feature was written against `LevelAccessor` there was no way to run one
    /// in a live world -- so a planted sapling stayed a sapling forever and a
    /// survival world had no renewable wood.
    #[test]
    fn the_second_advance_grows_a_tree() {
        let (world, pos) = planted("sapling_grows_a_tree");
        let sapling = SaplingBlock::new(&vanilla_blocks::OAK_SAPLING, "oak");

        let aged = world.get_block_state(pos).set_value(STAGE, 1);
        let _ = world.set_block(pos, aged, UpdateFlags::UPDATE_CLIENTS);

        // The trunk placer rolls a height, so a few attempts make the test
        // about whether a tree can grow rather than about one lucky roll.
        let mut grew = false;
        for _ in 0..20 {
            sapling.advance_tree(world.get_block_state(pos), &world, pos, &mut rand::rng());
            if world
                .get_block_state(pos)
                .get_block()
                .has_tag(&BlockTag::LOGS)
            {
                grew = true;
                break;
            }
            let aged = vanilla_blocks::OAK_SAPLING
                .default_state()
                .set_value(STAGE, 1);
            let _ = world.set_block(pos, aged, UpdateFlags::UPDATE_CLIENTS);
        }

        assert!(grew, "the sapling never became a log");

        // A trunk, not a single block: an oak is at least four tall.
        assert!(
            world
                .get_block_state(pos.above_n(2))
                .get_block()
                .has_tag(&BlockTag::LOGS),
            "no trunk above the stump"
        );
        // Leaves sit around the trunk, not only on top of it, and an oak may
        // come up as a fancy oak whose trunk is far taller than a plain one --
        // so the whole volume a tree could occupy is searched rather than the
        // column above the stump.
        let has_leaves = (-5..=5).any(|dx| {
            (1..=24).any(|dy| {
                (-5..=5).any(|dz| {
                    world
                        .get_block_state(pos.offset(dx, dy, dz))
                        .get_block()
                        .has_tag(&BlockTag::LEAVES)
                })
            })
        });
        assert!(has_leaves, "a tree with no leaves is not a tree");
    }

    /// A random tick on a lit sapling really does advance it.
    ///
    /// This is what connects the block to the growth: `random_tick` is called
    /// by the world, and if it were not wired to `advance_tree` nothing else
    /// here would notice.
    ///
    /// The matching darkness case is not tested. `fresh_test_world` reports a
    /// raw brightness of 15 whatever is placed above -- it runs no light
    /// engine -- so a test that stacked stone on the sapling would pass or fail
    /// for reasons that have nothing to do with the light check.
    #[test]
    fn a_random_tick_advances_a_lit_sapling() {
        let (world, pos) = planted("sapling_random_tick");
        let before = world.get_block_state(pos);
        assert_eq!(before.get_value(STAGE), 0);

        // The advance is one in seven, so this is about whether the wiring
        // exists rather than about a single roll.
        for _ in 0..200 {
            sapling_behavior().random_tick(world.get_block_state(pos), &world, pos);
            if world.get_block_state(pos).get_value(STAGE) == 1 {
                return;
            }
        }

        panic!("two hundred random ticks never advanced the sapling");
    }

    /// Bone meal has to reach the sapling at all.
    ///
    /// `SaplingBlock` implemented `Bonemealable` in full and its behavior
    /// answered `None` to `as_bonemealable`, which is the one question every
    /// caller of bone meal asks first. Nothing else in the tree would have
    /// noticed: the trait was written, tested, and unreachable.
    #[test]
    fn a_sapling_answers_that_it_takes_bone_meal() {
        let (world, pos) = planted("sapling_takes_bonemeal");
        let state = world.get_block_state(pos);

        let bonemealable = sapling_behavior()
            .as_bonemealable()
            .expect("a sapling is a bone meal target");

        assert!(bonemealable.is_valid_bonemeal_target(state, world.as_ref(), pos));

        // And it does something once it is reached: bone meal advances the
        // sapling exactly as a random tick would.
        let mut rng = rand::rng();
        bonemealable.perform_bonemeal(state, &world, &mut rng, pos);
        assert_eq!(world.get_block_state(pos).get_value(STAGE), 1);
    }

    /// A grower name Steel does not know leaves the sapling alone.
    ///
    /// Silently eating the sapling would be worse than not growing.
    #[test]
    fn an_unknown_grower_leaves_the_sapling_standing() {
        let (world, pos) = planted("sapling_unknown_grower");
        let sapling = SaplingBlock::new(&vanilla_blocks::OAK_SAPLING, "no_such_grower");

        let aged = world.get_block_state(pos).set_value(STAGE, 1);
        let _ = world.set_block(pos, aged, UpdateFlags::UPDATE_CLIENTS);
        sapling.advance_tree(aged, &world, pos, &mut rand::rng());

        assert_eq!(
            world.get_block_state(pos).get_block(),
            &vanilla_blocks::OAK_SAPLING
        );
    }
}
