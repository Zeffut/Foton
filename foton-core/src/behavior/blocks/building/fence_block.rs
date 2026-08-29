//! Fence block behavior implementation.
//!
//! Fences connect to adjacent fences, fence gates, and solid blocks.

use std::sync::Arc;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, schedule_water_tick_if_waterlogged};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::items::bind_player_mobs;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};
use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use foton_registry::vanilla_block_tags::BlockTag;
use foton_utils::{BlockPos, BlockStateId};

/// Behavior for fence blocks.
///
/// Fences have 4 boolean properties (north, east, south, west) that indicate
/// whether the fence connects in that direction. A fence connects to:
/// - Other fences of the same type
/// - Fence gates facing the appropriate direction
/// - Blocks with a sturdy face on the connecting side
#[block_behavior]
pub struct FenceBlock {
    block: BlockRef,
}

const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
/// North connection property.
pub const NORTH: &BoolProperty = &BlockStateProperties::NORTH;
/// East connection property.
pub const EAST: &BoolProperty = &BlockStateProperties::EAST;
/// South connection property.
pub const SOUTH: &BoolProperty = &BlockStateProperties::SOUTH;
/// West connection property.
pub const WEST: &BoolProperty = &BlockStateProperties::WEST;
/// Waterlogged property.
pub const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

impl FenceBlock {
    /// Creates a new fence block behavior for the given block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Checks if this fence should connect to the given neighbor state.
    fn connects_to(
        world: &dyn LevelReader,
        neighbor_state: BlockStateId,
        neighbor_pos: BlockPos,
        direction: Direction,
    ) -> bool {
        let neighbor_block = neighbor_state.get_block();

        // Check if it's a fence (same tag)
        if neighbor_block.has_tag(&BlockTag::FENCES) {
            return true;
        }

        // Check if it's a fence gate facing the right direction
        if neighbor_block.has_tag(&BlockTag::FENCE_GATES) {
            // Fence gates connect perpendicular to their facing direction
            // A gate facing north/south connects to fences to its east/west
            // A gate facing east/west connects to fences to its north/south
            if let Some(gate_facing) = neighbor_state.try_get_value(HORIZONTAL_FACING) {
                // Gate connects perpendicular to its facing
                let connects = match (gate_facing, direction) {
                    // Gate facing N/S connects to blocks on E/W sides,
                    // Gate facing E/W connects to blocks on N/S sides
                    (Direction::North | Direction::South, Direction::East | Direction::West)
                    | (Direction::East | Direction::West, Direction::North | Direction::South) => {
                        true
                    }
                    _ => false,
                };
                if connects {
                    return true;
                }
            }
        }

        // Check if the neighbor has a sturdy face on the opposite side
        let opposite = match direction {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        world.is_face_sturdy(neighbor_state, neighbor_pos, opposite)
    }

    /// Gets the connection state for a position by checking all 4 horizontal neighbors.
    fn get_connection_state(&self, world: &Arc<World>, pos: BlockPos) -> BlockStateId {
        let mut state = self.block.default_state();

        // Check north
        let north_pos = Direction::North.relative(pos);
        let north_state = world.get_block_state(north_pos);
        let connects_north = Self::connects_to(world, north_state, north_pos, Direction::North);
        state = state.set_value(NORTH, connects_north);

        // Check east
        let east_pos = Direction::East.relative(pos);
        let east_state = world.get_block_state(east_pos);
        let connects_east = Self::connects_to(world, east_state, east_pos, Direction::East);
        state = state.set_value(EAST, connects_east);

        // Check south
        let south_pos = Direction::South.relative(pos);
        let south_state = world.get_block_state(south_pos);
        let connects_south = Self::connects_to(world, south_state, south_pos, Direction::South);
        state = state.set_value(SOUTH, connects_south);

        // Check west
        let west_pos = Direction::West.relative(pos);
        let west_state = world.get_block_state(west_pos);
        let connects_west = Self::connects_to(world, west_state, west_pos, Direction::West);
        state = state.set_value(WEST, connects_west);

        state
    }
}

impl BlockBehavior for FenceBlock {
    /// Vanilla parity: `FenceBlock.useWithoutItem`.
    ///
    /// The client-side branch has no counterpart -- Foton only runs the server
    /// half of the interaction.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        bind_player_mobs(player, world, pos)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        log::debug!(
            "FenceBlock::get_state_for_placement called for {:?} at {:?}",
            self.block.key,
            context.place_pos()
        );
        Some(
            self.get_connection_state(context.world, context.place_pos())
                .set_value(WATERLOGGED, context.is_water_source()),
        )
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);

        // Only update for horizontal directions
        match direction {
            Direction::North => {
                let connects =
                    Self::connects_to(world, neighbor_state, neighbor_pos, Direction::North);
                state.set_value(NORTH, connects)
            }
            Direction::East => {
                let connects =
                    Self::connects_to(world, neighbor_state, neighbor_pos, Direction::East);
                state.set_value(EAST, connects)
            }
            Direction::South => {
                let connects =
                    Self::connects_to(world, neighbor_state, neighbor_pos, Direction::South);
                state.set_value(SOUTH, connects)
            }
            Direction::West => {
                let connects =
                    Self::connects_to(world, neighbor_state, neighbor_pos, Direction::West);
                state.set_value(WEST, connects)
            }
            // Vertical directions don't affect fence connections
            Direction::Up | Direction::Down => state,
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_fluids};
    use foton_utils::types::{InteractionHand, UpdateFlags};
    use foton_utils::{BlockPos, ChunkPos, Downcast as _};
    use glam::DVec3;

    use crate::behavior::init_behaviors;
    use crate::entity::entities::{LeashFenceKnotEntity, PigEntity};
    use crate::entity::{SharedEntity, next_entity_id};
    use crate::test_support::{
        TestLevel, TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk,
    };

    use super::*;

    const FENCE_POS: BlockPos = BlockPos::new(8, 64, 8);

    #[test]
    fn waterlogged_fence_update_shape_schedules_water_tick() {
        init_vanilla_registry();

        let behavior = FenceBlock::new(&vanilla_blocks::OAK_FENCE);
        let state = vanilla_blocks::OAK_FENCE
            .default_state()
            .set_value(WATERLOGGED, true);
        let level = TestLevel::default();

        let updated = behavior.update_shape(
            state,
            &level,
            BlockPos::ZERO,
            Direction::Up,
            Direction::Up.relative(BlockPos::ZERO),
            vanilla_blocks::AIR.default_state(),
        );

        assert_eq!(updated, state);
        assert_eq!(
            level
                .scheduled_fluid_ticks
                .borrow()
                .iter()
                .map(|tick| (tick.fluid, tick.delay))
                .collect::<Vec<_>>(),
            vec![(&vanilla_fluids::WATER, 5)]
        );
    }

    /// The empty-hand click is the path an ordinary (non-sneaking) player takes
    /// when tying a mob to a fence, so it is worth covering separately from
    /// `LeadItem::use_on`.
    #[test]
    fn clicking_a_fence_while_leading_a_pig_ties_it_to_a_knot() {
        let world = fresh_test_world("fence_use_without_item_binds_leashes");
        init_behaviors();
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(FENCE_POS));
        let state = vanilla_blocks::OAK_FENCE.default_state();
        assert!(world.set_block(FENCE_POS, state, UpdateFlags::UPDATE_ALL));

        let player = TestPlayerBuilder::new(Arc::clone(&world), "Leader", next_entity_id()).build();
        let player_entity: SharedEntity = Arc::clone(&player) as SharedEntity;
        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(8.0, 64.0, 7.0),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&pig))
            .expect("pig should attach to the loaded test chunk");
        let mob = pig.as_mob().expect("pig should expose mob behavior");
        mob.set_leashed_to(&player_entity);

        let behavior = FenceBlock::new(&vanilla_blocks::OAK_FENCE);
        let mut inventory =
            InventoryAccess::new(Arc::clone(&player.inventory), InteractionHand::MainHand);
        let result = behavior.use_without_item(
            state,
            &world,
            FENCE_POS,
            player.as_ref(),
            &BlockHitResult {
                location: DVec3::new(8.5, 65.0, 8.5),
                direction: Direction::Up,
                block_pos: FENCE_POS,
                miss: false,
                inside: false,
                world_border_hit: false,
            },
            &mut inventory,
        );

        assert_eq!(result, InteractionResult::SuccessServer);
        let holder = mob.leash_holder().expect("pig should stay leashed");
        assert_eq!(
            holder
                .downcast_ref::<LeashFenceKnotEntity>()
                .expect("pig should now be held by a fence knot")
                .block_pos(),
            FENCE_POS
        );
    }
}
