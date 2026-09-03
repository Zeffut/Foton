/// Fired when a world's rain flag changes.
pub struct WeatherChangeEvent {
    world: String,
    raining: bool,
    cancelled: bool,
}
unsafe impl DowncastType for WeatherChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/weather_change");
}
impl Event for WeatherChangeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl WeatherChangeEvent {
    pub fn new(world: impl Into<String>, raining: bool) -> Self {
        Self {
            world: world.into(),
            raining,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn raining(&self) -> bool {
        self.raining
    }
    pub fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// Fired when a world's thunder flag changes.
pub struct ThunderChangeEvent {
    world: String,
    thundering: bool,
    cancelled: bool,
}
unsafe impl DowncastType for ThunderChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/thunder_change");
}
impl Event for ThunderChangeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl ThunderChangeEvent {
    pub fn new(world: impl Into<String>, thundering: bool) -> Self {
        Self {
            world: world.into(),
            thundering,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn thundering(&self) -> bool {
        self.thundering
    }
    pub fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

use foton_utils::BlockPos;
use foton_utils::ChunkPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};

use super::Event;

/// A chunk became fully loaded and visible to gameplay.
pub struct ChunkLoadEvent {
    world: String,
    position: ChunkPos,
    new_chunk: bool,
}
unsafe impl DowncastType for ChunkLoadEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/chunk_load");
}
impl Event for ChunkLoadEvent {}
impl ChunkLoadEvent {
    pub fn new(world: impl Into<String>, position: ChunkPos, new_chunk: bool) -> Self {
        Self {
            world: world.into(),
            position,
            new_chunk,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> ChunkPos {
        self.position
    }
    pub const fn new_chunk(&self) -> bool {
        self.new_chunk
    }
}

/// A nether portal is about to fill its interior with portal blocks.
pub struct PortalCreateEvent {
    world: String,
    blocks: Vec<BlockPos>,
    cancelled: bool,
}
unsafe impl DowncastType for PortalCreateEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/portal_create");
}
impl Event for PortalCreateEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PortalCreateEvent {
    pub fn new(world: impl Into<String>, blocks: Vec<BlockPos>) -> Self {
        Self {
            world: world.into(),
            blocks,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub fn blocks(&self) -> &[BlockPos] {
        &self.blocks
    }
    pub fn blocks_mut(&mut self) -> &mut Vec<BlockPos> {
        &mut self.blocks
    }
    pub fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
