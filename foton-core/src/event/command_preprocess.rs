use uuid::Uuid;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use super::Event;

/// A player command before command dispatch.
pub struct PlayerCommandPreprocessEvent { player_id: Uuid, message: String, cancelled: bool }
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerCommandPreprocessEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_command_preprocess");
}
impl Event for PlayerCommandPreprocessEvent { fn is_cancelled(&self) -> bool { self.cancelled } }
impl PlayerCommandPreprocessEvent {
    pub fn new(player_id: Uuid, message: String) -> Self { Self { player_id, message, cancelled: false } }
    pub fn player_id(&self) -> Uuid { self.player_id }
    pub fn message(&self) -> &str { &self.message }
    pub fn set_message(&mut self, message: String) { self.message = message; }
    pub fn is_cancelled(&self) -> bool { self.cancelled }
    pub fn set_cancelled(&mut self, cancelled: bool) { self.cancelled = cancelled; }
}
