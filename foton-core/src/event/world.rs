/// Fired when a world's rain flag changes.
pub struct WeatherChangeEvent {
    world: String,
    raining: bool,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for WeatherChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/weather_change");
}
impl Event for WeatherChangeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl WeatherChangeEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(world: impl Into<String>, raining: bool) -> Self {
        Self {
            world: world.into(),
            raining,
            cancelled: false,
        }
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// True when rain is starting, false when it is stopping.
    #[must_use]
    pub const fn raining(&self) -> bool {
        self.raining
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// Fired when a world's thunder flag changes.
pub struct ThunderChangeEvent {
    world: String,
    thundering: bool,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for ThunderChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/thunder_change");
}
impl Event for ThunderChangeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl ThunderChangeEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(world: impl Into<String>, thundering: bool) -> Self {
        Self {
            world: world.into(),
            thundering,
            cancelled: false,
        }
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// True when a storm is starting, false when it is ending.
    #[must_use]
    pub const fn thundering(&self) -> bool {
        self.thundering
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for ChunkLoadEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/chunk_load");
}
impl Event for ChunkLoadEvent {}
impl ChunkLoadEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(world: impl Into<String>, position: ChunkPos, new_chunk: bool) -> Self {
        Self {
            world: world.into(),
            position,
            new_chunk,
        }
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> ChunkPos {
        self.position
    }
    /// Whether the chunk was just generated rather than read back from disk.
    #[must_use]
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PortalCreateEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/portal_create");
}
impl Event for PortalCreateEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PortalCreateEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(world: impl Into<String>, blocks: Vec<BlockPos>) -> Self {
        Self {
            world: world.into(),
            blocks,
            cancelled: false,
        }
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Every block this will affect.
    #[must_use]
    pub fn blocks(&self) -> &[BlockPos] {
        &self.blocks
    }
    /// Every block this will affect, so a listener can take some out.
    pub const fn blocks_mut(&mut self) -> &mut Vec<BlockPos> {
        &mut self.blocks
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
