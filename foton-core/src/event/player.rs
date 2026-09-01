//! Events about a player arriving and leaving.
//!
//! These two first because they are what the ecosystem asks for most: of the
//! fifty-nine most-downloaded server plugins surveyed in
//! `dev/plugin-api-usage.json`, forty need the join and thirty-six the quit.
//! Two events reach two thirds of that corpus, which is why the measurement
//! came before the design.

use glam::DVec3;
use std::sync::Arc;
use uuid::Uuid;

use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use text_components::TextComponent;

use super::Event;
use crate::player::Player;
use foton_utils::Identifier;

/// A player has died, before the death drops are processed.
pub struct PlayerDeathEvent {
    player_id: Uuid,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerDeathEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_death");
}
impl Event for PlayerDeathEvent {}
impl PlayerDeathEvent {
    pub const fn new(player_id: Uuid) -> Self { Self { player_id } }
    pub const fn player_id(&self) -> Uuid { self.player_id }
}

/// A player has completed protocol login and may enter the world.
pub struct PlayerLoginEvent {
    player: Arc<Player>,
    kick_message: Option<String>,
}

/// A player attempted to use an item or interact with the air/block.
pub struct PlayerInteractEvent {
    player_id: Uuid,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerInteractEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_interact");
}

impl Event for PlayerInteractEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerInteractEvent {
    #[must_use]
    /// Creates an uncancelled interaction event.
    pub const fn new(player_id: Uuid) -> Self {
        Self {
            player_id,
            cancelled: false,
        }
    }
    #[must_use]
    /// Returns the player's UUID.
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    #[must_use]
    /// Returns whether the interaction was cancelled.
    pub const fn cancelled(&self) -> bool {
        self.cancelled
    }
    /// Cancels or uncancels the interaction.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerLoginEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_login");
}

impl Event for PlayerLoginEvent {
    fn is_cancelled(&self) -> bool {
        self.kick_message.is_some()
    }
}

impl PlayerLoginEvent {
    #[must_use]
    /// Creates an allowed login event.
    pub const fn new(player: Arc<Player>) -> Self {
        Self {
            player,
            kick_message: None,
        }
    }
    #[must_use]
    /// Returns the logging-in player.
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }
    #[must_use]
    /// Returns the denial message, if admission was denied.
    pub fn kick_message(&self) -> Option<&str> {
        self.kick_message.as_deref()
    }
    /// Denies admission with a kick message.
    pub fn deny(&mut self, message: String) {
        self.kick_message = Some(message);
    }
}

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

/// A player movement that passed vanilla movement validation.
pub struct PlayerMoveEvent {
    player: Arc<Player>,
    from: DVec3,
    to: DVec3,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerMoveEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_move");
}

impl Event for PlayerMoveEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerMoveEvent {
    /// Creates an event for an accepted movement.
    #[must_use]
    pub const fn new(player: Arc<Player>, from: DVec3, to: DVec3) -> Self {
        Self {
            player,
            from,
            to,
            cancelled: false,
        }
    }
    /// Returns the moving player.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }
    /// Returns the starting position.
    #[must_use]
    pub const fn from(&self) -> DVec3 {
        self.from
    }
    /// Returns the destination selected by listeners.
    #[must_use]
    pub const fn to(&self) -> DVec3 {
        self.to
    }
    /// Changes the destination selected by listeners.
    pub const fn set_to(&mut self, to: DVec3) {
        self.to = to;
    }
    /// Cancels or uncancels the movement.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
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
