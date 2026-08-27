use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::REGISTRY;
use steel_registry::blocks::{
    BlockRef,
    block_state_ext::BlockStateExt,
    properties::{BlockStateProperties, BoolProperty, Direction, IntProperty},
    shapes::VoxelShape,
};
use steel_registry::vanilla_blocks;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockLocalAabb, BlockPos, BlockStateId};

use crate::behavior::{
    BlockBehavior, BlockCollisionContext, BlockPlaceContext,
    block::schedule_water_tick_if_waterlogged,
};
use crate::entity::entities::FallingBlockEntity;
use crate::world::{LevelReader, ScheduledTickAccess, World};

const SHAPE_STABLE_BOXES: &[BlockLocalAabb] = &[
    BlockLocalAabb::new(0.0, 0.875, 0.0, 1.0, 1.0, 1.0),
    BlockLocalAabb::new(0.0, 0.0, 0.0, 0.125, 1.0, 0.125),
    BlockLocalAabb::new(0.875, 0.0, 0.0, 1.0, 1.0, 0.125),
    BlockLocalAabb::new(0.0, 0.0, 0.875, 0.125, 1.0, 1.0),
    BlockLocalAabb::new(0.875, 0.0, 0.875, 1.0, 1.0, 1.0),
];
const SHAPE_UNSTABLE_BOTTOM_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, 0.0, 0.0, 1.0, 0.125, 1.0)];
const SHAPE_BELOW_BLOCK_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, -1.0, 0.0, 1.0, 0.0, 1.0)];

const SHAPE_STABLE: VoxelShape = VoxelShape::from_boxes(SHAPE_STABLE_BOXES);
const SHAPE_UNSTABLE_BOTTOM: VoxelShape = VoxelShape::from_boxes(SHAPE_UNSTABLE_BOTTOM_BOXES);
const SHAPE_BELOW_BLOCK: VoxelShape = VoxelShape::from_boxes(SHAPE_BELOW_BLOCK_BOXES);

/// Vanilla parity: `ScaffoldingBlock`.
///
/// Everything the block does hangs off one number. `distance` counts how many
/// blocks of scaffolding stand between this one and something solid, and the
/// tick that recomputes it is what makes a tower stand, shrink its foothold at
/// the bottom, and come down all at once when its support goes.
#[block_behavior]
pub struct ScaffoldingBlock {
    block: BlockRef,
}

const BOTTOM: &BoolProperty = &BlockStateProperties::BOTTOM;
const STABILITY_DISTANCE: &IntProperty = &BlockStateProperties::STABILITY_DISTANCE;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Vanilla parity: the `7` seeding `ScaffoldingBlock.getDistance`, which is
/// also the maximum value of the `distance` property.
const MAX_STABILITY_DISTANCE: i32 = 7;

/// Vanilla parity: `ScaffoldingBlock.TICK_DELAY`.
const TICK_DELAY: i32 = 1;

/// Narrows a distance to what the property holds.
///
/// `get_distance` returns a count, and the property is one digit wide; the two
/// meet at seven, which is both the property's maximum and the value that means
/// "unsupported".
fn stability_value(distance: i32) -> u8 {
    debug_assert!((0..=MAX_STABILITY_DISTANCE).contains(&distance));
    u8::try_from(distance.clamp(0, MAX_STABILITY_DISTANCE)).unwrap_or(0)
}

impl ScaffoldingBlock {
    /// Creates a scaffolding block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Returns whether scaffolding at `pos` is the lowest of its column.
    ///
    /// Vanilla parity: `ScaffoldingBlock.isBottom`. The `distance > 0` is what
    /// keeps a block resting on solid ground from growing the extra rim: it
    /// has no gap under it to fall through.
    fn is_bottom(&self, level: &dyn LevelReader, pos: BlockPos, distance: i32) -> bool {
        distance > 0 && level.get_block_state(pos.below()).get_block() != self.block
    }

    /// Returns how far `pos` is from something that can hold scaffolding up,
    /// where `7` means "too far".
    ///
    /// Vanilla parity: the static `ScaffoldingBlock.getDistance`.
    #[must_use]
    pub fn get_distance(level: &dyn LevelReader, pos: BlockPos) -> i32 {
        let below_pos = pos.below();
        let below = level.get_block_state(below_pos);

        let mut distance = if below.get_block() == &vanilla_blocks::SCAFFOLDING {
            i32::from(below.get_value(STABILITY_DISTANCE))
        } else if level.is_face_sturdy(below, below_pos, Direction::Up) {
            return 0;
        } else {
            MAX_STABILITY_DISTANCE
        };

        for direction in Direction::HORIZONTAL {
            let neighbor = level.get_block_state(direction.relative(pos));
            if neighbor.get_block() != &vanilla_blocks::SCAFFOLDING {
                continue;
            }
            distance = distance.min(i32::from(neighbor.get_value(STABILITY_DISTANCE)) + 1);
            if distance == 1 {
                break;
            }
        }

        distance
    }
}

impl BlockBehavior for ScaffoldingBlock {
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
        world.schedule_block_tick_default(pos, self.block, 1);
        state
    }

    /// Vanilla parity: `ScaffoldingBlock.getStateForPlacement`.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let pos = context.place_pos();
        let level = context.world.as_ref();
        let distance = Self::get_distance(level, pos);
        Some(
            self.block
                .default_state()
                .set_value(WATERLOGGED, context.is_water_source())
                .set_value(STABILITY_DISTANCE, stability_value(distance))
                .set_value(BOTTOM, self.is_bottom(level, pos, distance)),
        )
    }

    /// Vanilla parity: `ScaffoldingBlock.onPlace`.
    ///
    /// The tick this queues is the only thing that ever writes `distance`
    /// again. Scaffolding that arrives at its position by any route other than
    /// the placement above -- a `/setblock`, a structure, a piston -- carries
    /// the default `7` until this fires, and then either settles on a real
    /// distance or falls.
    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, TICK_DELAY);
    }

    /// Vanilla parity: `ScaffoldingBlock.tick`.
    ///
    /// The two ways out at distance seven are not interchangeable. A block that
    /// was already seven never had support to lose, so it becomes a falling
    /// block; one that had a distance and lost it is destroyed where it stands,
    /// and the neighbours it was holding up are told, which is what walks the
    /// collapse up and along the tower.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let distance = Self::get_distance(world.as_ref(), pos);
        let new_state = state
            .set_value(STABILITY_DISTANCE, stability_value(distance))
            .set_value(BOTTOM, self.is_bottom(world.as_ref(), pos, distance));

        if distance == MAX_STABILITY_DISTANCE {
            if i32::from(state.get_value(STABILITY_DISTANCE)) == MAX_STABILITY_DISTANCE {
                let _ = FallingBlockEntity::fall(world, pos, new_state);
            } else {
                world.destroy_block(pos, true);
            }
        } else if state != new_state {
            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
        }
    }

    /// Vanilla parity: `ScaffoldingBlock.canSurvive`.
    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::get_distance(world, pos) < MAX_STABILITY_DISTANCE
    }

    /// Vanilla parity: `ScaffoldingBlock.canBeReplaced`, which inverts the
    /// default: scaffolding is replaceable by scaffolding and by nothing else,
    /// so a tower grows when clicked with its own item and blocks anything
    /// else from being placed into it.
    fn can_be_replaced(&self, _state: BlockStateId, context: &BlockPlaceContext<'_>) -> bool {
        let scaffolding_item = REGISTRY.items.by_block(self.block);
        context.with_item(|item| item.is(scaffolding_item))
    }

    fn get_collision_shape(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        pos: BlockPos,
        context: BlockCollisionContext,
    ) -> VoxelShape {
        if context.is_placement() {
            return VoxelShape::EMPTY;
        }

        if context.is_above(VoxelShape::FULL_BLOCK, pos, true) && !context.is_descending() {
            return SHAPE_STABLE;
        }

        let distance = state.get_value(STABILITY_DISTANCE);
        let bottom = state.get_value(BOTTOM);
        if distance != 0 && bottom && context.is_above(SHAPE_BELOW_BLOCK, pos, true) {
            SHAPE_UNSTABLE_BOTTOM
        } else {
            VoxelShape::EMPTY
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_fluids};

    use crate::test_support::TestLevel;

    fn scaffolding_state(distance: u8, bottom: bool) -> BlockStateId {
        vanilla_blocks::SCAFFOLDING
            .default_state()
            .set_value(STABILITY_DISTANCE, distance)
            .set_value(BOTTOM, bottom)
    }

    fn collision_shape(state: BlockStateId, context: BlockCollisionContext) -> VoxelShape {
        let behavior = ScaffoldingBlock::new(&vanilla_blocks::SCAFFOLDING);
        let level = TestLevel::default().with_min_y(0);
        behavior.get_collision_shape(state, &level, BlockPos::new(0, 64, 0), context)
    }

    #[test]
    fn placement_context_has_no_scaffolding_collision() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(0, false),
            BlockCollisionContext::pre_move(65.0, false),
        );

        assert_eq!(shape, VoxelShape::EMPTY);
    }

    #[test]
    fn entity_above_scaffolding_collides_with_stable_shape() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(0, false),
            BlockCollisionContext::entity(65.0, false),
        );

        assert_eq!(shape, SHAPE_STABLE);
    }

    #[test]
    fn descending_entity_only_collides_with_unstable_bottom_shape() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(1, true),
            BlockCollisionContext::entity(64.5, true),
        );

        assert_eq!(shape, SHAPE_UNSTABLE_BOTTOM);
    }

    #[test]
    fn non_bottom_descending_scaffolding_has_empty_collision() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(1, false),
            BlockCollisionContext::entity(64.5, true),
        );

        assert_eq!(shape, VoxelShape::EMPTY);
    }

    #[test]
    fn shape_update_schedules_stability_and_water_ticks() {
        init_vanilla_registry();
        let behavior = ScaffoldingBlock::new(&vanilla_blocks::SCAFFOLDING);
        let state = vanilla_blocks::SCAFFOLDING
            .default_state()
            .set_value(WATERLOGGED, true);
        let pos = BlockPos::new(0, 64, 0);
        let level = TestLevel::default();

        assert_eq!(
            behavior.update_shape(
                state,
                &level,
                pos,
                Direction::North,
                pos.north(),
                vanilla_blocks::AIR.default_state(),
            ),
            state
        );
        assert_eq!(
            level
                .scheduled_block_ticks
                .borrow()
                .iter()
                .map(|tick| (tick.pos, tick.block, tick.delay))
                .collect::<Vec<_>>(),
            vec![(pos, &vanilla_blocks::SCAFFOLDING, 1)]
        );
        assert_eq!(
            level
                .scheduled_fluid_ticks
                .borrow()
                .iter()
                .map(|tick| (tick.pos, tick.fluid, tick.delay))
                .collect::<Vec<_>>(),
            vec![(pos, &vanilla_fluids::WATER, 5)]
        );
    }

    mod stability {
        use glam::DVec3;
        use steel_registry::entity_type::EntityTypeRef;
        use steel_registry::item_stack::ItemStack;
        use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
        use steel_utils::types::InteractionHand;
        use steel_utils::{ChunkPos, WorldAabb};

        use super::super::*;
        use crate::behavior::context::{BlockHitResult, PlacementOrientation, PlacementSource};
        use crate::behavior::init_behaviors;
        use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

        /// A world with one loaded chunk and the scaffolding behavior ready.
        fn bench(key: &'static str) -> (Arc<World>, ScaffoldingBlock) {
            init_vanilla_registry();
            init_behaviors();
            let world = fresh_test_world(key);
            insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
            (world, ScaffoldingBlock::new(&vanilla_blocks::SCAFFOLDING))
        }

        /// Puts untouched scaffolding at `pos`, the way a `/setblock` does:
        /// carrying the default distance of seven until something ticks it.
        fn raw_scaffolding(world: &Arc<World>, pos: BlockPos) {
            assert!(world.set_block(
                pos,
                vanilla_blocks::SCAFFOLDING.default_state(),
                UpdateFlags::UPDATE_ALL,
            ));
        }

        fn distance_at(world: &Arc<World>, pos: BlockPos) -> i32 {
            i32::from(world.get_block_state(pos).get_value(STABILITY_DISTANCE))
        }

        fn entities_of(world: &Arc<World>, kind: EntityTypeRef) -> usize {
            let everywhere = WorldAabb::new(0.0, 0.0, 0.0, 16.0, 128.0, 16.0);
            world
                .get_entities_in_aabb_matching(&everywhere, |entity| entity.entity_type() == kind)
                .len()
        }

        fn place_context<'a>(
            world: &'a Arc<World>,
            clicked: BlockPos,
            item: &'a mut ItemStack,
        ) -> BlockPlaceContext<'a> {
            let hit_result = BlockHitResult {
                location: DVec3::ZERO,
                direction: Direction::Up,
                block_pos: clicked,
                miss: false,
                inside: false,
                world_border_hit: false,
            };
            let source = PlacementSource::direct(
                None,
                InteractionHand::MainHand,
                item,
                PlacementOrientation::Player {
                    rotation: 0.0,
                    pitch: 0.0,
                },
                false,
            );
            BlockPlaceContext::new(world, source, &hit_result)
        }

        /// Vanilla parity: `ScaffoldingBlock.getStateForPlacement`. Placement is
        /// the one path that writes a real distance without waiting for a tick.
        #[test]
        fn scaffolding_placed_on_the_ground_starts_at_distance_zero() {
            let (world, behavior) = bench("scaffolding_placement");
            let ground = BlockPos::new(8, 64, 8);
            assert!(world.set_block(
                ground,
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_ALL,
            ));

            let mut held = ItemStack::new(&vanilla_items::SCAFFOLDING);
            let context = place_context(&world, ground, &mut held);
            let state = behavior
                .get_state_for_placement(&context)
                .expect("scaffolding always has a placement state");

            assert_eq!(i32::from(state.get_value(STABILITY_DISTANCE)), 0);
            assert!(
                !state.get_value(BOTTOM),
                "a block standing on the ground has no gap to grow its rim into"
            );
        }

        /// Vanilla parity: `ScaffoldingBlock.tick`, which is the only thing that
        /// ever writes `distance` after placement. Vertical stacking keeps the
        /// distance it inherits; only reaching sideways costs a point, which is
        /// what limits a scaffolding bridge to six blocks of overhang.
        #[test]
        fn a_tick_walks_the_distance_out_sideways_but_not_upwards() {
            let (world, behavior) = bench("scaffolding_distance");
            assert!(world.set_block(
                BlockPos::new(8, 63, 8),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_ALL,
            ));

            let column = [BlockPos::new(8, 64, 8), BlockPos::new(8, 65, 8)];
            let arm = [
                BlockPos::new(9, 65, 8),
                BlockPos::new(10, 65, 8),
                BlockPos::new(11, 65, 8),
            ];
            for pos in column.into_iter().chain(arm) {
                raw_scaffolding(&world, pos);
                assert_eq!(
                    distance_at(&world, pos),
                    MAX_STABILITY_DISTANCE,
                    "untouched scaffolding starts at the default seven"
                );
            }

            for pos in column.into_iter().chain(arm) {
                behavior.tick(world.get_block_state(pos), &world, pos);
            }

            assert_eq!(distance_at(&world, column[0]), 0);
            assert_eq!(distance_at(&world, column[1]), 0, "stacking is free");
            assert_eq!(distance_at(&world, arm[0]), 1);
            assert_eq!(distance_at(&world, arm[1]), 2);
            assert_eq!(distance_at(&world, arm[2]), 3);
            assert!(
                world.get_block_state(arm[0]).get_value(BOTTOM),
                "scaffolding with air under it grows the rim you can stand on"
            );
        }

        /// Vanilla parity: the `state.getValue(DISTANCE) == 7` arm of
        /// `ScaffoldingBlock.tick`. A block that never had support has nothing
        /// to be destroyed for, so it becomes a falling block instead.
        #[test]
        fn scaffolding_that_never_had_support_falls() {
            let (world, behavior) = bench("scaffolding_falls");
            let pos = BlockPos::new(8, 70, 8);
            raw_scaffolding(&world, pos);

            behavior.tick(world.get_block_state(pos), &world, pos);

            assert_eq!(
                world.get_block_state(pos).get_block(),
                &vanilla_blocks::AIR,
                "the block left its position"
            );
            assert_eq!(entities_of(&world, &vanilla_entities::FALLING_BLOCK), 1);
            assert_eq!(
                entities_of(&world, &vanilla_entities::ITEM),
                0,
                "a falling block is not a drop"
            );
        }

        /// Vanilla parity: the `else` arm of the same branch. Losing a support
        /// it once had destroys the block where it stands, which drops it and
        /// tells its neighbours -- and that is what walks a collapse along a
        /// tower rather than raining falling blocks.
        #[test]
        fn scaffolding_that_loses_its_support_is_destroyed_and_drops() {
            let (world, behavior) = bench("scaffolding_collapse");
            let ground = BlockPos::new(8, 63, 8);
            let pos = BlockPos::new(8, 64, 8);
            assert!(world.set_block(
                ground,
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_ALL,
            ));
            raw_scaffolding(&world, pos);
            behavior.tick(world.get_block_state(pos), &world, pos);
            assert_eq!(
                distance_at(&world, pos),
                0,
                "it has to have had a support before it can lose one"
            );

            assert!(world.set_block(
                ground,
                vanilla_blocks::AIR.default_state(),
                UpdateFlags::UPDATE_ALL,
            ));
            behavior.tick(world.get_block_state(pos), &world, pos);

            assert_eq!(world.get_block_state(pos).get_block(), &vanilla_blocks::AIR);
            assert_eq!(entities_of(&world, &vanilla_entities::FALLING_BLOCK), 0);
            assert_eq!(
                entities_of(&world, &vanilla_entities::ITEM),
                1,
                "it dropped itself"
            );
        }

        /// Vanilla parity: `ScaffoldingBlock.canBeReplaced`, which inverts the
        /// default rule instead of narrowing it.
        #[test]
        fn only_the_scaffolding_item_replaces_scaffolding() {
            let (world, behavior) = bench("scaffolding_replace");
            let pos = BlockPos::new(8, 64, 8);
            let state = vanilla_blocks::SCAFFOLDING.default_state();

            let mut scaffolding = ItemStack::new(&vanilla_items::SCAFFOLDING);
            assert!(
                behavior.can_be_replaced(state, &place_context(&world, pos, &mut scaffolding)),
                "a tower grows when clicked with its own item"
            );

            let mut stone = ItemStack::new(&vanilla_items::STONE);
            assert!(!behavior.can_be_replaced(state, &place_context(&world, pos, &mut stone)));

            let mut nothing = ItemStack::empty();
            assert!(!behavior.can_be_replaced(state, &place_context(&world, pos, &mut nothing)));
        }

        /// Vanilla parity: `ScaffoldingBlock.canSurvive`, which asks the same
        /// question the tick does and is what refuses a placement into thin air.
        #[test]
        fn scaffolding_only_survives_within_reach_of_a_support() {
            let (world, _) = bench("scaffolding_survive");
            let behavior = ScaffoldingBlock::new(&vanilla_blocks::SCAFFOLDING);
            let ground = BlockPos::new(8, 63, 8);
            assert!(world.set_block(
                ground,
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_ALL,
            ));
            let state = vanilla_blocks::SCAFFOLDING.default_state();

            assert!(behavior.can_survive(state, world.as_ref(), BlockPos::new(8, 64, 8)));
            assert!(!behavior.can_survive(state, world.as_ref(), BlockPos::new(8, 70, 8)));
        }
    }
}
