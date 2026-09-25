//! Sculk block behavior.
//!
//! Vanilla parity: `SculkBlock extends DropExperienceBlock implements SculkBehaviour`.
//!
//! The `DropExperienceBlock` half -- one experience per block mined -- is here in full;
//! it is the whole of the block's `BlockBehaviour` surface.
//!
//! The `SculkBehaviour` half -- `attemptUseCharge`, `canChangeBlockStateOnSpread` and the
//! rest -- is only ever called by a `SculkSpreader` walking its charge cursors, so it lives
//! next door in `spreader`, which both a live sculk catalyst and the deep-dark world
//! generation feature drive.

use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::item_stack::ItemStack;
use foton_utils::value_providers::IntProvider;
use foton_utils::{BlockPos, BlockStateId};

use crate::behavior::context::BlockPlaceContext;
use crate::behavior::{BlockBehavior, sample_block_experience};
use crate::world::World;

/// Vanilla `SculkBlock`.
#[block_behavior]
pub struct SculkBlock {
    block: BlockRef,
    #[json_arg(int_provider, json = "xp_range")]
    experience: IntProvider,
}

impl SculkBlock {
    /// Creates the sculk block behavior with its extracted experience provider.
    #[must_use]
    pub const fn new(block: BlockRef, experience: IntProvider) -> Self {
        Self { block, experience }
    }
}

impl BlockBehavior for SculkBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn experience_drop(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        tool: &ItemStack,
    ) -> i32 {
        sample_block_experience(tool, &self.experience)
    }
}
