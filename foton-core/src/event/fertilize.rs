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
unsafe impl DowncastType for BlockFertilizeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_fertilize");
}
impl Event for BlockFertilizeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockFertilizeEvent {
    pub fn new(world: impl Into<String>, position: BlockPos, player: Option<Uuid>) -> Self {
        Self {
            world: world.into(),
            position,
            player,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn player_id(&self) -> Option<Uuid> {
        self.player
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
