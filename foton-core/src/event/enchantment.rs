//! Enchanting-table offers, before plugins decide whether they are usable.

use foton_registry::{enchantment::EnchantmentRef, item_stack::ItemStack};
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey};
use uuid::Uuid;

use super::Event;

/// One mutable Bukkit enchantment clue and its required experience level.
#[derive(Clone, Copy, Debug)]
pub struct EnchantmentOffer {
    /// The clue, rather than the complete enchantment list granted on click.
    pub enchantment: EnchantmentRef,
    /// The clue's enchantment level.
    pub level: i32,
    /// Minimum player experience level required to select the offer.
    pub cost: i32,
}

/// The full per-menu seed and offers. Costs may exist without a clue in Vanilla.
#[derive(Clone, Copy, Debug)]
pub struct EnchantmentViewState {
    /// Full-width menu seed, independent of the truncated client data slot.
    pub seed: i32,
    /// Experience level gates for the three buttons.
    pub costs: [i32; 3],
    /// Optional visible clues.
    pub offers: [Option<EnchantmentOffer>; 3],
}

impl EnchantmentViewState {
    /// Paper preserves a clue-less vanilla cost, but clears a removed clue's cost.
    pub fn apply_offers(&mut self, offers: [Option<EnchantmentOffer>; 3]) {
        for (index, offer) in offers.iter().enumerate() {
            if let Some(offer) = offer {
                self.costs[index] = offer.cost;
            } else if self.offers[index].is_some() {
                self.costs[index] = 0;
            }
        }
        self.offers = offers;
    }

    /// Clears the costs as well as the clues when a prepare event is cancelled.
    pub const fn clear(&mut self) {
        self.costs = [0; 3];
        self.offers = [None; 3];
    }
}

/// A recomputed nonempty enchanting input, including initially cancelled inputs.
pub struct PrepareItemEnchantEvent {
    /// Process-unique identity of the menu emitting this event.
    pub menu_id: u64,
    /// The player using the table.
    pub player_id: Uuid,
    /// The table's world key.
    pub world: String,
    /// Position of the table.
    pub position: BlockPos,
    /// Actual item and lapis stacks; applied back after dispatch.
    pub items: [ItemStack; 2],
    /// Number of usable bookshelves around the table.
    pub bonus: i32,
    /// Per-menu seed, costs and clues.
    pub state: EnchantmentViewState,
    /// Initially true for a non-enchantable item; plugins can allow it.
    pub cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PrepareItemEnchantEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/prepare_item_enchant");
}

impl Event for PrepareItemEnchantEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PrepareItemEnchantEvent {
    /// The player using the table (compatibility accessor).
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player_id
    }

    /// The table's world key.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Position of the table.
    #[must_use]
    pub const fn table(&self) -> BlockPos {
        self.position
    }

    /// The actual input item; lapis remains available through `items[1]`.
    #[must_use]
    pub const fn item(&self) -> &ItemStack {
        &self.items[0]
    }

    /// Typed clues for the three offer rows.
    #[must_use]
    pub const fn offers(&self) -> &[Option<EnchantmentOffer>; 3] {
        &self.state.offers
    }

    /// Replaces clues and updates their corresponding costs.
    pub fn set_offers(&mut self, offers: [Option<EnchantmentOffer>; 3]) {
        self.state.apply_offers(offers);
    }

    /// Enchanting power of the shelves around the table.
    #[must_use]
    pub const fn bonus(&self) -> i32 {
        self.bonus
    }

    /// Cancels or allows preparation; the menu clears cancelled offers.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
