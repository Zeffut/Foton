//! Events about the server itself.

use std::net::IpAddr;

use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use text_components::TextComponent;

use super::Event;

/// A client asked for the server-list entry.
///
/// Bukkit parity: `ServerListPingEvent`. What a listener leaves as the MOTD
/// and the player limit is what the client is shown.
pub struct ServerListPingEvent {
    address: IpAddr,
    motd: TextComponent,
    online: i32,
    max_players: i32,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for ServerListPingEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/server_list_ping");
}

impl Event for ServerListPingEvent {}

impl ServerListPingEvent {
    /// Creates the event with the configured entry.
    #[must_use]
    pub const fn new(address: IpAddr, motd: TextComponent, online: i32, max_players: i32) -> Self {
        Self {
            address,
            motd,
            online,
            max_players,
        }
    }

    /// Who is asking.
    #[must_use]
    pub const fn address(&self) -> IpAddr {
        self.address
    }

    /// The message of the day.
    #[must_use]
    pub const fn motd(&self) -> &TextComponent {
        &self.motd
    }

    /// Changes the message of the day.
    pub fn set_motd(&mut self, motd: TextComponent) {
        self.motd = motd;
    }

    /// How many players are online.
    #[must_use]
    pub const fn online(&self) -> i32 {
        self.online
    }

    /// The player limit shown.
    #[must_use]
    pub const fn max_players(&self) -> i32 {
        self.max_players
    }

    /// Changes the player limit shown.
    pub const fn set_max_players(&mut self, max_players: i32) {
        self.max_players = max_players;
    }
}

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
