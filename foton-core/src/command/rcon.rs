//! Output sink for a command an Rcon client asked for.
//!
//! Vanilla parity: `RconConsoleSource`. There, one long-lived source owns a
//! `StringBuffer`; `DedicatedServer.runCommand` clears it, runs the command
//! through `executeBlocking(...)` -- which parks the Rcon thread on the server
//! thread -- and reads the buffer back afterwards.
//!
//! A Foton game tick may never be waited on, so the shape is inverted: each
//! request gets its own sink, the Rcon task awaits a channel, and the reply is
//! sent when the last handle to the sink is dropped. That one moment covers
//! every way a command can end -- it completed, it failed to parse, it hit the
//! command limit, it overflowed the execution queue, it was cancelled because
//! the server is shutting down -- so no path can leave a client waiting
//! forever for a reply that is never coming.

#[cfg(test)]
use std::sync::Arc;
use std::{fmt, mem};

use foton_utils::locks::SyncMutex;
use text_components::TextComponent;
use tokio::sync::oneshot;

/// Maximum command output retained and sent for one Rcon request.
pub const MAX_RCON_OUTPUT_BYTES: usize = 1024 * 1024;

struct BoundedOutputWriter<'a> {
    output: &'a mut PendingRconOutput,
}

impl fmt::Write for BoundedOutputWriter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.output.saturated {
            return Ok(());
        }

        let remaining = MAX_RCON_OUTPUT_BYTES.saturating_sub(self.output.buffer.len());
        let mut end = text.len().min(remaining);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.output.buffer.push_str(&text[..end]);
        self.output.saturated = end < text.len();
        Ok(())
    }
}

#[derive(Default)]
struct PendingRconOutput {
    buffer: String,
    saturated: bool,
}

/// Accumulates one Rcon command's output and delivers it once the command ends.
pub struct RconOutput {
    connection: u64,
    output: SyncMutex<PendingRconOutput>,
    reply: SyncMutex<Option<oneshot::Sender<String>>>,
}

impl RconOutput {
    /// Creates a sink for one command and the receiver its reply arrives on.
    #[must_use]
    pub fn new(connection: u64) -> (Self, oneshot::Receiver<String>) {
        let (sender, receiver) = oneshot::channel();
        let output = Self {
            connection,
            output: SyncMutex::new(PendingRconOutput::default()),
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

        let mut output = self.output.lock();
        // `Display` resolves translations through the global resolutor, which
        // is the same plain text the console branch logs.
        let _ = write!(
            BoundedOutputWriter {
                output: &mut output
            },
            "{text}"
        );
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
        let output = mem::take(self.output.get_mut());
        let _ = reply.send(output.buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::{Arc, MAX_RCON_OUTPUT_BYTES, RconOutput};
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

    #[test]
    fn output_is_bounded_across_multiple_fragments() {
        let (output, mut receiver) = RconOutput::new(0);
        let fragment = "x".repeat(MAX_RCON_OUTPUT_BYTES / 2 + 1);
        output.record(&TextComponent::plain(fragment.clone()));
        output.record(&TextComponent::plain(fragment));
        output.record(&TextComponent::plain("ignored tail"));

        drop(output);
        let response = receiver
            .try_recv()
            .expect("dropping the output should deliver its response");
        assert_eq!(response.len(), MAX_RCON_OUTPUT_BYTES);
        assert!(response.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn output_stays_a_continuous_prefix_after_utf8_boundary_truncation() {
        let (output, mut receiver) = RconOutput::new(0);
        output.record(&TextComponent::plain("x".repeat(MAX_RCON_OUTPUT_BYTES - 1)));
        output.record(&TextComponent::plain("é"));
        output.record(&TextComponent::plain("later text must not fill the gap"));

        drop(output);
        let response = receiver
            .try_recv()
            .expect("dropping the output should deliver its response");
        assert_eq!(response.len(), MAX_RCON_OUTPUT_BYTES - 1);
        assert!(response.bytes().all(|byte| byte == b'x'));
    }
}
