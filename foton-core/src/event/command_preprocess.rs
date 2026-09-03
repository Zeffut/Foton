use super::Event;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

/// A player command before command dispatch.
pub struct PlayerCommandPreprocessEvent {
    player_id: Uuid,
    message: String,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerCommandPreprocessEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_command_preprocess");
}
impl Event for PlayerCommandPreprocessEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerCommandPreprocessEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid, message: String) -> Self {
        Self {
            player_id,
            message,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// The command line as typed, leading slash included.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Rewrites the line before the server parses it.
    pub fn set_message(&mut self, message: String) {
        self.message = message;
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
