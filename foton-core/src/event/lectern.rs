use super::Event;
use foton_utils::BlockPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

/// Fired before a player takes a book from a lectern.
pub struct PlayerTakeLecternBookEvent {
    player: Uuid,
    world: String,
    position: BlockPos,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerTakeLecternBookEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_take_lectern_book");
}
impl Event for PlayerTakeLecternBookEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerTakeLecternBookEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(player: Uuid, world: impl Into<String>, position: BlockPos) -> Self {
        Self {
            player,
            world: world.into(),
            position,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
