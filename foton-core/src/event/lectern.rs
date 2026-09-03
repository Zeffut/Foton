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
unsafe impl DowncastType for PlayerTakeLecternBookEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_take_lectern_book");
}
impl Event for PlayerTakeLecternBookEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerTakeLecternBookEvent {
    pub fn new(player: Uuid, world: impl Into<String>, position: BlockPos) -> Self {
        Self {
            player,
            world: world.into(),
            position,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
