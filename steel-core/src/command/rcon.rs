//! Output sink for a command an Rcon client asked for.
//!
//! Vanilla parity: `RconConsoleSource`. There, one long-lived source owns a
//! `StringBuffer`; `DedicatedServer.runCommand` clears it, runs the command
//! through `executeBlocking(...)` -- which parks the Rcon thread on the server
//! thread -- and reads the buffer back afterwards.
//!
//! A Steel game tick may never be waited on, so the shape is inverted: each
//! request gets its own sink, the Rcon task awaits a channel, and the reply is
//! sent when the last handle to the sink is dropped. That one moment covers
//! every way a command can end -- it completed, it failed to parse, it hit the
//! command limit, it overflowed the execution queue, it was cancelled because
//! the server is shutting down -- so no path can leave a client waiting
//! forever for a reply that is never coming.

use std::mem;
#[cfg(test)]
use std::sync::Arc;

use steel_utils::locks::SyncMutex;
use text_components::TextComponent;
use tokio::sync::oneshot;

/// Accumulates one Rcon command's output and delivers it once the command ends.
pub struct RconOutput {
    connection: u64,
    buffer: SyncMutex<String>,
    reply: SyncMutex<Option<oneshot::Sender<String>>>,
}

impl RconOutput {
    /// Creates a sink for one command and the receiver its reply arrives on.
    #[must_use]
    pub fn new(connection: u64) -> (Self, oneshot::Receiver<String>) {
        let (sender, receiver) = oneshot::channel();
        let output = Self {
            connection,
            buffer: SyncMutex::new(String::new()),
            reply: SyncMutex::new(Some(sender)),
        };
        (output, receiver)
    }

    /// Returns the connection this command came in on.
    ///
    /// Two commands from one client keep their order; two clients do not wait
    /// on each other.
    #[must_use]
    pub const fn connection(&self) -> u64 {
        self.connection
    }

    /// Appends one message to the pending reply.
    ///
    /// Vanilla parity: `RconConsoleSource.sendSystemMessage`, which appends
    /// `Component.getString()` with no separator of any kind. Two messages from
    /// one command therefore run together, which is what Rcon clients see from
    /// a vanilla server too.
    pub(crate) fn record(&self, text: &TextComponent) {
        use std::fmt::Write as _;

        let mut buffer = self.buffer.lock();
        // `Display` resolves translations through the global resolutor, which
        // is the same plain text the console branch logs.
        let _ = write!(buffer, "{text}");
    }
}

#[cfg(test)]
impl RconOutput {
    /// Builds a sink whose reply nobody is waiting for.
    pub(crate) fn for_test(connection: u64) -> Arc<Self> {
        Arc::new(Self::new(connection).0)
    }
}

impl Drop for RconOutput {
    fn drop(&mut self) {
        let Some(reply) = self.reply.get_mut().take() else {
            return;
        };
        let _ = reply.send(mem::take(self.buffer.get_mut()));
    }
}

#[cfg(test)]
mod tests {
    use super::{Arc, RconOutput};
    use tokio::sync::oneshot::error::TryRecvError;

    use text_components::TextComponent;

    #[test]
    fn output_is_delivered_when_the_last_handle_goes_away() {
        let (output, mut receiver) = RconOutput::new(0);
        let output = Arc::new(output);
        let held = Arc::clone(&output);
        output.record(&TextComponent::plain("first"));
        held.record(&TextComponent::plain("second"));

        drop(output);
        assert_eq!(
            receiver.try_recv(),
            Err(TryRecvError::Empty),
            "a surviving handle means the command is still running"
        );

        drop(held);
        assert_eq!(receiver.try_recv(), Ok("firstsecond".to_owned()));
    }

    #[test]
    fn a_command_that_says_nothing_still_answers() {
        let (output, mut receiver) = RconOutput::new(0);
        drop(output);
        assert_eq!(receiver.try_recv(), Ok(String::new()));
    }
}
