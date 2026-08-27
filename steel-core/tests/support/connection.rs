use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use steel_protocol::packet_traits::{CompressionInfo, EncodedPacket};
use steel_utils::locks::SyncMutex;
use text_components::TextComponent;

use crate::player::connection::NetworkConnection;

#[derive(Default)]
pub(crate) struct TestConnection {
    closed: AtomicBool,
}

impl NetworkConnection for TestConnection {
    fn compression(&self) -> Option<CompressionInfo> {
        None
    }

    fn send_encoded(&self, _packet: EncodedPacket) {}

    fn send_encoded_bundle(&self, _packets: Vec<EncodedPacket>) {}

    fn disconnect_with_reason(&self, _reason: TextComponent) {
        self.close();
    }

    fn tick(&self) {}

    fn latency(&self) -> i32 {
        0
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// A connection that keeps everything it is handed.
///
/// A broadcast only reaches a player through their connection, so a test that
/// has to see one records it here rather than trying to read it back off the
/// world.
pub(crate) struct RecordingConnection {
    sent: Arc<SyncMutex<Vec<EncodedPacket>>>,
    closed: AtomicBool,
}

impl RecordingConnection {
    /// Creates a connection that writes everything it is sent into `sent`.
    pub(crate) fn new(sent: Arc<SyncMutex<Vec<EncodedPacket>>>) -> Self {
        Self {
            sent,
            closed: AtomicBool::new(false),
        }
    }
}

impl NetworkConnection for RecordingConnection {
    fn compression(&self) -> Option<CompressionInfo> {
        None
    }

    fn send_encoded(&self, packet: EncodedPacket) {
        self.sent.lock().push(packet);
    }

    fn send_encoded_bundle(&self, packets: Vec<EncodedPacket>) {
        self.sent.lock().extend(packets);
    }

    fn disconnect_with_reason(&self, _reason: TextComponent) {
        self.close();
    }

    fn tick(&self) {}

    fn latency(&self) -> i32 {
        0
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}
