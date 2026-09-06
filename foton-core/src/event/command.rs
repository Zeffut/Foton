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

/// Somebody asked for tab completions, before the server computed any.
///
/// Vanilla has no equivalent; this is Paper's `AsyncTabCompleteEvent`, and it
/// exists so a plugin can complete a command the Brigadier tree has never heard
/// of -- the same reason [`CommandEvent`] exists. A listener that supplies
/// completions and marks the event handled replaces the server's answer
/// entirely; one that only adds to the list is ignored unless it says so.
///
/// Paper fires this off the main thread, which is what the `Async` in its name
/// promises. Foton fires it on the command-request phase of the tick instead.
/// The difference is visible to a listener that blocks: here that costs tick
/// time rather than nothing.
pub struct AsyncTabCompleteEvent {
    player: Option<Arc<Player>>,
    buffer: String,
    completions: Vec<String>,
    handled: bool,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for AsyncTabCompleteEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/async_tab_complete");
}

impl Event for AsyncTabCompleteEvent {}

impl AsyncTabCompleteEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player: Option<Arc<Player>>, buffer: String) -> Self {
        Self {
            player,
            buffer,
            completions: Vec::new(),
            handled: false,
            cancelled: false,
        }
    }

    /// Who is completing, when it was a player.
    #[must_use]
    pub const fn player(&self) -> Option<&Arc<Player>> {
        self.player.as_ref()
    }

    /// Everything typed so far, leading slash included.
    #[must_use]
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// The completions a listener supplied.
    #[must_use]
    pub fn completions(&self) -> &[String] {
        &self.completions
    }

    /// Replaces the completions a listener has supplied so far.
    pub fn set_completions(&mut self, completions: Vec<String>) {
        self.completions = completions;
    }

    /// Whether a listener means its completions to replace the server's.
    #[must_use]
    pub const fn is_handled(&self) -> bool {
        self.handled
    }

    /// Claims the completion, so the server does not compute its own.
    pub const fn set_handled(&mut self, handled: bool) {
        self.handled = handled;
    }

    /// Whether a listener refused the completion outright.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Refuses the completion; the player is offered nothing.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
