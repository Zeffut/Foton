//! Trapped chest behavior.
//!
//! Vanilla parity: `TrappedChestBlock`, which extends `ChestBlock` and adds
//! exactly one thing -- a redstone signal counting the players looking inside.
//! Everything else a chest does it still does: it pairs into a double chest,
//! it refuses to open under a solid block, and hoppers reach it either way.
//! So the chest behavior is held rather than reimplemented, and only the four
//! signal methods differ.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::properties::Direction;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::blocks::container::ChestBlock;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::inventory::lock::AttachedContainers;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, SignalQueryContext, World};

/// Strongest signal a trapped chest can give.
///
/// Vanilla parity: the `Mth.clamp(getOpenCount(...), 0, 15)` of `ownSignal`.
const MAX_SIGNAL: i32 = 15;

/// Behavior for the trapped chest.
#[block_behavior]
pub struct TrappedChestBlock {
    /// The chest this is, in every respect but the signal.
    chest: ChestBlock,
    block: BlockRef,
}

impl TrappedChestBlock {
    /// Creates a trapped chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            chest: ChestBlock::trapped(block),
            block,
        }
    }

    /// Returns how many players are looking inside.
    ///
    /// Vanilla parity: `ChestBlockEntity.getOpenCount`. Only this half is
    /// counted; a double trapped chest gives each half its own count, which is
    /// why opening one side powers only that side.
    fn viewers(world: &dyn LevelReader, pos: BlockPos) -> i32 {
        world
            .get_block_entity(pos)
            .map_or(0, |entity| entity.base().opener_count())
    }
}

impl BlockBehavior for TrappedChestBlock {
    // The chest half, delegated.

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.chest.get_state_for_placement(context)
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
        self.chest
            .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        self.chest
            .use_without_item(state, world, pos, player, hit_result, inv)
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.chest.new_block_entity(level, pos, state)
    }

    fn get_attached_containers(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> AttachedContainers {
        self.chest.get_attached_containers(state, world, pos)
    }

    fn has_analog_output_signal(&self, state: BlockStateId) -> bool {
        self.chest.has_analog_output_signal(state)
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        self.chest
            .get_analog_output_signal(state, world, pos, direction)
    }

    // And the part that makes it a trapped chest.

    /// Vanilla parity: `TrappedChestBlock.isSignalSource`.
    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    /// Vanilla parity: `TrappedChestBlock.ownSignal`, one level of signal per
    /// player looking inside, up to fifteen.
    ///
    /// This is `own_signal` rather than `get_signal` because that is the one
    /// vanilla overrides. `get_signal` falls back to `own_signal`, so this
    /// covers both, whereas overriding only `get_signal` would leave
    /// `get_best_own_or_neighbour_signal` reading zero. Nothing asks that
    /// question of a trapped chest today -- wire reaches this through
    /// `get_signal` either way -- so the difference is not observable yet; it
    /// is written the vanilla way so it stays right when something does.
    fn get_own_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        Self::viewers(world, pos).clamp(0, MAX_SIGNAL)
    }

    /// Vanilla parity: `TrappedChestBlock.getDirectSignal`, which powers only
    /// the block above -- the reason a trapped chest cannot strongly power a
    /// block beside it.
    fn get_direct_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
        context: SignalQueryContext,
    ) -> i32 {
        if direction == Direction::Up {
            self.get_signal(state, world, pos, direction, context)
        } else {
            0
        }
    }

    /// Tells the neighbors when somebody opens or closes it.
    ///
    /// Vanilla parity: `TrappedChestBlockEntity.signalOpenCount`. Without this
    /// the signal would be correct and nothing would ever read it again, so the
    /// chest would appear to work only when something else happened to poke the
    /// block beside it.
    ///
    /// The block below is updated as well as the block itself -- the same pair
    /// a pressure plate updates -- so the signal also reaches a wire buried
    /// under the chest, not just one running alongside it.
    fn on_opener_count_changed(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        _state: BlockStateId,
        previous: i32,
        current: i32,
    ) {
        if previous == current {
            return;
        }
        world.update_neighbors_at(pos, self.block);
        world.update_neighbors_at(pos.below(), self.block);
    }
}
