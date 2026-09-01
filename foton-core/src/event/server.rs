//! Events about the server itself.

use foton_utils::downcast::{DowncastType, DowncastTypeKey};

use super::Event;

/// One game tick happened.
///
/// Not vanilla, and not gameplay: it exists so that something outside
/// `foton-core` can do work on the tick thread without `foton-core` knowing
/// what that work is. A plugin scheduler is the first user, and it is the
/// reason `runTask` can promise what Bukkit promises -- that a task body runs
/// where it is safe to touch the world.
///
/// Fired once per tick, after the tick's own work. A listener here runs inside
/// the tick and delays it, which is the price of being allowed to touch
/// anything at all.
pub struct ServerTickEvent {
    tick: u64,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for ServerTickEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/server_tick");
}

impl Event for ServerTickEvent {}

impl ServerTickEvent {
    /// Creates the event for one tick.
    #[must_use]
    pub const fn new(tick: u64) -> Self {
        Self { tick }
    }

    /// Which tick this was, counted from the server starting.
    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.tick
    }
}
