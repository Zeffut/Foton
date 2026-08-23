//! Sculk block behavior.
//!
//! Vanilla parity: `SculkBlock extends DropExperienceBlock implements SculkBehaviour`.
//!
//! The `DropExperienceBlock` half -- one experience per block mined -- is here in full;
//! it is the whole of the block's `BlockBehaviour` surface.
//!
//! Not implemented: the `SculkBehaviour` half. `attemptUseCharge` and
//! `canChangeBlockStateOnSpread` are only ever called by a `SculkSpreader` walking its
//! charge cursors, and Steel has no level-side spreader: the only port of that algorithm
//! is bound to `WorldGenRegion` inside `worldgen/feature/features/sculk_patch.rs`, which
//! is what grows the deep dark at world generation. Until that algorithm is lifted to
//! `World`, a sculk block in a live world never grows a sensor or a shrieker on top of
//! itself, and `SculkCatalystBlock` -- the block that would drive it -- is unimplemented
//! for the same reason.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::item_stack::ItemStack;
use steel_utils::value_providers::IntProvider;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::context::BlockPlaceContext;
use crate::behavior::{BlockBehavior, try_drop_experience};
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

    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            try_drop_experience(world, pos, tool, &self.experience);
        }
    }
}
