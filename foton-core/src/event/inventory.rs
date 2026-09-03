//! Events emitted by inventory interaction paths.

use super::Event;
use foton_registry::item_stack::ItemStack;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

/// Fired when an external inventory view has been opened for a player.
pub struct InventoryOpenEvent {
    player_id: Uuid,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for InventoryOpenEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/inventory_open");
}
impl Event for InventoryOpenEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl InventoryOpenEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid) -> Self {
        Self {
            player_id,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A player's external inventory view was closed.
pub struct InventoryCloseEvent {
    player_id: Uuid,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for InventoryCloseEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/inventory_close");
}
impl Event for InventoryCloseEvent {}
impl InventoryCloseEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid) -> Self {
        Self { player_id }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
}

/// A player clicked a slot in an open container before vanilla applies it.
pub struct InventoryClickEvent {
    player_id: Uuid,
    current_item: Option<ItemStack>,
    cursor_item: Option<ItemStack>,
    click: String,
    slot: Option<usize>,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for InventoryClickEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/inventory_click");
}
impl Event for InventoryClickEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl InventoryClickEvent {
    /// Creates an uncancelled click event.
    #[must_use]
    pub const fn new(
        player_id: Uuid,
        current_item: Option<ItemStack>,
        cursor_item: Option<ItemStack>,
        click: String,
        slot: Option<usize>,
    ) -> Self {
        Self {
            player_id,
            current_item,
            cursor_item,
            click,
            slot,
            cancelled: false,
        }
    }
    /// Returns the clicking player's UUID.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Returns the item in the clicked slot before the interaction.
    #[must_use]
    pub const fn current_item(&self) -> Option<&ItemStack> {
        self.current_item.as_ref()
    }
    /// Returns the item held on the cursor before the interaction.
    #[must_use]
    pub const fn cursor_item(&self) -> Option<&ItemStack> {
        self.cursor_item.as_ref()
    }
    /// Returns the Bukkit click type name.
    #[must_use]
    pub fn click(&self) -> &str {
        &self.click
    }
    /// Returns the raw slot index, when the click targeted a slot.
    #[must_use]
    pub const fn slot(&self) -> Option<usize> {
        self.slot
    }
    /// Returns whether a listener cancelled this click.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Cancels or uncancels the click.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// Fired when a player completes an inventory drag before its distribution.
pub struct InventoryDragEvent {
    player_id: Uuid,
    slots: Vec<usize>,
    old_cursor: ItemStack,
    drag_type: String,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for InventoryDragEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/inventory_drag");
}
impl Event for InventoryDragEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl InventoryDragEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(
        player_id: Uuid,
        slots: Vec<usize>,
        old_cursor: ItemStack,
        drag_type: String,
    ) -> Self {
        Self {
            player_id,
            slots,
            old_cursor,
            drag_type,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Every slot the drag passed over.
    #[must_use]
    pub fn slots(&self) -> &[usize] {
        &self.slots
    }
    /// What was held on the cursor before the drag began.
    #[must_use]
    pub const fn old_cursor(&self) -> &ItemStack {
        &self.old_cursor
    }
    /// How the stack was spread, as Bukkit's `DragType` name, or `UNKNOWN` when the menu did not say.
    #[must_use]
    pub fn drag_type(&self) -> &str {
        &self.drag_type
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// Fired when a grindstone recomputes its preview.
pub struct PrepareGrindstoneEvent {
    player_id: Uuid,
    upper: ItemStack,
    lower: ItemStack,
    result: ItemStack,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PrepareGrindstoneEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/prepare_grindstone");
}
impl Event for PrepareGrindstoneEvent {}
impl PrepareGrindstoneEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(
        player_id: Uuid,
        upper: ItemStack,
        lower: ItemStack,
        result: ItemStack,
    ) -> Self {
        Self {
            player_id,
            upper,
            lower,
            result,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// The item in the top slot.
    #[must_use]
    pub const fn upper(&self) -> &ItemStack {
        &self.upper
    }
    /// The item in the bottom slot.
    #[must_use]
    pub const fn lower(&self) -> &ItemStack {
        &self.lower
    }
    /// What will happen unless a listener changes it.
    #[must_use]
    pub const fn result(&self) -> &ItemStack {
        &self.result
    }
    /// Replaces the item in the top slot.
    pub fn set_upper(&mut self, item: ItemStack) {
        self.upper = item;
    }
    /// Replaces the item in the bottom slot.
    pub fn set_lower(&mut self, item: ItemStack) {
        self.lower = item;
    }
    /// Chooses what happens instead.
    pub fn set_result(&mut self, item: ItemStack) {
        self.result = item;
    }
}

/// Fired when a crafting grid preview is recomputed.
pub struct PrepareItemCraftEvent {
    player_id: Uuid,
    matrix: Vec<ItemStack>,
    result: ItemStack,
    is_repair: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PrepareItemCraftEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/prepare_item_craft");
}
impl Event for PrepareItemCraftEvent {}
impl PrepareItemCraftEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(
        player_id: Uuid,
        matrix: Vec<ItemStack>,
        result: ItemStack,
        is_repair: bool,
    ) -> Self {
        Self {
            player_id,
            matrix,
            result,
            is_repair,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// The crafting grid, read row by row.
    #[must_use]
    pub fn matrix(&self) -> &[ItemStack] {
        &self.matrix
    }
    /// What will happen unless a listener changes it.
    #[must_use]
    pub const fn result(&self) -> &ItemStack {
        &self.result
    }
    /// Whether this is two damaged tools being combined rather than a recipe.
    #[must_use]
    pub const fn is_repair(&self) -> bool {
        self.is_repair
    }
    /// Replaces what is laid out on the grid.
    pub fn set_matrix(&mut self, matrix: Vec<ItemStack>) {
        self.matrix = matrix;
    }
    /// Chooses what happens instead.
    pub fn set_result(&mut self, result: ItemStack) {
        self.result = result;
    }
}

/// Fired immediately before an automated crafter ejects a crafted result.
pub struct CrafterCraftEvent {
    world: String,
    position: foton_utils::BlockPos,
    recipe: String,
    result: ItemStack,
    remaining_items: Vec<ItemStack>,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for CrafterCraftEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/crafter_craft");
}
impl Event for CrafterCraftEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl CrafterCraftEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(
        world: String,
        position: foton_utils::BlockPos,
        recipe: String,
        result: ItemStack,
        remaining_items: Vec<ItemStack>,
    ) -> Self {
        Self {
            world,
            position,
            recipe,
            result,
            remaining_items,
            cancelled: false,
        }
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> foton_utils::BlockPos {
        self.position
    }
    /// Which recipe the crafter matched.
    #[must_use]
    pub fn recipe(&self) -> &str {
        &self.recipe
    }
    /// What will happen unless a listener changes it.
    #[must_use]
    pub const fn result(&self) -> &ItemStack {
        &self.result
    }
    /// What stays in the grid afterwards, such as an emptied bucket.
    #[must_use]
    pub fn remaining_items(&self) -> &[ItemStack] {
        &self.remaining_items
    }
    /// Chooses what happens instead.
    pub fn set_result(&mut self, result: ItemStack) {
        self.result = result;
    }
    /// Changes what stays in the grid afterwards.
    pub fn set_remaining_items(&mut self, items: Vec<ItemStack>) {
        self.remaining_items = items;
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

#[cfg(test)]
mod tests {
    use super::CrafterCraftEvent;
    use crate::event::{Event, EventBus};
    use foton_registry::item_stack::ItemStack;
    use foton_utils::{BlockPos, Identifier};

    #[test]
    fn crafter_craft_event_can_be_cancelled() {
        let bus = EventBus::new();
        bus.on::<CrafterCraftEvent, _>(Identifier::from_foton("test"), |event| {
            event.set_cancelled(true);
        });
        let mut event = CrafterCraftEvent::new(
            "world".to_owned(),
            BlockPos::new(1, 64, -2),
            "minecraft:stick".to_owned(),
            ItemStack::empty(),
            vec![],
        );
        bus.fire(&mut event);
        assert!(event.is_cancelled());
        assert_eq!(event.position(), BlockPos::new(1, 64, -2));
    }
}
