//! Raw protocol traffic, offered to outside code before Foton acts on it.
//!
//! Packet libraries such as `PacketEvents` sit between the socket and the game:
//! every packet a client sends is shown to them before the server handles it,
//! and every packet the server sends is shown to them before it is written.
//! They can drop it or rewrite it. That is the whole contract, and it is a
//! byte-level one on purpose -- those libraries read the protocol themselves,
//! so handing them Foton's typed structs would mean translating twice.
//!
//! Nothing here knows about plugins. A tap is installed by whoever wants one,
//! and a server without one pays a single relaxed atomic load per packet.
//!
//! Threading: a tap is called on the connection's own network tasks, which is
//! where a Netty-based server calls the same libraries. It must not block, and
//! it must not assume it is on the tick thread.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use foton_utils::locks::SyncRwLock;
use uuid::Uuid;

/// The protocol phase a packet belongs to. Ids are only unique within one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapPhase {
    /// Between login and play, where registries and resource packs are sent.
    Configuration,
    /// The game itself.
    Play,
}

/// What a tap decided about one packet.
#[derive(Debug, PartialEq, Eq)]
pub enum TapVerdict {
    /// Let the packet through untouched.
    Pass,
    /// Drop it. Inbound, the server never handles it; outbound, the client
    /// never receives it.
    Cancel,
    /// Replace its payload with these bytes. The id is unchanged.
    Rewrite(Vec<u8>),
}

/// What a tap decided about one outbound packet.
#[derive(Debug, PartialEq, Eq)]
pub struct TapOutcome {
    /// Whether and how the packet goes out.
    pub verdict: TapVerdict,
    /// Whether [`PacketTap::sent`] must be called once the packet has been
    /// written, which is when "after send" work may run.
    pub after_send: bool,
}

impl TapOutcome {
    /// The packet goes out as it was, with nothing to run afterwards.
    #[must_use]
    pub const fn pass() -> Self {
        Self {
            verdict: TapVerdict::Pass,
            after_send: false,
        }
    }
}

/// Outside code that sees raw packets.
///
/// Connections are named by the id Foton gave them at accept time, which is
/// stable from configuration through play and is never reused while the
/// server runs.
pub trait PacketTap: Send + Sync {
    /// A client finished logging in and entered configuration.
    fn opened(&self, connection: u64, profile: Uuid, name: &str, address: SocketAddr);
    /// The client entered play and was given this entity id.
    fn playing(&self, connection: u64, player: Uuid, entity_id: i32);
    /// The connection is gone. No call for it follows.
    fn closed(&self, connection: u64);
    /// A packet the client sent, before Foton handles it.
    fn inbound(&self, connection: u64, phase: TapPhase, id: i32, payload: &[u8]) -> TapVerdict;
    /// A packet Foton is about to write to the client.
    fn outbound(&self, connection: u64, phase: TapPhase, id: i32, payload: &[u8]) -> TapOutcome;
    /// A packet whose outcome asked for it has been written.
    fn sent(&self, connection: u64);
}

/// The installed tap, if any, and which outbound packets it is spared.
pub struct PacketTaps {
    installed: AtomicBool,
    tap: SyncRwLock<Option<Arc<dyn PacketTap>>>,
    /// Play-phase clientbound ids the tap is not shown.
    ///
    /// A clientbound packet reaches the tap already framed and, above the
    /// compression threshold, compressed -- broadcasts are encoded once for
    /// every recipient. Showing one to a tap costs an inflate, a copy across
    /// the boundary and, if rewritten, a deflate. For chunk data that is most
    /// of the bandwidth a server has, so the tap's owner may decline it.
    skipped_outbound: [AtomicU64; 4],
}

impl Default for PacketTaps {
    fn default() -> Self {
        Self::new()
    }
}

impl PacketTaps {
    /// No tap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            installed: AtomicBool::new(false),
            tap: SyncRwLock::new(None),
            skipped_outbound: [const { AtomicU64::new(0) }; 4],
        }
    }

    /// Installs a tap, replacing any previous one.
    pub fn install(&self, tap: Arc<dyn PacketTap>) {
        *self.tap.write() = Some(tap);
        self.installed.store(true, Ordering::Release);
    }

    /// Removes the tap. Packets already inside it finish normally.
    pub fn uninstall(&self) {
        self.installed.store(false, Ordering::Release);
        *self.tap.write() = None;
    }

    /// The tap, when one is installed.
    #[must_use]
    pub fn current(&self) -> Option<Arc<dyn PacketTap>> {
        if !self.installed.load(Ordering::Acquire) {
            return None;
        }
        self.tap.read().clone()
    }

    /// Spares the tap a play-phase clientbound packet id, or stops sparing it.
    ///
    /// Ids outside `0..256` are ignored: the protocol has fewer than that.
    pub fn set_outbound_skipped(&self, id: i32, skipped: bool) {
        let Some((word, bit)) = Self::slot(id) else {
            return;
        };
        if skipped {
            self.skipped_outbound[word].fetch_or(bit, Ordering::Relaxed);
        } else {
            self.skipped_outbound[word].fetch_and(!bit, Ordering::Relaxed);
        }
    }

    /// Whether a play-phase clientbound id is kept from the tap.
    #[must_use]
    pub fn skips_outbound(&self, id: i32) -> bool {
        Self::slot(id).is_some_and(|(word, bit)| {
            self.skipped_outbound[word].load(Ordering::Relaxed) & bit != 0
        })
    }

    fn slot(id: i32) -> Option<(usize, u64)> {
        let id = usize::try_from(id).ok().filter(|id| *id < 256)?;
        Some((id / 64, 1 << (id % 64)))
    }
}

#[cfg(test)]
mod tests {
    use super::PacketTaps;

    #[test]
    fn skipping_one_id_spares_no_neighbour() {
        let taps = PacketTaps::new();
        taps.set_outbound_skipped(45, true);
        taps.set_outbound_skipped(64, true);
        assert!(taps.skips_outbound(45));
        assert!(taps.skips_outbound(64));
        assert!(!taps.skips_outbound(44));
        assert!(!taps.skips_outbound(46));
        assert!(!taps.skips_outbound(63));
        taps.set_outbound_skipped(45, false);
        assert!(!taps.skips_outbound(45));
        assert!(!taps.skips_outbound(-1));
        assert!(!taps.skips_outbound(300));
    }
}
