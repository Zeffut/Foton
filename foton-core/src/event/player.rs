//! Events about a player arriving and leaving.
//!
//! These two first because they are what the ecosystem asks for most: of the
//! fifty-nine most-downloaded server plugins surveyed in
//! `dev/plugin-api-usage.json`, forty need the join and thirty-six the quit.
//! Two events reach two thirds of that corpus, which is why the measurement
//! came before the design.

use std::sync::Arc;

use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use text_components::TextComponent;

use super::Event;
use crate::player::Player;
use foton_utils::Identifier;

/// A player finished joining, and the server is about to announce it.
///
/// Not cancellable, matching `org.bukkit.event.player.PlayerJoinEvent`: by the
/// time this fires the player is in the world, and a listener that wanted to
/// stop them should have done it before they got there. What it can change is
/// the announcement.
pub struct PlayerJoinEvent {
    player: Arc<Player>,
    message: Option<TextComponent>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerJoinEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_join");
}

impl Event for PlayerJoinEvent {}

impl PlayerJoinEvent {
    /// Creates the event with the announcement the server would make on its own.
    #[must_use]
    pub const fn new(player: Arc<Player>, message: Option<TextComponent>) -> Self {
        Self { player, message }
    }

    /// The player who joined.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// What will be announced, or `None` when nothing will be.
    #[must_use]
    pub const fn message(&self) -> Option<&TextComponent> {
        self.message.as_ref()
    }

    /// Changes the announcement. `None` suppresses it entirely.
    pub fn set_message(&mut self, message: Option<TextComponent>) {
        self.message = message;
    }

    /// Takes the announcement out, for the server to send.
    #[must_use]
    pub fn into_message(self) -> Option<TextComponent> {
        self.message
    }
}

/// A player is leaving, and the server is about to announce it.
///
/// Not cancellable, matching `org.bukkit.event.player.PlayerQuitEvent`. A
/// connection that has gone will not come back because a listener objected.
pub struct PlayerQuitEvent {
    player: Arc<Player>,
    message: Option<TextComponent>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerQuitEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_quit");
}

impl Event for PlayerQuitEvent {}

impl PlayerQuitEvent {
    /// Creates the event with the announcement the server would make on its own.
    #[must_use]
    pub const fn new(player: Arc<Player>, message: Option<TextComponent>) -> Self {
        Self { player, message }
    }

    /// The player who left.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// What will be announced, or `None` when nothing will be.
    #[must_use]
    pub const fn message(&self) -> Option<&TextComponent> {
        self.message.as_ref()
    }

    /// Changes the announcement. `None` suppresses it entirely.
    pub fn set_message(&mut self, message: Option<TextComponent>) {
        self.message = message;
    }

    /// Takes the announcement out, for the server to send.
    #[must_use]
    pub fn into_message(self) -> Option<TextComponent> {
        self.message
    }
}

/// A player said something, before anyone else hears it.
///
/// Corresponds to `org.bukkit.event.player.AsyncPlayerChatEvent`, which the
/// corpus still prefers ten to four over Paper's newer `AsyncChatEvent`. The
/// name drops the `Async` because Foton's chat is not: it is handled on the
/// packet path and dispatched from there, and calling it async would be
/// describing Bukkit's threading rather than Foton's.
///
/// Cancellable, and five of the ten plugins that touch it do cancel.
pub struct PlayerChatEvent {
    player: Arc<Player>,
    message: String,
    changed: bool,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerChatEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_chat");
}

impl Event for PlayerChatEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerChatEvent {
    /// Creates the event carrying what the player actually typed.
    #[must_use]
    pub const fn new(player: Arc<Player>, message: String) -> Self {
        Self {
            player,
            message,
            changed: false,
            cancelled: false,
        }
    }

    /// The player who spoke.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// What will be said.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Rewrites what will be said.
    ///
    /// This has a consequence a listener cannot see: the client signed the
    /// text it sent, and that signature does not cover a rewritten one. A
    /// changed message therefore goes out unsigned, which is the only honest
    /// option -- forwarding someone's signature over words they did not write
    /// is exactly what signed chat exists to prevent.
    pub fn set_message(&mut self, message: String) {
        self.message = message;
        self.changed = true;
    }

    /// Whether a listener rewrote the message.
    #[must_use]
    pub const fn was_changed(&self) -> bool {
        self.changed
    }

    /// Stops the message from being said at all.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    /// Takes the message out, for the server to send.
    #[must_use]
    pub fn into_message(self) -> String {
        self.message
    }
}

/// An opaque custom payload sent by a player.
///
/// Vanilla discards payload types it does not understand. Exposing the bytes
/// here preserves that default while allowing an optional protocol extension,
/// such as the plugin host, to subscribe without coupling it to the player.
pub struct PlayerCustomPayloadEvent {
    player: Arc<Player>,
    channel: Identifier,
    payload: Vec<u8>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerCustomPayloadEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_custom_payload");
}

impl Event for PlayerCustomPayloadEvent {}

impl PlayerCustomPayloadEvent {
    /// Creates an event carrying the packet's untouched channel and bytes.
    #[must_use]
    pub const fn new(player: Arc<Player>, channel: Identifier, payload: Vec<u8>) -> Self {
        Self {
            player,
            channel,
            payload,
        }
    }

    /// The player who sent the payload.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// The custom payload type identifier.
    #[must_use]
    pub const fn channel(&self) -> &Identifier {
        &self.channel
    }

    /// The packet bytes after its identifier.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
