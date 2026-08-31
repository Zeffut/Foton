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
