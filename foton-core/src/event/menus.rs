//! Events a workstation screen raises: preparing an enchantment or a smithing
//! result, and buying from a merchant.

use foton_registry::item_stack::ItemStack;
use foton_registry::trading::MerchantOffer;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::{BlockPos, Identifier};
use uuid::Uuid;

use super::Event;

/// One of the three offers an enchanting table shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnchantOffer {
    /// The enchantment the screen names as a clue.
    pub enchantment: Identifier,
    /// Its level.
    pub level: i32,
    /// The experience levels the offer requires.
    pub cost: i32,
}

/// An enchanting table has worked out its offers for the item put in it.
///
/// Paper parity: `PrepareItemEnchantEvent`, fired from `slotsChanged` once
/// the three costs and clues are rolled. It starts cancelled for an item that
/// cannot be enchanted; cancelling blanks every offer, and each offer a
/// listener changes is what the screen shows and what a click is priced at.
pub struct PrepareItemEnchantEvent {
    player: Uuid,
    world: String,
    table: BlockPos,
    item: ItemStack,
    offers: [Option<EnchantOffer>; 3],
    bonus: i32,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PrepareItemEnchantEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/prepare_item_enchant");
}

impl Event for PrepareItemEnchantEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PrepareItemEnchantEvent {
    /// Creates the event, cancelled already when `enchantable` is false.
    #[must_use]
    pub const fn new(
        player: Uuid,
        world: String,
        table: BlockPos,
        item: ItemStack,
        offers: [Option<EnchantOffer>; 3],
        bonus: i32,
        enchantable: bool,
    ) -> Self {
        Self {
            player,
            world,
            table,
            item,
            offers,
            bonus,
            cancelled: !enchantable,
        }
    }

    /// Who is enchanting.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The table's world.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Where the table stands.
    #[must_use]
    pub const fn table(&self) -> BlockPos {
        self.table
    }

    /// The item to enchant.
    #[must_use]
    pub const fn item(&self) -> &ItemStack {
        &self.item
    }

    /// The three offers; `None` where the row is empty.
    #[must_use]
    pub const fn offers(&self) -> &[Option<EnchantOffer>; 3] {
        &self.offers
    }

    /// Replaces the offers.
    pub fn set_offers(&mut self, offers: [Option<EnchantOffer>; 3]) {
        self.offers = offers;
    }

    /// The enchanting power of the shelves around the table.
    #[must_use]
    pub const fn bonus(&self) -> i32 {
        self.bonus
    }

    /// Blanks the offers, or shows them again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A smithing table has worked out what its three inputs make.
///
/// Paper parity: `PrepareSmithingEvent`, fired each time the result is
/// recomputed, empty or not. The result a listener leaves is the one shown;
/// taking it still needs a recipe to match, as vanilla's `mayPickup` does.
pub struct PrepareSmithingEvent {
    player: Uuid,
    inputs: [ItemStack; 3],
    result: ItemStack,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PrepareSmithingEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/prepare_smithing");
}

impl Event for PrepareSmithingEvent {}

impl PrepareSmithingEvent {
    /// Creates the event for the template, base and addition laid out.
    #[must_use]
    pub const fn new(player: Uuid, inputs: [ItemStack; 3], result: ItemStack) -> Self {
        Self {
            player,
            inputs,
            result,
        }
    }

    /// Who is at the table.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// Template, base and addition.
    #[must_use]
    pub const fn inputs(&self) -> &[ItemStack; 3] {
        &self.inputs
    }

    /// What the table offers.
    #[must_use]
    pub const fn result(&self) -> &ItemStack {
        &self.result
    }

    /// Changes what the table offers.
    pub fn set_result(&mut self, result: ItemStack) {
        self.result = result;
    }
}

/// A player is about to take a trade from a merchant's screen.
///
/// Paper parity: `PlayerPurchaseEvent`, or `PlayerTradeEvent` when the
/// merchant is a mob. It fires once per trade, so a shift-click that buys
/// five times asks five times; cancelling leaves that trade undone.
pub struct PlayerPurchaseEvent {
    player: Uuid,
    trader: Option<Uuid>,
    offer: MerchantOffer,
    offers: Vec<MerchantOffer>,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerPurchaseEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_purchase");
}

impl Event for PlayerPurchaseEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerPurchaseEvent {
    /// Creates the event for `offer`, one of the merchant's `offers`.
    #[must_use]
    pub const fn new(
        player: Uuid,
        trader: Option<Uuid>,
        offer: MerchantOffer,
        offers: Vec<MerchantOffer>,
    ) -> Self {
        Self {
            player,
            trader,
            offer,
            offers,
            cancelled: false,
        }
    }

    /// Who is buying.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The mob selling, or `None` for a merchant a plugin made.
    #[must_use]
    pub const fn trader(&self) -> Option<Uuid> {
        self.trader
    }

    /// The trade being taken.
    #[must_use]
    pub const fn offer(&self) -> &MerchantOffer {
        &self.offer
    }

    /// Every trade the merchant offers.
    #[must_use]
    pub fn offers(&self) -> &[MerchantOffer] {
        &self.offers
    }

    /// Refuses the trade, or allows it again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
