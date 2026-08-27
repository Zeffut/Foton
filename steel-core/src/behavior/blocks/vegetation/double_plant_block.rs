use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, DoubleBlockHalf, EnumProperty,
};
use steel_registry::vanilla_blocks;
use steel_utils::{BlockPos, BlockStateId, axis::Axis, types::UpdateFlags};

use crate::behavior::BlockStateBehaviorExt;
use crate::behavior::block::{BlockBehavior, BlockLootContext};
use crate::behavior::blocks::vegetation::Vegetation;
use crate::behavior::blocks::vegetation::vegetation_block::vegetation_can_survive;
use crate::behavior::context::{BlockPlaceContext, PlacementSource};
use crate::entity::Entity;
use crate::fluid::{FluidStateExt as _, get_fluid_state};
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};
use steel_registry::item_stack::ItemStack;
use steel_utils::types::InteractionHand;

use super::BlockRef;

/// Behavior for vanilla two-block-tall plants.
#[block_behavior]
pub struct DoublePlantBlock {
    pub(super) block: BlockRef,
}

const HALF: &EnumProperty<DoubleBlockHalf> = &BlockStateProperties::DOUBLE_BLOCK_HALF;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

impl DoublePlantBlock {
    /// Creates a new double plant block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    pub(super) fn copy_waterlogged_from(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockStateId {
        if state.try_get_value(WATERLOGGED).is_some() {
            state.set_value(WATERLOGGED, get_fluid_state(world, pos).is_water())
        } else {
            state
        }
    }

    /// Runs Vanilla `DoublePlantBlock.updateShape` while preserving virtual
    /// `canSurvive` dispatch for subclasses such as small dripleaf.
    pub(super) fn update_shape_with_survival(
        &self,
        survival_behavior: &dyn BlockBehavior,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        let half = state.get_value(HALF);
        let neighbor_is_matching_other_half =
            neighbor_state.get_block() == self.block && neighbor_state.get_value(HALF) != half;

        if direction.get_axis() == Axis::Y
            && (half == DoubleBlockHalf::Lower) == (direction == Direction::Up)
            && !neighbor_is_matching_other_half
        {
            return vanilla_blocks::AIR.default_state();
        }

        if half == DoubleBlockHalf::Lower
            && direction == Direction::Down
            && !survival_behavior.can_survive(state, world, pos)
        {
            return vanilla_blocks::AIR.default_state();
        }

        state
    }
    /// Takes the lower half away without letting it drop.
    ///
    /// Vanilla parity: `DoublePlantBlock.preventDropFromBottomPart`, which is
    /// what stops a creative player who breaks the top of a plant from being
    /// handed the bottom of it.
    fn prevent_drop_from_bottom_part(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        player: &Player,
    ) {
        if state.get_value(HALF) != DoubleBlockHalf::Upper {
            return;
        }

        let bottom_pos = pos.below();
        let bottom_state = world.get_block_state(bottom_pos);
        if bottom_state.get_block() != self.block
            || bottom_state.get_value(HALF) != DoubleBlockHalf::Lower
        {
            return;
        }

        let replacement = if get_fluid_state(world, bottom_pos).is_water() {
            vanilla_blocks::WATER.default_state()
        } else {
            vanilla_blocks::AIR.default_state()
        };
        world.set_block(
            bottom_pos,
            replacement,
            UpdateFlags::UPDATE_ALL | UpdateFlags::UPDATE_SUPPRESS_DROPS,
        );
        world.destroy_block_effect(bottom_pos, u32::from(bottom_state.0), Some(player.id()));
    }

    pub(super) fn place_at(
        world: &Arc<World>,
        state: BlockStateId,
        lower_pos: BlockPos,
        update_type: UpdateFlags,
    ) {
        let upper_pos = lower_pos.above();
        world.set_block(
            lower_pos,
            Self::copy_waterlogged_from(
                world,
                lower_pos,
                state.set_value(HALF, DoubleBlockHalf::Lower),
            ),
            update_type,
        );
        world.set_block(
            upper_pos,
            Self::copy_waterlogged_from(
                world,
                upper_pos,
                state.set_value(HALF, DoubleBlockHalf::Upper),
            ),
            update_type,
        );
    }
}

impl Vegetation for DoublePlantBlock {}

impl BlockBehavior for DoublePlantBlock {
    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.update_shape_with_survival(self, state, world, pos, direction, neighbor_state)
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        if state.get_value(HALF) == DoubleBlockHalf::Upper {
            let state_below = world.get_block_state(pos.below());
            state_below.get_block() == state.get_block()
                && state_below.get_value(HALF) == DoubleBlockHalf::Lower
        } else {
            vegetation_can_survive(self, state, world, pos)
        }
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source: &PlacementSource<'_>,
    ) {
        let upper_pos = pos.above();
        let upper_state = Self::copy_waterlogged_from(
            world,
            upper_pos,
            self.block
                .default_state()
                .set_value(HALF, DoubleBlockHalf::Upper),
        );
        world.set_block(upper_pos, upper_state, UpdateFlags::UPDATE_ALL);
    }

    fn player_will_destroy(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> BlockStateId {
        // Vanilla parity: `DoublePlantBlock.playerWillDestroy`. The plant pays
        // out here, while both halves are still standing, because
        // `blocks/large_fern` and `blocks/tall_grass` ask whether the other
        // half is there -- and removing one half takes the other with it, so a
        // roll made afterwards finds nothing and pays nothing.
        if player.has_infinite_materials() {
            self.prevent_drop_from_bottom_part(world, pos, state, player);
            return state;
        }

        let tool = {
            let inventory = player.inventory.lock();
            let held = inventory.get_item_in_hand(InteractionHand::MainHand);
            held.copy_with_count(held.count())
        };
        let drops = BlockLootContext::new(world, pos)
            .with_entity(Some(player))
            .with_tool(&tool)
            .get_drops(state);
        for item in drops {
            if !item.is_empty() {
                world.pop_resource(pos, item);
            }
        }
        state
    }

    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        // Vanilla parity: `DoublePlantBlock.playerDestroy`, which rolls
        // `Blocks.AIR` so the plant cannot be paid for twice --
        // `playerWillDestroy` already paid. Steel rolls a player's break after
        // the block is gone rather than before, so "already gone" is the same
        // test, made here.
        if context.world().get_block_state(context.pos()).get_block() == self.block {
            return None;
        }
        Some(Vec::new())
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        if context.place_pos().y() >= context.world.max_y_exclusive() - 1 {
            return None;
        }
        if !context
            .world
            .get_block_state(context.place_pos().above())
            .can_be_replaced(context)
        {
            return None;
        }
        Some(self.block.default_state())
    }
}
