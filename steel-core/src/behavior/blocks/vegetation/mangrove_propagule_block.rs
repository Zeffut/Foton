use std::sync::Arc;

use rand::{Rng, RngExt};
use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, IntProperty,
};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::block::{BlockBehavior, schedule_water_tick_if_waterlogged};
use crate::behavior::blocks::vegetation::bonemealable::Bonemealable;
use crate::behavior::context::BlockPlaceContext;
use crate::world::{LevelReader, ScheduledTickAccess, World};

use super::sapling_block::SaplingBlock;
use super::{BlockRef, default_surviving_state};

/// Vanilla parity: `MangrovePropaguleBlock`.
///
/// One block leads two lives. Hanging under mangrove leaves it is fruit, and
/// its `age` is how ripe it is; planted in the ground it is a sapling, and
/// everything about growing a tree comes from [`SaplingBlock`], which vanilla
/// reaches by inheritance and Steel by holding one.
///
/// - Hanging: block above must be in `SUPPORTS_HANGING_MANGROVE_PROPAGULE`.
/// - Planted: block below must be in `SUPPORTS_MANGROVE_PROPAGULE` (vanilla's
///   `mayPlaceOn` override applied to the `VegetationBlock` survival rule).
#[block_behavior]
pub struct MangrovePropaguleBlock {
    block: BlockRef,
    /// The extracted grower name, which is only ever handed straight to the
    /// sapling below.
    #[json_arg(value, json = "tree_grower_name")]
    _grower_name: &'static str,
    /// The half of this block that is a sapling.
    ///
    /// Vanilla parity: the `extends SaplingBlock` the class opens with. Its
    /// `advanceTree` and its bone meal roll are what a planted propagule uses
    /// unchanged.
    sapling: SaplingBlock,
}

const AGE: &IntProperty = &BlockStateProperties::AGE_4;
const HANGING: &BoolProperty = &BlockStateProperties::HANGING;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Vanilla parity: `MangrovePropaguleBlock.MAX_AGE`, which is also the age a
/// propagule is placed at by hand.
const MAX_AGE: u8 = 4;

/// One-in-this-many chance a planted propagule advances on a random tick.
///
/// Vanilla parity: the `random.nextInt(7) == 0` of
/// `MangrovePropaguleBlock.randomTick`. Note what is missing beside it: the
/// propagule replaces `SaplingBlock.randomTick` outright and never asks about
/// the light, which is why one grows in the shade of the swamp it comes from.
const ADVANCE_CHANCE: i32 = 7;

/// Vanilla parity: `MangrovePropaguleBlock.isHanging`.
fn is_hanging(state: BlockStateId) -> bool {
    state.get_value(HANGING)
}

/// Vanilla parity: `MangrovePropaguleBlock.isFullyGrown`.
fn is_fully_grown(state: BlockStateId) -> bool {
    state.get_value(AGE) == MAX_AGE
}

/// Vanilla parity: the `state.cycle(AGE)` of a ripening propagule, which is
/// only ever reached below the maximum, so it only ever counts up.
fn ripened(state: BlockStateId) -> BlockStateId {
    state.set_value(AGE, state.get_value(AGE) + 1)
}

impl MangrovePropaguleBlock {
    /// Creates a new mangrove propagule block behavior.
    #[must_use]
    pub const fn new(block: BlockRef, grower_name: &'static str) -> Self {
        Self {
            block,
            _grower_name: grower_name,
            sapling: SaplingBlock::new(block, grower_name),
        }
    }

    /// Creates vanilla's initial hanging propagule state.
    pub(crate) fn create_new_hanging_propagule() -> BlockStateId {
        vanilla_blocks::MANGROVE_PROPAGULE
            .default_state()
            .set_value(HANGING, true)
            .set_value(AGE, 0)
    }
}

impl BlockBehavior for MangrovePropaguleBlock {
    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);
        if self.can_survive(state, world, pos) {
            state
        } else {
            vanilla_blocks::AIR.default_state()
        }
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        if is_hanging(state) {
            let above = world.get_block_state(pos.above());
            return above
                .get_block()
                .has_tag(&BlockTag::SUPPORTS_HANGING_MANGROVE_PROPAGULE);
        }

        let below = world.get_block_state(pos.below());
        below
            .get_block()
            .has_tag(&BlockTag::SUPPORTS_MANGROVE_PROPAGULE)
    }

    /// Vanilla parity: `MangrovePropaguleBlock.getStateForPlacement`. A
    /// propagule set down by hand is already ripe, whatever age it had while it
    /// was hanging.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            default_surviving_state(self.block, self, context)?
                .set_value(WATERLOGGED, context.is_water_source())
                .set_value(AGE, MAX_AGE),
        )
    }

    /// Vanilla parity: `MangrovePropaguleBlock.randomTick`, which is two
    /// different blocks' worth of behavior. Hanging, it ripens by one; planted,
    /// it takes the sapling's one-in-seven step towards a tree.
    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if is_hanging(state) {
            if !is_fully_grown(state) {
                world.set_block(pos, ripened(state), UpdateFlags::UPDATE_CLIENTS);
            }
            return;
        }

        let mut rng = rand::rng();
        if rng.random_range(0..ADVANCE_CHANCE) != 0 {
            return;
        }
        self.sapling.advance_tree(state, world, pos, &mut rng);
    }

    fn as_bonemealable(&self) -> Option<&dyn Bonemealable> {
        Some(self)
    }
}

impl Bonemealable for MangrovePropaguleBlock {
    /// Vanilla parity: `MangrovePropaguleBlock.isValidBonemealTarget`. A ripe
    /// propagule still hanging is the one thing bone meal has nothing to say
    /// to; a planted one always takes it.
    fn is_valid_bonemeal_target(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
    ) -> bool {
        !is_hanging(state) || !is_fully_grown(state)
    }

    /// Vanilla parity: `MangrovePropaguleBlock.isBonemealSuccess`. Ripening a
    /// hanging propagule is certain; growing a planted one is the sapling's
    /// roll.
    fn is_bonemeal_success(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        rng: &mut dyn Rng,
        pos: BlockPos,
    ) -> bool {
        if is_hanging(state) {
            return !is_fully_grown(state);
        }
        self.sapling.is_bonemeal_success(state, world, rng, pos)
    }

    /// Vanilla parity: `MangrovePropaguleBlock.performBonemeal`.
    fn perform_bonemeal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        rng: &mut dyn Rng,
        pos: BlockPos,
    ) {
        if is_hanging(state) && !is_fully_grown(state) {
            world.set_block(pos, ripened(state), UpdateFlags::UPDATE_CLIENTS);
            return;
        }
        self.sapling.perform_bonemeal(state, world, rng, pos);
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::*;
    use crate::test_support::TestLevel;

    #[test]
    fn new_hanging_propagule_starts_at_age_zero() {
        init_vanilla_registry();

        let state = MangrovePropaguleBlock::create_new_hanging_propagule();

        assert_eq!(state.get_block(), &vanilla_blocks::MANGROVE_PROPAGULE);
        assert!(state.get_value(HANGING));
        assert_eq!(state.get_value(AGE), 0);
    }

    #[test]
    fn unsupported_waterlogged_propagule_schedules_water_before_breaking() {
        init_vanilla_registry();
        let behavior = MangrovePropaguleBlock::new(&vanilla_blocks::MANGROVE_PROPAGULE, "mangrove");
        let state = vanilla_blocks::MANGROVE_PROPAGULE
            .default_state()
            .set_value(WATERLOGGED, true);
        let level = TestLevel::default();

        assert!(
            behavior
                .update_shape(
                    state,
                    &level,
                    BlockPos::ZERO,
                    Direction::Down,
                    BlockPos::ZERO.below(),
                    vanilla_blocks::AIR.default_state(),
                )
                .is_air()
        );
        assert!(level.scheduled_water_tick());
    }

    mod growing {
        use steel_registry::init_vanilla_registry;
        use steel_utils::ChunkPos;

        use super::super::*;
        use crate::behavior::init_behaviors;
        use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

        fn behavior() -> MangrovePropaguleBlock {
            MangrovePropaguleBlock::new(&vanilla_blocks::MANGROVE_PROPAGULE, "mangrove")
        }

        /// A propagule hanging under a mangrove leaf, at the age given.
        fn hanging(key: &'static str, age: u8) -> (Arc<World>, BlockPos) {
            init_vanilla_registry();
            init_behaviors();
            let world = fresh_test_world(key);
            insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
            let pos = BlockPos::new(8, 100, 8);
            assert!(world.set_block(
                pos.above(),
                vanilla_blocks::MANGROVE_LEAVES.default_state(),
                UpdateFlags::UPDATE_CLIENTS,
            ));
            assert!(world.set_block(
                pos,
                MangrovePropaguleBlock::create_new_hanging_propagule().set_value(AGE, age),
                UpdateFlags::UPDATE_CLIENTS,
            ));
            (world, pos)
        }

        /// A propagule planted in mud, ready to become a tree.
        fn planted(key: &'static str) -> (Arc<World>, BlockPos) {
            init_vanilla_registry();
            init_behaviors();
            let world = fresh_test_world(key);
            insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
            let pos = BlockPos::new(8, 100, 8);
            // Mud on stone, not mud on nothing. A mangrove sends its roots down
            // before the trunk goes up, and they stop at the first block they
            // cannot grow through; over open air they simply keep going, run
            // past `max_root_length` and the whole feature refuses to place.
            for dx in -4..=4 {
                for dz in -4..=4 {
                    for dy in -6..=-2 {
                        assert!(world.set_block(
                            pos.offset(dx, dy, dz),
                            vanilla_blocks::STONE.default_state(),
                            UpdateFlags::UPDATE_CLIENTS,
                        ));
                    }
                    assert!(world.set_block(
                        pos.offset(dx, -1, dz),
                        vanilla_blocks::MUD.default_state(),
                        UpdateFlags::UPDATE_CLIENTS,
                    ));
                }
            }
            assert!(
                world.set_block(
                    pos,
                    vanilla_blocks::MANGROVE_PROPAGULE
                        .default_state()
                        .set_value(AGE, MAX_AGE),
                    UpdateFlags::UPDATE_CLIENTS,
                )
            );
            (world, pos)
        }

        /// Vanilla parity: the hanging arm of `randomTick`. The fruit ripens by
        /// one age a tick and stops at four; `random_tick` was a no-op, so a
        /// propagule hung at age zero for ever and never became pickable fruit.
        #[test]
        fn a_hanging_propagule_ripens_one_age_at_a_time() {
            let (world, pos) = hanging("propagule_ripens", 0);
            let behavior = behavior();

            for expected in 1..=MAX_AGE {
                behavior.random_tick(world.get_block_state(pos), &world, pos);
                assert_eq!(world.get_block_state(pos).get_value(AGE), expected);
            }

            behavior.random_tick(world.get_block_state(pos), &world, pos);
            assert_eq!(
                world.get_block_state(pos).get_value(AGE),
                MAX_AGE,
                "a ripe propagule stops rather than wrapping back to zero"
            );
        }

        /// Vanilla parity: the planted arm of `randomTick`, which is the
        /// sapling's. A mangrove is what tells this apart from the hanging arm:
        /// a propagule that ripened instead of growing would still be a
        /// propagule.
        #[test]
        fn a_planted_propagule_grows_a_mangrove() {
            let (world, pos) = planted("propagule_grows");
            let behavior = behavior();

            // The advance is one in seven and the tree itself rolls a height,
            // so this is about whether the wiring exists, not one roll. The
            // loop stops the moment the propagule stops being one: the world
            // would dispatch the next tick to whatever block replaced it.
            let mut gave_way = false;
            for _ in 0..400 {
                let state = world.get_block_state(pos);
                if state.get_block() != &vanilla_blocks::MANGROVE_PROPAGULE {
                    gave_way = true;
                    break;
                }
                behavior.random_tick(state, &world, pos);
            }
            assert!(gave_way, "four hundred ticks and it was still a propagule");

            // A mangrove's trunk starts one to three blocks above where the
            // propagule stood, with roots filling the space between, so the
            // proof is a trunk and a canopy somewhere in the volume rather than
            // a log at the stump.
            let anywhere_near = |wanted: BlockRef| {
                (-8..=8).any(|dx| {
                    (0..=24).any(|dy| {
                        (-8..=8).any(|dz| {
                            world.get_block_state(pos.offset(dx, dy, dz)).get_block() == wanted
                        })
                    })
                })
            };
            assert!(
                anywhere_near(&vanilla_blocks::MANGROVE_LOG),
                "a mangrove with no trunk is not a mangrove"
            );
            assert!(
                anywhere_near(&vanilla_blocks::MANGROVE_LEAVES),
                "a mangrove with no leaves is not a mangrove"
            );
        }

        /// Vanilla parity: `isValidBonemealTarget` and `performBonemeal`. Bone
        /// meal ripens a hanging propagule with certainty -- no roll -- and a
        /// ripe one refuses it outright.
        #[test]
        fn bone_meal_ripens_a_hanging_propagule_and_a_ripe_one_refuses_it() {
            let (world, pos) = hanging("propagule_bonemeal", 1);
            let behavior = behavior();
            let mut rng = rand::rng();
            let state = world.get_block_state(pos);

            assert!(behavior.is_valid_bonemeal_target(state, world.as_ref(), pos));
            assert!(behavior.is_bonemeal_success(state, &world, &mut rng, pos));
            behavior.perform_bonemeal(state, &world, &mut rng, pos);
            assert_eq!(world.get_block_state(pos).get_value(AGE), 2);

            let ripe = state.set_value(AGE, MAX_AGE);
            assert!(
                !behavior.is_valid_bonemeal_target(ripe, world.as_ref(), pos),
                "nothing is left to ripen"
            );
        }

        /// Vanilla parity: `getStateForPlacement`, which sets the age to four
        /// whatever the item was picked at -- a planted propagule is never
        /// half-grown fruit, and the default state it starts from is age zero.
        #[test]
        fn a_propagule_is_planted_ripe() {
            use glam::DVec3;
            use steel_registry::item_stack::ItemStack;
            use steel_registry::vanilla_items;
            use steel_utils::types::InteractionHand;

            use crate::behavior::context::{
                BlockHitResult, BlockPlaceContext, PlacementOrientation, PlacementSource,
            };

            let (world, pos) = planted("propagule_placement");
            assert_eq!(
                vanilla_blocks::MANGROVE_PROPAGULE
                    .default_state()
                    .get_value(AGE),
                0,
                "the placement has to reach past the default, or this proves nothing"
            );

            let hit_result = BlockHitResult {
                location: DVec3::ZERO,
                direction: Direction::Up,
                block_pos: pos.below(),
                miss: false,
                inside: false,
                world_border_hit: false,
            };
            let mut held = ItemStack::new(&vanilla_items::MANGROVE_PROPAGULE);
            let source = PlacementSource::direct(
                None,
                InteractionHand::MainHand,
                &mut held,
                PlacementOrientation::Player {
                    rotation: 0.0,
                    pitch: 0.0,
                },
                false,
            );
            let context = BlockPlaceContext::new(&world, source, &hit_result);

            let state = behavior()
                .get_state_for_placement(&context)
                .expect("mud supports a propagule");

            assert!(!state.get_value(HANGING));
            assert_eq!(state.get_value(AGE), MAX_AGE);
            assert!(!state.get_value(WATERLOGGED), "it was planted in open air");
        }
    }
}
