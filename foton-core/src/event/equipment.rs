//! Events about what a player wears and what they finish using.

use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::types::InteractionHand;
use uuid::Uuid;

use super::Event;

/// Something a player wears changed.
///
/// Fired from the once-a-tick equipment comparison, so it sees every way a
/// piece of armor arrives or leaves -- a click, a dispenser, a command, the
/// armor breaking -- after the fact. Not cancellable, as in Paper.
pub struct PlayerArmorChangeEvent {
    player: Uuid,
    slot: EquipmentSlot,
    old_item: ItemStack,
    new_item: ItemStack,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerArmorChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_armor_change");
}

impl Event for PlayerArmorChangeEvent {}

impl PlayerArmorChangeEvent {
    /// Creates the event for one armor slot changing.
    #[must_use]
    pub const fn new(
        player: Uuid,
        slot: EquipmentSlot,
        old_item: ItemStack,
        new_item: ItemStack,
    ) -> Self {
        Self {
            player,
            slot,
            old_item,
            new_item,
        }
    }

    /// Who is wearing it.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// Which armor slot changed.
    #[must_use]
    pub const fn slot(&self) -> EquipmentSlot {
        self.slot
    }

    /// What was there before.
    #[must_use]
    pub const fn old_item(&self) -> &ItemStack {
        &self.old_item
    }

    /// What is there now.
    #[must_use]
    pub const fn new_item(&self) -> &ItemStack {
        &self.new_item
    }
}

/// A player is about to finish using an item -- eating, drinking.
///
/// A listener may cancel it, swap what is consumed, or choose what is left in
/// the hand afterwards instead of the item's own leftover (the bowl, the
/// bottle).
pub struct PlayerItemConsumeEvent {
    player: Uuid,
    hand: InteractionHand,
    item: ItemStack,
    replacement: Option<ItemStack>,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerItemConsumeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_item_consume");
}

impl Event for PlayerItemConsumeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerItemConsumeEvent {
    /// Creates the event for `item`, held in `hand`.
    #[must_use]
    pub const fn new(player: Uuid, hand: InteractionHand, item: ItemStack) -> Self {
        Self {
            player,
            hand,
            item,
            replacement: None,
            cancelled: false,
        }
    }

    /// Who is consuming.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The hand holding the item.
    #[must_use]
    pub const fn hand(&self) -> InteractionHand {
        self.hand
    }

    /// What will be consumed.
    #[must_use]
    pub const fn item(&self) -> &ItemStack {
        &self.item
    }

    /// Changes what will be consumed.
    pub fn set_item(&mut self, item: ItemStack) {
        self.item = item;
    }

    /// What the hand will hold afterwards, when a listener chose it.
    #[must_use]
    pub const fn replacement(&self) -> Option<&ItemStack> {
        self.replacement.as_ref()
    }

    /// Chooses what the hand holds afterwards; `None` keeps the item's own.
    pub fn set_replacement(&mut self, replacement: Option<ItemStack>) {
        self.replacement = replacement;
    }

    /// Stops the item being consumed, or lets it be consumed again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player pressed the swap-hands key.
///
/// A listener may cancel it, or choose what each hand holds afterwards in
/// place of the plain exchange. Both stacks are named by where they are going,
/// as in Paper: the main-hand item is what the main hand will hold.
pub struct PlayerSwapHandItemsEvent {
    player: Uuid,
    main_hand_item: ItemStack,
    off_hand_item: ItemStack,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerSwapHandItemsEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_swap_hand_items");
}

impl Event for PlayerSwapHandItemsEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerSwapHandItemsEvent {
    /// Creates the event for a plain exchange: `main_hand_item` is what the
    /// offhand holds now, `off_hand_item` what the main hand holds now.
    #[must_use]
    pub const fn new(player: Uuid, main_hand_item: ItemStack, off_hand_item: ItemStack) -> Self {
        Self {
            player,
            main_hand_item,
            off_hand_item,
            cancelled: false,
        }
    }

    /// Who pressed the key.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// What the main hand will hold.
    #[must_use]
    pub const fn main_hand_item(&self) -> &ItemStack {
        &self.main_hand_item
    }

    /// Chooses what the main hand will hold.
    pub fn set_main_hand_item(&mut self, item: ItemStack) {
        self.main_hand_item = item;
    }

    /// What the offhand will hold.
    #[must_use]
    pub const fn off_hand_item(&self) -> &ItemStack {
        &self.off_hand_item
    }

    /// Chooses what the offhand will hold.
    pub fn set_off_hand_item(&mut self, item: ItemStack) {
        self.off_hand_item = item;
    }

    /// Stops the swap, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player is about to trade what they hold for what an armor stand wears.
///
/// Fired from the stand's swap, after its own locks have been checked and
/// before anything moves. Cancelling leaves both stacks where they are.
pub struct PlayerArmorStandManipulateEvent {
    player: Uuid,
    armor_stand: Uuid,
    slot: EquipmentSlot,
    hand: InteractionHand,
    player_item: ItemStack,
    armor_stand_item: ItemStack,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerArmorStandManipulateEvent {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("foton:event/player_armor_stand_manipulate");
}

impl Event for PlayerArmorStandManipulateEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerArmorStandManipulateEvent {
    /// Creates the event for `player` swapping with the stand's `slot`.
    #[must_use]
    pub const fn new(
        player: Uuid,
        armor_stand: Uuid,
        slot: EquipmentSlot,
        hand: InteractionHand,
        player_item: ItemStack,
        armor_stand_item: ItemStack,
    ) -> Self {
        Self {
            player,
            armor_stand,
            slot,
            hand,
            player_item,
            armor_stand_item,
            cancelled: false,
        }
    }

    /// Who is swapping.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The stand.
    #[must_use]
    pub const fn armor_stand(&self) -> Uuid {
        self.armor_stand
    }

    /// The stand's slot the swap is about.
    #[must_use]
    pub const fn slot(&self) -> EquipmentSlot {
        self.slot
    }

    /// The hand the player used.
    #[must_use]
    pub const fn hand(&self) -> InteractionHand {
        self.hand
    }

    /// What the player holds.
    #[must_use]
    pub const fn player_item(&self) -> &ItemStack {
        &self.player_item
    }

    /// What the stand wears in `slot`.
    #[must_use]
    pub const fn armor_stand_item(&self) -> &ItemStack {
        &self.armor_stand_item
    }

    /// Stops the swap, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
