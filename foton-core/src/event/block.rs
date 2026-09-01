//! Events about a player changing the world's blocks.
//!
//! The two a protection plugin is built out of, and the reason cancellation is
//! in the bus at all. Nine of the fifty-nine plugins surveyed in
//! `dev/plugin-api-usage.json` need the break and eleven the place.

use std::sync::Arc;

use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::{BlockPos, BlockStateId};

use super::Event;
use crate::player::Player;

/// A player is about to break a block.
///
/// Fires before anything has happened to the block, so cancelling leaves the
/// world exactly as it was rather than putting it back. That is a deliberate
/// difference from `org.bukkit.event.block.BlockBreakEvent`, which fires after
/// the state has been set and rolls back on cancel: a listener sees the same
/// thing either way, and not having to undo is worth more than matching the
/// order in which Bukkit happens to do it.
pub struct BlockBreakEvent {
    player: Arc<Player>,
    position: BlockPos,
    state: BlockStateId,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for BlockBreakEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_break");
}

impl Event for BlockBreakEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl BlockBreakEvent {
    /// Creates the event for a break that has not happened yet.
    #[must_use]
    pub const fn new(player: Arc<Player>, position: BlockPos, state: BlockStateId) -> Self {
        Self {
            player,
            position,
            state,
            cancelled: false,
        }
    }

    /// The player breaking it.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// Where the block is.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// The block as it still stands.
    #[must_use]
    pub const fn state(&self) -> BlockStateId {
        self.state
    }

    /// Stops the break. The block keeps standing.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player is about to place a block.
///
/// Fires after every vanilla check has passed -- reach, survivability,
/// obstruction -- and before the block is written, so a listener is only asked
/// about placements that would otherwise have happened.
///
/// Only a player's placement reaches this. A dispenser firing a block is not a
/// `BlockPlaceEvent` in Bukkit either.
pub struct BlockPlaceEvent {
    player: Arc<Player>,
    position: BlockPos,
    state: BlockStateId,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for BlockPlaceEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_place");
}

impl Event for BlockPlaceEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl BlockPlaceEvent {
    /// Creates the event for a placement that has not happened yet.
    #[must_use]
    pub const fn new(player: Arc<Player>, position: BlockPos, state: BlockStateId) -> Self {
        Self {
            player,
            position,
            state,
            cancelled: false,
        }
    }

    /// The player placing it.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// Where the block would go.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// The state that would be placed.
    #[must_use]
    pub const fn state(&self) -> BlockStateId {
        self.state
    }

    /// Stops the placement. Nothing is written.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
