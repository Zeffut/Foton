//! Events about commands.

use std::sync::Arc;

use foton_utils::downcast::{DowncastType, DowncastTypeKey};

use super::Event;
use crate::player::Player;

/// Somebody typed a command, before the server tried to run it.
///
/// A listener that claims the command stops the server from parsing it at all.
/// That is what lets a plugin own a name the server has never heard of, which
/// is the only way a plugin command can work: Foton's dispatcher is a
/// Brigadier tree built at startup and a plugin's command is not in it.
///
/// Claiming a name the server *does* know shadows the built-in, so a listener
/// is expected to claim only what it was actually asked to own.
pub struct CommandEvent {
    player: Option<Arc<Player>>,
    command: String,
    handled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for CommandEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/command");
}

impl Event for CommandEvent {}

impl CommandEvent {
    /// Creates the event for one typed command, without its leading slash.
    #[must_use]
    pub fn new(player: Option<Arc<Player>>, command: impl Into<String>) -> Self {
        Self {
            player,
            command: command.into(),
            handled: false,
        }
    }

    /// Who typed it, when that was a player rather than the console.
    #[must_use]
    pub const fn player(&self) -> Option<&Arc<Player>> {
        self.player.as_ref()
    }

    /// The command line, without its leading slash.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Whether somebody has taken responsibility for running this.
    #[must_use]
    pub const fn is_handled(&self) -> bool {
        self.handled
    }

    /// Claims the command, so the server will not try to parse it.
    pub const fn set_handled(&mut self, handled: bool) {
        self.handled = handled;
    }
}
