//! Events emitted by inventory interaction paths.

use uuid::Uuid;
use foton_registry::item_stack::ItemStack;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use super::Event;

/// A player clicked a slot in an open container before vanilla applies it.
pub struct InventoryClickEvent { player_id: Uuid, current_item: Option<ItemStack>, cancelled: bool }

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for InventoryClickEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/inventory_click");
}
impl Event for InventoryClickEvent { fn is_cancelled(&self) -> bool { self.cancelled } }
impl InventoryClickEvent {
    /// Creates an uncancelled click event.
    pub fn new(player_id: Uuid, current_item: Option<ItemStack>) -> Self {
        Self { player_id, current_item, cancelled: false }
    }
    /// Returns the clicking player's UUID.
    pub const fn player_id(&self) -> Uuid { self.player_id }
    /// Returns the item in the clicked slot before the interaction.
    pub fn current_item(&self) -> Option<&ItemStack> { self.current_item.as_ref() }
    /// Returns whether a listener cancelled this click.
    pub const fn is_cancelled(&self) -> bool { self.cancelled }
    /// Cancels or uncancels the click.
    pub const fn set_cancelled(&mut self, cancelled: bool) { self.cancelled = cancelled; }
}
