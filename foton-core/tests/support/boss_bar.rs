//! A test player whose boss-bar packets can be read back.
//!
//! Both the boss-bar unit tests and the wither's tracking tests need the same
//! thing: a real `Player` whose connection keeps what it was sent, and a way to
//! ask which boss-event operations arrived.

use std::io::Cursor;
use std::mem;
use std::sync::Arc;

use foton_protocol::packet_traits::{CompressionInfo, EncodedPacket};
use foton_registry::packets::play::C_BOSS_EVENT;
use foton_utils::codec::VarInt;
use foton_utils::locks::SyncMutex;
use foton_utils::serial::ReadFrom as _;
use text_components::TextComponent;

use super::TestPlayerBuilder;
use crate::player::connection::NetworkConnection;
use crate::player::{Player, PlayerConnection};
use crate::world::World;

/// Vanilla `ClientboundBossEventPacket.OperationType` ordinals.
pub(crate) const OP_ADD: i32 = 0;
pub(crate) const OP_REMOVE: i32 = 1;
pub(crate) const OP_UPDATE_PROGRESS: i32 = 2;
pub(crate) const OP_UPDATE_STYLE: i32 = 4;

struct RecordingConnection {
    sent_packets: Arc<SyncMutex<Vec<EncodedPacket>>>,
}

impl NetworkConnection for RecordingConnection {
    fn compression(&self) -> Option<CompressionInfo> {
        None
    }

    fn send_encoded(&self, packet: EncodedPacket) {
        self.sent_packets.lock().push(packet);
    }

    fn send_encoded_bundle(&self, packets: Vec<EncodedPacket>) {
        self.sent_packets.lock().extend(packets);
    }

    fn disconnect_with_reason(&self, _reason: TextComponent) {}

    fn tick(&self) {}

    fn latency(&self) -> i32 {
        0
    }

    fn close(&self) {}

    fn closed(&self) -> bool {
        false
    }
}

/// A connected test player that remembers every packet it was sent.
pub(crate) struct BossBarViewer {
    pub(crate) player: Arc<Player>,
    sent: Arc<SyncMutex<Vec<EncodedPacket>>>,
}

impl BossBarViewer {
    /// Builds a client-loaded player in `world` with a recording connection.
    pub(crate) fn new(world: &Arc<World>, name: &'static str, entity_id: i32) -> Self {
        let sent = Arc::new(SyncMutex::new(Vec::new()));
        let connection = Arc::new(PlayerConnection::Other(Box::new(RecordingConnection {
            sent_packets: Arc::clone(&sent),
        })));
        let player = TestPlayerBuilder::new(Arc::clone(world), name, entity_id)
            .connection(connection)
            .build();
        Self { player, sent }
    }

    /// Returns the boss-event operations sent since the last call, and forgets
    /// every packet recorded so far.
    pub(crate) fn take_boss_operations(&self) -> Vec<i32> {
        let packets = mem::take(&mut *self.sent.lock());
        packets.iter().filter_map(boss_event_operation).collect()
    }
}

/// Returns the operation tag of a boss-event packet, or `None` for any other
/// packet.
fn boss_event_operation(packet: &EncodedPacket) -> Option<i32> {
    let mut cursor = Cursor::new(packet.encoded_data.as_slice());
    VarInt::read(&mut cursor).ok()?;
    if VarInt::read(&mut cursor).ok()?.0 != C_BOSS_EVENT {
        return None;
    }
    // The bar UUID is two big-endian longs; skip them to reach the tag.
    u64::read(&mut cursor).ok()?;
    u64::read(&mut cursor).ok()?;
    Some(VarInt::read(&mut cursor).ok()?.0)
}
