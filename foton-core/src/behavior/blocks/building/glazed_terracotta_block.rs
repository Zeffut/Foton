//! Glazed terracotta block behavior.

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use foton_utils::BlockStateId;

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;

/// Vanilla `GlazedTerracottaBlock` placement facing.
#[block_behavior]
pub struct GlazedTerracottaBlock {
    block: BlockRef,
}

const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

impl GlazedTerracottaBlock {
    /// Creates a new glazed terracotta block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for GlazedTerracottaBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(HORIZONTAL_FACING, context.horizontal_direction().opposite()),
        )
    }
}
