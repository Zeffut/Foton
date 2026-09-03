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
unsafe impl DowncastType for InventoryOpenEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/inventory_open");
}
impl Event for InventoryOpenEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl InventoryOpenEvent {
    pub const fn new(player_id: Uuid) -> Self {
        Self {
            player_id,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
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
    pub const fn new(player_id: Uuid) -> Self {
        Self { player_id }
    }
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
    pub fn new(
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
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Returns the item in the clicked slot before the interaction.
    pub fn current_item(&self) -> Option<&ItemStack> {
        self.current_item.as_ref()
    }
    /// Returns the item held on the cursor before the interaction.
    pub fn cursor_item(&self) -> Option<&ItemStack> {
        self.cursor_item.as_ref()
    }
    /// Returns the Bukkit click type name.
    pub fn click(&self) -> &str {
        &self.click
    }
    /// Returns the raw slot index, when the click targeted a slot.
    pub const fn slot(&self) -> Option<usize> {
        self.slot
    }
    /// Returns whether a listener cancelled this click.
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
    pub fn new(
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
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn slots(&self) -> &[usize] {
        &self.slots
    }
    pub fn old_cursor(&self) -> &ItemStack {
        &self.old_cursor
    }
    pub fn drag_type(&self) -> &str {
        &self.drag_type
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
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
unsafe impl DowncastType for PrepareGrindstoneEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/prepare_grindstone");
}
impl Event for PrepareGrindstoneEvent {}
impl PrepareGrindstoneEvent {
    pub fn new(player_id: Uuid, upper: ItemStack, lower: ItemStack, result: ItemStack) -> Self {
        Self {
            player_id,
            upper,
            lower,
            result,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn upper(&self) -> &ItemStack {
        &self.upper
    }
    pub fn lower(&self) -> &ItemStack {
        &self.lower
    }
    pub fn result(&self) -> &ItemStack {
        &self.result
    }
    pub fn set_upper(&mut self, item: ItemStack) {
        self.upper = item;
    }
    pub fn set_lower(&mut self, item: ItemStack) {
        self.lower = item;
    }
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
unsafe impl DowncastType for PrepareItemCraftEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/prepare_item_craft");
}
impl Event for PrepareItemCraftEvent {}
impl PrepareItemCraftEvent {
    pub fn new(
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
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn matrix(&self) -> &[ItemStack] {
        &self.matrix
    }
    pub fn result(&self) -> &ItemStack {
        &self.result
    }
    pub fn is_repair(&self) -> bool {
        self.is_repair
    }
    pub fn set_matrix(&mut self, matrix: Vec<ItemStack>) {
        self.matrix = matrix;
    }
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
unsafe impl DowncastType for CrafterCraftEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/crafter_craft");
}
impl Event for CrafterCraftEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl CrafterCraftEvent {
    pub fn new(
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
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> foton_utils::BlockPos {
        self.position
    }
    pub fn recipe(&self) -> &str {
        &self.recipe
    }
    pub fn result(&self) -> &ItemStack {
        &self.result
    }
    pub fn remaining_items(&self) -> &[ItemStack] {
        &self.remaining_items
    }
    pub fn set_result(&mut self, result: ItemStack) {
        self.result = result;
    }
    pub fn set_remaining_items(&mut self, items: Vec<ItemStack>) {
        self.remaining_items = items;
    }
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
