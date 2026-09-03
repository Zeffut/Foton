use super::Event;
use foton_utils::BlockPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

/// Fired before bonemeal grows the clicked block.
pub struct BlockFertilizeEvent {
    world: String,
    position: BlockPos,
    player: Option<Uuid>,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for BlockFertilizeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_fertilize");
}
impl Event for BlockFertilizeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockFertilizeEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(world: impl Into<String>, position: BlockPos, player: Option<Uuid>) -> Self {
        Self {
            world: world.into(),
            position,
            player,
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
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Option<Uuid> {
        self.player
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
