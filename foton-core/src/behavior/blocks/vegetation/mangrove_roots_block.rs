//! Vanilla `MangroveRootsBlock` behavior.

use foton_macros::block_behavior;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty, Direction};
use foton_utils::{BlockPos, BlockStateId};

use super::BlockRef;
use crate::behavior::block::{BlockBehavior, schedule_water_tick_if_waterlogged};
use crate::behavior::context::BlockPlaceContext;
use crate::world::ScheduledTickAccess;

/// Vanilla `MangroveRootsBlock`.
///
/// The class is a plain waterloggable block: its only other override,
/// `skipRendering` against a vertical neighbour, is client-side. Muddy mangrove
/// roots are a separate block obtained by crafting, not by any interaction with
/// this one.
#[block_behavior]
pub struct MangroveRootsBlock {
    block: BlockRef,
}

const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

impl MangroveRootsBlock {
    /// Creates a mangrove roots behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for MangroveRootsBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(WATERLOGGED, context.is_water_source()),
        )
    }

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

        state
    }
}
