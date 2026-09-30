//! Events about what a block gives when a player breaks or picks from it.

use foton_registry::item_stack::ItemStack;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId};
use uuid::Uuid;

use super::Event;

/// A block a player broke has dropped its items.
///
/// The items are already in the world when this runs, so a listener can read
/// and change them as entities; any it removes from the list, or all of them
/// when it cancels, are taken back out before a client sees them.
pub struct BlockDropItemEvent {
    player: Uuid,
    world: String,
    position: BlockPos,
    broken: BlockStateId,
    items: Vec<Uuid>,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for BlockDropItemEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_drop_item");
}

impl Event for BlockDropItemEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl BlockDropItemEvent {
    /// Creates the event for the item entities one break produced.
    #[must_use]
    pub const fn new(
        player: Uuid,
        world: String,
        position: BlockPos,
        broken: BlockStateId,
        items: Vec<Uuid>,
    ) -> Self {
        Self {
            player,
            world,
            position,
            broken,
            items,
            cancelled: false,
        }
    }

    /// Who broke the block.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The world it was in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Where it was.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// The block as it stood before it broke.
    #[must_use]
    pub const fn broken(&self) -> BlockStateId {
        self.broken
    }

    /// The item entities that stay.
    #[must_use]
    pub fn items(&self) -> &[Uuid] {
        &self.items
    }

    /// Keeps only these item entities.
    pub fn set_items(&mut self, items: Vec<Uuid>) {
        self.items = items;
    }

    /// Takes every item back, or lets them stay again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player is picking from a block without breaking it -- sweet berries off
/// a bush, glow berries off cave vines.
///
/// The stacks have not been dropped yet: a listener may change them, and a
/// cancel leaves the block unpicked.
pub struct PlayerHarvestBlockEvent {
    player: Uuid,
    world: String,
    position: BlockPos,
    hand: InteractionHand,
    items: Vec<ItemStack>,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerHarvestBlockEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_harvest_block");
}

impl Event for PlayerHarvestBlockEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerHarvestBlockEvent {
    /// Creates the event for the stacks a harvest would drop.
    #[must_use]
    pub const fn new(
        player: Uuid,
        world: String,
        position: BlockPos,
        hand: InteractionHand,
        items: Vec<ItemStack>,
    ) -> Self {
        Self {
            player,
            world,
            position,
            hand,
            items,
            cancelled: false,
        }
    }

    /// Who is harvesting.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The world the block is in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Where the block is.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// The hand used.
    #[must_use]
    pub const fn hand(&self) -> InteractionHand {
        self.hand
    }

    /// What will be dropped.
    #[must_use]
    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }

    /// Replaces what will be dropped.
    pub fn set_items(&mut self, items: Vec<ItemStack>) {
        self.items = items;
    }

    /// Leaves the block unpicked, or lets the harvest happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
