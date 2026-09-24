//! This module contains the `JavaConnection` struct, which is used to represent a connection to a Java client.
use std::future::Future;
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::vec::IntoIter;

use foton_protocol::packet_reader::TCPNetworkDecoder;
use foton_protocol::packet_traits::{
    ClientPacket, CompressionInfo, EncodedPacket, PacketDirection, PacketTranslation, ServerPacket,
    TranslationBatch,
};
use foton_protocol::packet_writer::TCPNetworkEncoder;
use foton_protocol::packets::common::{
    CDisconnect, CKeepAlive, CPongResponse, SClientInformation, SCustomClickAction, SCustomPayload,
    SKeepAlive, SPingRequest,
};
use foton_protocol::packets::game::{
    CBundleDelimiter, CCommandSuggestions, ClientCommandAction, PlayerAction, PlayerCommandAction,
    SAcceptTeleportation, SAttack, SChangeDifficulty, SChangeGameMode, SChat, SChatAck,
    SChatCommand, SChatSessionUpdate, SChunkBatchReceived, SClientCommand, SClientTickEnd,
    SCommandSuggestion, SContainerButtonClick, SContainerClick, SContainerClose,
    SContainerSlotStateChanged, SEditBook, SInteract, SJigsawGenerate, SMovePlayer, SMovePlayerPos,
    SMovePlayerPosRot, SMovePlayerRot, SMovePlayerStatusOnly, SMoveVehicle, SPickItemFromBlock,
    SPlayerAbilities, SPlayerAction, SPlayerCommand, SPlayerInput, SPlayerLoad, SRenameItem,
    SSeenAdvancements, SSelectBundleItem, SSelectTrade, SSetBeacon, SSetCarriedItem,
    SSetCommandBlock, SSetCommandMinecart, SSetCreativeModeSlot, SSetJigsawBlock,
    SSetStructureBlock, SSignUpdate, SSpectatorAction, SSwing, SUseItem, SUseItemOn,
};

use foton_protocol::utils::{ConnectionProtocol, MAX_PACKET_DATA_SIZE, PacketError, RawPacket};
use foton_registry::packets::play;
use foton_utils::locks::{AsyncMutex, SyncMutex};
use foton_utils::translations;
use text_components::content::Resolvable;
use text_components::custom::CustomData;
use text_components::resolving::TextResolutor;
use text_components::{Modifier, TextComponent, format::Color};
use tokio::io::{BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::select;
use tokio::sync::{
    OwnedSemaphorePermit, Semaphore, TryAcquireError,
    mpsc::{self, Receiver, Sender, error::TryRecvError},
};
use tokio::time::{MissedTickBehavior, interval, timeout};
use tokio_util::sync::CancellationToken;

use crate::command::{handle_client_request, sender::CommandSender};
use crate::event::PlayerClientLoadedWorldEvent;
use crate::event::PlayerCommandPreprocessEvent;
use crate::player::Player;
use crate::player::chat::is_chat_message_illegal;
use crate::player::connection::NetworkConnection;
use crate::server::Server;

/// Shared Java socket writer.
type JavaNetworkEncoder = TCPNetworkEncoder<BufWriter<OwnedWriteHalf>>;
/// Shared, closeable writer for one Java Edition socket.
pub type JavaNetworkWriter = Arc<AsyncMutex<Option<JavaNetworkEncoder>>>;

/// Outbound packet queue message for Java connections.
pub enum OutboundPacket {
    /// Normal packet write that may be interrupted by connection shutdown.
    Packet(EncodedPacket),
    /// Final disconnect packet that must be flushed before closing the socket.
    Disconnect(EncodedPacket),
}

/// Maximum encoded bytes retained while a Java peer is not reading.
const OUTBOUND_PACKET_QUEUE_BYTE_BUDGET: usize = MAX_PACKET_DATA_SIZE * 2;

/// Secondary bound for empty and tiny reliable packets, whose allocation overhead is not encoded bytes.
const OUTBOUND_PACKET_QUEUE_ENTRY_CAPACITY: usize = 4096;

/// Entry bound for atomic chunk batches.
const OUTBOUND_CHUNK_QUEUE_ENTRY_CAPACITY: usize = 256;

/// Queue entries kept outside the chunk-stream budget for control traffic.
const OUTBOUND_CONTROL_ENTRY_RESERVE: usize = 16;

/// Envelope slots covering every possible lane admission.
const OUTBOUND_ENVELOPE_QUEUE_CAPACITY: usize =
    OUTBOUND_PACKET_QUEUE_ENTRY_CAPACITY + OUTBOUND_CHUNK_QUEUE_ENTRY_CAPACITY;

/// Process-wide ceiling for retained packet entries, including tiny packets.
const GLOBAL_OUTBOUND_ENTRY_BUDGET: usize = OUTBOUND_PACKET_QUEUE_ENTRY_CAPACITY * 16;

/// Global entries inaccessible to ordinary and chunk traffic.
const GLOBAL_OUTBOUND_CONTROL_ENTRY_RESERVE: usize = OUTBOUND_CONTROL_ENTRY_RESERVE * 16;

/// Encoded bytes kept outside the chunk-stream budget for small control traffic.
const OUTBOUND_CONTROL_BYTE_RESERVE: usize = 64 * 1024;

/// Encoded bytes kept outside the chunk-stream budget for reliable gameplay packets.
const OUTBOUND_RELIABLE_BYTE_RESERVE: usize = 1024 * 1024;

/// Process-wide ceiling across every Java connection.
const GLOBAL_OUTBOUND_PACKET_BYTE_BUDGET: usize = MAX_PACKET_DATA_SIZE * 32;

/// Global capacity unavailable to bulk and ordinary gameplay packets.
const GLOBAL_OUTBOUND_CONTROL_BYTE_RESERVE: usize = MAX_PACKET_DATA_SIZE * 4;

/// Global capacity unavailable to chunk batches, but available to reliable gameplay packets.
const GLOBAL_OUTBOUND_RELIABLE_BYTE_RESERVE: usize = MAX_PACKET_DATA_SIZE * 2;

/// Maximum time a socket write may keep a connection task alive.
const NETWORK_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// A final disconnect is best effort: the connection is closing either way.
const FINAL_DISCONNECT_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// Vanilla sends a keep-alive every fifteen seconds and times out the next one.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

async fn with_disconnect_write_deadline<F>(deadline: Duration, write: F) -> Result<(), PacketError>
where
    F: Future<Output = Result<(), PacketError>>,
{
    match timeout(deadline, write).await {
        Ok(result) => result,
        Err(_) => Err(PacketError::SendError(format!(
            "final disconnect write exceeded its {} ms deadline",
            deadline.as_millis()
        ))),
    }
}

/// Runs a final disconnect write with the connection shutdown deadline.
pub async fn write_final_disconnect<F>(write: F) -> Result<(), PacketError>
where
    F: Future<Output = Result<(), PacketError>>,
{
    with_disconnect_write_deadline(FINAL_DISCONNECT_WRITE_TIMEOUT, write).await
}

/// Bounded sender for a Java connection's outbound packets.
#[derive(Clone)]
pub struct OutboundPacketSender {
    sender: Sender<QueuedOutboundEnvelope>,
    control_sender: Sender<QueuedOutboundEnvelope>,
    budgets: Arc<OutboundPacketBudgets>,
}

struct OutboundPacketBudgets {
    reliable_entry_budget: Arc<Semaphore>,
    chunk_entry_budget: Arc<Semaphore>,
    control_entry_budget: Arc<Semaphore>,
    byte_budget: Arc<Semaphore>,
    non_control_byte_budget: Arc<Semaphore>,
    chunk_byte_budget: Arc<Semaphore>,
    chunk_byte_capacity: usize,
    global_budget: Arc<GlobalOutboundBudget>,
}

struct GlobalOutboundBudget {
    entry_budget: Arc<Semaphore>,
    non_control_entry_budget: Arc<Semaphore>,
    byte_budget: Arc<Semaphore>,
    non_control_byte_budget: Arc<Semaphore>,
    chunk_byte_budget: Arc<Semaphore>,
}

static GLOBAL_OUTBOUND_BUDGET: LazyLock<Arc<GlobalOutboundBudget>> = LazyLock::new(|| {
    Arc::new(GlobalOutboundBudget {
        entry_budget: Arc::new(Semaphore::new(GLOBAL_OUTBOUND_ENTRY_BUDGET)),
        non_control_entry_budget: Arc::new(Semaphore::new(
            GLOBAL_OUTBOUND_ENTRY_BUDGET - GLOBAL_OUTBOUND_CONTROL_ENTRY_RESERVE,
        )),
        byte_budget: Arc::new(Semaphore::new(GLOBAL_OUTBOUND_PACKET_BYTE_BUDGET)),
        non_control_byte_budget: Arc::new(Semaphore::new(
            GLOBAL_OUTBOUND_PACKET_BYTE_BUDGET - GLOBAL_OUTBOUND_CONTROL_BYTE_RESERVE,
        )),
        chunk_byte_budget: Arc::new(Semaphore::new(
            GLOBAL_OUTBOUND_PACKET_BYTE_BUDGET
                - GLOBAL_OUTBOUND_CONTROL_BYTE_RESERVE
                - GLOBAL_OUTBOUND_RELIABLE_BYTE_RESERVE,
        )),
    })
});

struct PacketBatchBudget {
    _entry_budget: OwnedSemaphorePermit,
    _global_entry_budget: OwnedSemaphorePermit,
    _global_non_control_entry_budget: OwnedSemaphorePermit,
    _byte_budget: OwnedSemaphorePermit,
    _non_control_byte_budget: OwnedSemaphorePermit,
    _chunk_byte_budget: Option<OwnedSemaphorePermit>,
    _global_byte_budget: OwnedSemaphorePermit,
    _global_non_control_byte_budget: OwnedSemaphorePermit,
    _global_chunk_byte_budget: Option<OwnedSemaphorePermit>,
}

enum QueuedPacketBudget {
    Normal {
        _entry_budget: OwnedSemaphorePermit,
        _global_entry_budget: OwnedSemaphorePermit,
        _global_non_control_entry_budget: OwnedSemaphorePermit,
        _byte_budget: OwnedSemaphorePermit,
        _non_control_byte_budget: OwnedSemaphorePermit,
        _global_byte_budget: OwnedSemaphorePermit,
        _global_non_control_byte_budget: OwnedSemaphorePermit,
    },
    Control {
        _entry_budget: OwnedSemaphorePermit,
        _global_entry_budget: OwnedSemaphorePermit,
        _byte_budget: OwnedSemaphorePermit,
        _global_byte_budget: OwnedSemaphorePermit,
    },
    Batch {
        _budget: Arc<PacketBatchBudget>,
    },
}

pub(crate) enum QueuedOutboundEnvelope {
    Single(QueuedOutboundPacket),
    Batch(Vec<QueuedOutboundPacket>),
}

/// A queued packet together with the encoded-byte budget it owns.
///
/// The permit remains alive while the socket write borrows this value, so a
/// slow writer cannot free queue capacity before it releases the packet bytes.
pub struct QueuedOutboundPacket {
    packet: OutboundPacket,
    _budget: QueuedPacketBudget,
}

impl QueuedOutboundPacket {
    /// Returns the encoded packet and whether the connection closes after it is written.
    #[must_use]
    pub const fn write_parts(&self) -> (&EncodedPacket, bool) {
        match &self.packet {
            OutboundPacket::Packet(packet) => (packet, false),
            OutboundPacket::Disconnect(packet) => (packet, true),
        }
    }
}

/// Receiver half of a Java connection's byte-budgeted outbound queue.
pub struct OutboundPacketReceiver {
    envelopes: Receiver<QueuedOutboundEnvelope>,
    control_envelopes: Receiver<QueuedOutboundEnvelope>,
    active_batch: IntoIter<QueuedOutboundPacket>,
    control_was_selected: bool,
}

impl OutboundPacketReceiver {
    /// Receives one wire-indivisible envelope.
    ///
    /// At most one control envelope overtakes the data FIFO before data gets
    /// another turn. Batch contents never participate in this selection.
    pub(crate) async fn recv_envelope(&mut self) -> Option<QueuedOutboundEnvelope> {
        if let Ok(envelope) = self.try_recv_envelope() {
            return Some(envelope);
        }
        if self.control_was_selected {
            return select! {
                biased;
                Some(envelope) = self.envelopes.recv() => {
                    self.control_was_selected = false;
                    Some(envelope)
                }
                Some(envelope) = self.control_envelopes.recv() => Some(envelope),
                else => None,
            };
        }
        select! {
            biased;
            Some(envelope) = self.control_envelopes.recv() => {
                self.control_was_selected = true;
                Some(envelope)
            }
            Some(envelope) = self.envelopes.recv() => Some(envelope),
            else => None,
        }
    }

    fn try_recv_envelope(&mut self) -> Result<QueuedOutboundEnvelope, TryRecvError> {
        if self.control_was_selected {
            match self.envelopes.try_recv() {
                Ok(envelope) => {
                    self.control_was_selected = false;
                    Ok(envelope)
                }
                Err(data_error) => match self.control_envelopes.try_recv() {
                    Ok(envelope) => Ok(envelope),
                    Err(control_error) => {
                        Err(Self::combined_empty_error(data_error, control_error))
                    }
                },
            }
        } else {
            match self.control_envelopes.try_recv() {
                Ok(envelope) => {
                    self.control_was_selected = true;
                    Ok(envelope)
                }
                Err(control_error) => match self.envelopes.try_recv() {
                    Ok(envelope) => Ok(envelope),
                    Err(data_error) => Err(Self::combined_empty_error(data_error, control_error)),
                },
            }
        }
    }

    const fn combined_empty_error(
        data_error: TryRecvError,
        control_error: TryRecvError,
    ) -> TryRecvError {
        if matches!(data_error, TryRecvError::Disconnected)
            && matches!(control_error, TryRecvError::Disconnected)
        {
            TryRecvError::Disconnected
        } else {
            TryRecvError::Empty
        }
    }

    /// Receives the next queued packet.
    pub async fn recv(&mut self) -> Option<QueuedOutboundPacket> {
        if let Some(packet) = self.active_batch.next() {
            return Some(packet);
        }
        loop {
            let envelope = self.recv_envelope().await?;
            match envelope {
                QueuedOutboundEnvelope::Single(packet) => return Some(packet),
                QueuedOutboundEnvelope::Batch(packets) => {
                    self.active_batch = packets.into_iter();
                    if let Some(packet) = self.active_batch.next() {
                        return Some(packet);
                    }
                }
            }
        }
    }

    /// Attempts to receive a packet without waiting.
    pub fn try_recv(&mut self) -> Result<QueuedOutboundPacket, TryRecvError> {
        if let Some(packet) = self.active_batch.next() {
            return Ok(packet);
        }
        loop {
            match self.try_recv_envelope()? {
                QueuedOutboundEnvelope::Single(packet) => return Ok(packet),
                QueuedOutboundEnvelope::Batch(packets) => {
                    self.active_batch = packets.into_iter();
                    if let Some(packet) = self.active_batch.next() {
                        return Ok(packet);
                    }
                }
            }
        }
    }
}

/// Creates the bounded outbound channel shared by login and play.
#[must_use]
pub fn outbound_packet_channel() -> (OutboundPacketSender, OutboundPacketReceiver) {
    outbound_packet_channel_with_budget(OUTBOUND_PACKET_QUEUE_BYTE_BUDGET)
}

fn outbound_packet_channel_with_budget(
    byte_budget: usize,
) -> (OutboundPacketSender, OutboundPacketReceiver) {
    outbound_packet_channel_with_budgets(byte_budget, Arc::clone(&GLOBAL_OUTBOUND_BUDGET))
}

fn outbound_packet_channel_with_budgets(
    byte_budget: usize,
    global_budget: Arc<GlobalOutboundBudget>,
) -> (OutboundPacketSender, OutboundPacketReceiver) {
    let (sender, receiver) = mpsc::channel(OUTBOUND_ENVELOPE_QUEUE_CAPACITY);
    let (control_sender, control_receiver) = mpsc::channel(OUTBOUND_CONTROL_ENTRY_RESERVE);
    let control_byte_reserve = OUTBOUND_CONTROL_BYTE_RESERVE.min(byte_budget / 4);
    let non_control_byte_capacity = byte_budget.saturating_sub(control_byte_reserve);
    let reliable_byte_reserve = OUTBOUND_RELIABLE_BYTE_RESERVE.min(non_control_byte_capacity / 4);
    let chunk_byte_capacity = non_control_byte_capacity.saturating_sub(reliable_byte_reserve);
    (
        OutboundPacketSender {
            sender,
            control_sender,
            budgets: Arc::new(OutboundPacketBudgets {
                reliable_entry_budget: Arc::new(Semaphore::new(
                    OUTBOUND_PACKET_QUEUE_ENTRY_CAPACITY,
                )),
                chunk_entry_budget: Arc::new(Semaphore::new(OUTBOUND_CHUNK_QUEUE_ENTRY_CAPACITY)),
                control_entry_budget: Arc::new(Semaphore::new(OUTBOUND_CONTROL_ENTRY_RESERVE)),
                byte_budget: Arc::new(Semaphore::new(byte_budget)),
                non_control_byte_budget: Arc::new(Semaphore::new(non_control_byte_capacity)),
                chunk_byte_budget: Arc::new(Semaphore::new(chunk_byte_capacity)),
                chunk_byte_capacity,
                global_budget,
            }),
        },
        OutboundPacketReceiver {
            envelopes: receiver,
            control_envelopes: control_receiver,
            active_batch: Vec::new().into_iter(),
            control_was_selected: false,
        },
    )
}

impl OutboundPacketSender {
    /// Attempts to enqueue a packet without waiting for queue capacity.
    #[expect(
        clippy::too_many_lines,
        reason = "all reliable-lane permit acquisitions and their exact rollback values stay together so no byte or entry budget can be omitted"
    )]
    pub fn try_send(
        &self,
        packet: OutboundPacket,
    ) -> Result<(), mpsc::error::TrySendError<OutboundPacket>> {
        let encoded_bytes = packet.encoded_packet().encoded_data.len().max(1);
        let Ok(encoded_bytes) = u32::try_from(encoded_bytes) else {
            return Err(mpsc::error::TrySendError::Full(packet));
        };
        let entry_budget = match Arc::clone(&self.budgets.reliable_entry_budget).try_acquire_owned()
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packet));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packet));
            }
        };
        let global_entry_budget =
            match Arc::clone(&self.budgets.global_budget.entry_budget).try_acquire_owned() {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packet));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packet));
                }
            };
        let global_non_control_entry_budget =
            match Arc::clone(&self.budgets.global_budget.non_control_entry_budget)
                .try_acquire_owned()
            {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packet));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packet));
                }
            };
        let byte_budget =
            match Arc::clone(&self.budgets.byte_budget).try_acquire_many_owned(encoded_bytes) {
                Ok(byte_budget) => byte_budget,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packet));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packet));
                }
            };
        let non_control_byte_budget = match Arc::clone(&self.budgets.non_control_byte_budget)
            .try_acquire_many_owned(encoded_bytes)
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packet));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packet));
            }
        };
        let global_byte_budget = match Arc::clone(&self.budgets.global_budget.byte_budget)
            .try_acquire_many_owned(encoded_bytes)
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packet));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packet));
            }
        };
        let global_non_control_byte_budget =
            match Arc::clone(&self.budgets.global_budget.non_control_byte_budget)
                .try_acquire_many_owned(encoded_bytes)
            {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packet));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packet));
                }
            };
        let channel_permit = match self.sender.try_reserve() {
            Ok(permit) => permit,
            Err(mpsc::error::TrySendError::Full(())) => {
                return Err(mpsc::error::TrySendError::Full(packet));
            }
            Err(mpsc::error::TrySendError::Closed(())) => {
                return Err(mpsc::error::TrySendError::Closed(packet));
            }
        };
        channel_permit.send(QueuedOutboundEnvelope::Single(QueuedOutboundPacket {
            packet,
            _budget: QueuedPacketBudget::Normal {
                _entry_budget: entry_budget,
                _global_entry_budget: global_entry_budget,
                _global_non_control_entry_budget: global_non_control_entry_budget,
                _byte_budget: byte_budget,
                _non_control_byte_budget: non_control_byte_budget,
                _global_byte_budget: global_byte_budget,
                _global_non_control_byte_budget: global_non_control_byte_budget,
            },
        }));
        Ok(())
    }

    fn try_send_control(
        &self,
        packet: OutboundPacket,
    ) -> Result<(), mpsc::error::TrySendError<OutboundPacket>> {
        let encoded_bytes = packet.encoded_packet().encoded_data.len().max(1);
        let Ok(encoded_bytes) = u32::try_from(encoded_bytes) else {
            return Err(mpsc::error::TrySendError::Full(packet));
        };
        let entry_budget = match Arc::clone(&self.budgets.control_entry_budget).try_acquire_owned()
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packet));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packet));
            }
        };
        let global_entry_budget =
            match Arc::clone(&self.budgets.global_budget.entry_budget).try_acquire_owned() {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packet));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packet));
                }
            };
        let byte_budget =
            match Arc::clone(&self.budgets.byte_budget).try_acquire_many_owned(encoded_bytes) {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packet));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packet));
                }
            };
        let global_byte_budget = match Arc::clone(&self.budgets.global_budget.byte_budget)
            .try_acquire_many_owned(encoded_bytes)
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packet));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packet));
            }
        };
        let channel_permit = match self.control_sender.try_reserve() {
            Ok(permit) => permit,
            Err(mpsc::error::TrySendError::Full(())) => {
                return Err(mpsc::error::TrySendError::Full(packet));
            }
            Err(mpsc::error::TrySendError::Closed(())) => {
                return Err(mpsc::error::TrySendError::Closed(packet));
            }
        };
        channel_permit.send(QueuedOutboundEnvelope::Single(QueuedOutboundPacket {
            packet,
            _budget: QueuedPacketBudget::Control {
                _entry_budget: entry_budget,
                _global_entry_budget: global_entry_budget,
                _byte_budget: byte_budget,
                _global_byte_budget: global_byte_budget,
            },
        }));
        Ok(())
    }

    /// Attempts to admit an entire packet batch without blocking.
    #[expect(
        clippy::too_many_lines,
        reason = "keeping every atomic reservation in one scope makes rollback auditable"
    )]
    fn try_send_packet_batch(
        &self,
        packets: Vec<EncodedPacket>,
        lane: OutboundBatchLane,
    ) -> Result<(), mpsc::error::TrySendError<Vec<EncodedPacket>>> {
        if packets.is_empty() {
            return Ok(());
        }

        let packet_count = packets.len();
        let Ok(packet_count_u32) = u32::try_from(packet_count) else {
            return self.full_or_closed(packets);
        };
        let Some(encoded_bytes) = packets.iter().try_fold(0_usize, |total, packet| {
            total.checked_add(packet.encoded_data.len().max(1))
        }) else {
            return self.full_or_closed(packets);
        };
        let Ok(encoded_bytes_u32) = u32::try_from(encoded_bytes) else {
            return self.full_or_closed(packets);
        };

        let entry_budget = match lane {
            OutboundBatchLane::Reliable => Arc::clone(&self.budgets.reliable_entry_budget)
                .try_acquire_many_owned(packet_count_u32),
            OutboundBatchLane::Chunk => Arc::clone(&self.budgets.chunk_entry_budget)
                .try_acquire_many_owned(packet_count_u32),
        };
        let entry_budget = match entry_budget {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packets));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packets));
            }
        };
        let global_entry_budget = match Arc::clone(&self.budgets.global_budget.entry_budget)
            .try_acquire_many_owned(packet_count_u32)
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packets));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packets));
            }
        };
        let global_non_control_entry_budget =
            match Arc::clone(&self.budgets.global_budget.non_control_entry_budget)
                .try_acquire_many_owned(packet_count_u32)
            {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packets));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packets));
                }
            };
        let byte_budget =
            match Arc::clone(&self.budgets.byte_budget).try_acquire_many_owned(encoded_bytes_u32) {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packets));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packets));
                }
            };
        let non_control_byte_budget = match Arc::clone(&self.budgets.non_control_byte_budget)
            .try_acquire_many_owned(encoded_bytes_u32)
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packets));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packets));
            }
        };
        let global_byte_budget = match Arc::clone(&self.budgets.global_budget.byte_budget)
            .try_acquire_many_owned(encoded_bytes_u32)
        {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                return Err(mpsc::error::TrySendError::Full(packets));
            }
            Err(TryAcquireError::Closed) => {
                return Err(mpsc::error::TrySendError::Closed(packets));
            }
        };
        let global_non_control_byte_budget =
            match Arc::clone(&self.budgets.global_budget.non_control_byte_budget)
                .try_acquire_many_owned(encoded_bytes_u32)
            {
                Ok(permit) => permit,
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packets));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packets));
                }
            };
        let chunk_byte_budget = if lane == OutboundBatchLane::Chunk {
            match Arc::clone(&self.budgets.chunk_byte_budget)
                .try_acquire_many_owned(encoded_bytes_u32)
            {
                Ok(permit) => Some(permit),
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packets));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packets));
                }
            }
        } else {
            None
        };
        let global_chunk_byte_budget = if lane == OutboundBatchLane::Chunk {
            match Arc::clone(&self.budgets.global_budget.chunk_byte_budget)
                .try_acquire_many_owned(encoded_bytes_u32)
            {
                Ok(permit) => Some(permit),
                Err(TryAcquireError::NoPermits) => {
                    return Err(mpsc::error::TrySendError::Full(packets));
                }
                Err(TryAcquireError::Closed) => {
                    return Err(mpsc::error::TrySendError::Closed(packets));
                }
            }
        } else {
            None
        };
        let channel_permit = match self.sender.try_reserve() {
            Ok(permit) => permit,
            Err(mpsc::error::TrySendError::Full(())) => {
                return Err(mpsc::error::TrySendError::Full(packets));
            }
            Err(mpsc::error::TrySendError::Closed(())) => {
                return Err(mpsc::error::TrySendError::Closed(packets));
            }
        };
        let budget = Arc::new(PacketBatchBudget {
            _entry_budget: entry_budget,
            _global_entry_budget: global_entry_budget,
            _global_non_control_entry_budget: global_non_control_entry_budget,
            _byte_budget: byte_budget,
            _non_control_byte_budget: non_control_byte_budget,
            _chunk_byte_budget: chunk_byte_budget,
            _global_byte_budget: global_byte_budget,
            _global_non_control_byte_budget: global_non_control_byte_budget,
            _global_chunk_byte_budget: global_chunk_byte_budget,
        });

        let queued_packets = packets
            .into_iter()
            .map(|packet| QueuedOutboundPacket {
                packet: OutboundPacket::Packet(packet),
                _budget: QueuedPacketBudget::Batch {
                    _budget: Arc::clone(&budget),
                },
            })
            .collect();
        channel_permit.send(QueuedOutboundEnvelope::Batch(queued_packets));
        Ok(())
    }

    pub(crate) fn try_send_chunk_batch(
        &self,
        packets: Vec<EncodedPacket>,
    ) -> Result<(), mpsc::error::TrySendError<Vec<EncodedPacket>>> {
        self.try_send_packet_batch(packets, OutboundBatchLane::Chunk)
    }

    fn full_or_closed(
        &self,
        packets: Vec<EncodedPacket>,
    ) -> Result<(), mpsc::error::TrySendError<Vec<EncodedPacket>>> {
        if self.sender.is_closed() {
            Err(mpsc::error::TrySendError::Closed(packets))
        } else {
            Err(mpsc::error::TrySendError::Full(packets))
        }
    }

    fn chunk_batch_byte_capacity(&self) -> usize {
        self.budgets.chunk_byte_capacity
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OutboundBatchLane {
    Reliable,
    Chunk,
}

impl OutboundPacket {
    const fn encoded_packet(&self) -> &EncodedPacket {
        match self {
            Self::Packet(packet) | Self::Disconnect(packet) => packet,
        }
    }
}

/// A decoded play packet whose handler runs in the server's inter-tick packet phase.
pub(crate) struct ScheduledPlayPacket(ScheduledPlayPacketKind);

/// Cross-player concurrency permitted for a scheduled packet handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScheduledPacketExecution {
    /// The handler may overlap handlers for other players, but never its own player lane.
    ///
    /// Shared mutations must be fully linearized by their resource locks, and the handler must
    /// tolerate cross-player execution order differing from packet submission order.
    PlayerLocal,
    /// The handler may overlap player-local work, but not another serialized handler. Serialized
    /// handlers start in global packet submission order.
    Serialized,
    /// The handler is a global submission-order barrier and must not overlap scheduled work.
    Exclusive,
}

enum ScheduledPlayPacketKind {
    PingRequest(SPingRequest),
    AcceptTeleportation(SAcceptTeleportation),
    Attack(SAttack),
    Interact(SInteract),
    CustomClickAction(SCustomClickAction),
    CustomPayload(SCustomPayload),
    Chat(Box<SChat>),
    ChatAck(SChatAck),
    ChatSessionUpdate(SChatSessionUpdate),
    ClientInformation(SClientInformation),
    ClientTickEnd,
    MovePlayer(SMovePlayer),
    MoveVehicle(SMoveVehicle),
    PlayerLoaded,
    ChatCommand(SChatCommand),
    CommandSuggestion(SCommandSuggestion),
    ContainerButtonClick(SContainerButtonClick),
    ContainerClick(SContainerClick),
    ContainerClose(SContainerClose),
    ContainerSlotStateChanged(SContainerSlotStateChanged),
    EditBook(SEditBook),
    SelectBundleItem(SSelectBundleItem),
    SelectTrade(SSelectTrade),
    SetBeacon(SSetBeacon),
    SetCommandBlock(SSetCommandBlock),
    SetCommandMinecart(SSetCommandMinecart),
    SetJigsawBlock(SSetJigsawBlock),
    JigsawGenerate(SJigsawGenerate),
    SetStructureBlock(Box<SSetStructureBlock>),
    SetCreativeModeSlot(SSetCreativeModeSlot),
    PlayerInput(SPlayerInput),
    PlayerCommand(SPlayerCommand),
    PlayerAbilities(SPlayerAbilities),
    RenameItem(SRenameItem),
    UseItemOn(SUseItemOn),
    UseItem(SUseItem),
    SetCarriedItem(SSetCarriedItem),
    Swing(SSwing),
    PlayerAction(SPlayerAction),
    PickItemFromBlock(SPickItemFromBlock),
    SignUpdate(SSignUpdate),
    SpectatorAction(SSpectatorAction),
    ClientCommand(SClientCommand),
    SeenAdvancements(SSeenAdvancements),
    ChangeGameMode(SChangeGameMode),
    ChangeDifficulty(SChangeDifficulty),
}

enum ImmediatePlayPacket {
    KeepAlive(SKeepAlive),
    ChunkBatchReceived(SChunkBatchReceived),
    Unknown(i32),
}

enum DecodedPlayPacket {
    Scheduled(ScheduledPlayPacket),
    Immediate(ImmediatePlayPacket),
}

impl ScheduledPlayPacket {
    /// Returns whether this packet acknowledges target-world synchronization.
    pub(crate) const fn is_domain_handshake_packet(&self) -> bool {
        matches!(
            self.0,
            ScheduledPlayPacketKind::AcceptTeleportation(_) | ScheduledPlayPacketKind::PlayerLoaded
        )
    }

    /// Returns whether this packet is allowed to run while a domain switch is in progress.
    pub(crate) const fn is_maintenance_packet(&self) -> bool {
        matches!(self.0, ScheduledPlayPacketKind::PingRequest(_))
    }

    /// Returns whether this is the death screen's one-shot respawn request.
    pub(crate) const fn is_perform_respawn(&self) -> bool {
        matches!(
            self.0,
            ScheduledPlayPacketKind::ClientCommand(SClientCommand {
                action: ClientCommandAction::PerformRespawn,
            })
        )
    }

    #[cfg(test)]
    pub(crate) const fn perform_respawn_for_test() -> Self {
        Self(ScheduledPlayPacketKind::ClientCommand(SClientCommand {
            action: ClientCommandAction::PerformRespawn,
        }))
    }

    #[cfg(test)]
    pub(crate) const fn ping_request_for_test(time: i64) -> Self {
        Self(ScheduledPlayPacketKind::PingRequest(SPingRequest { time }))
    }

    /// Returns the handler's audited cross-player concurrency class.
    ///
    /// This match is intentionally exhaustive so every newly implemented packet requires an
    /// explicit concurrency decision.
    pub(crate) const fn execution(&self) -> ScheduledPacketExecution {
        match &self.0 {
            // These handlers touch player-owned state or use an individually linearizable shared
            // operation. The player lane preserves same-player order.
            ScheduledPlayPacketKind::PingRequest(_)
            | ScheduledPlayPacketKind::ChatSessionUpdate(_)
            | ScheduledPlayPacketKind::ClientInformation(_)
            | ScheduledPlayPacketKind::ClientTickEnd
            | ScheduledPlayPacketKind::PlayerLoaded
            | ScheduledPlayPacketKind::ChatCommand(_)
            | ScheduledPlayPacketKind::CommandSuggestion(_)
            | ScheduledPlayPacketKind::ContainerClose(_)
            | ScheduledPlayPacketKind::SetCreativeModeSlot(_)
            | ScheduledPlayPacketKind::EditBook(_)
            | ScheduledPlayPacketKind::SelectBundleItem(_)
            | ScheduledPlayPacketKind::PlayerInput(_)
            | ScheduledPlayPacketKind::PlayerAbilities(_)
            | ScheduledPlayPacketKind::SetCarriedItem(_)
            | ScheduledPlayPacketKind::Swing(_)
            | ScheduledPlayPacketKind::PickItemFromBlock(_)
            | ScheduledPlayPacketKind::ClientCommand(_)
            | ScheduledPlayPacketKind::SeenAdvancements(_) => ScheduledPacketExecution::PlayerLocal,
            ScheduledPlayPacketKind::PlayerCommand(packet) => match packet.action {
                PlayerCommandAction::StartSprinting
                | PlayerCommandAction::StopSprinting
                | PlayerCommandAction::StartFallFlying => ScheduledPacketExecution::PlayerLocal,
                PlayerCommandAction::LeaveBed => ScheduledPacketExecution::Serialized,
                // Each reaches into the vehicle the player is sitting on: a jump moves
                // both bodies, and the inventory key opens a menu over the mount's own
                // equipment. Neither transaction can be audited against concurrently
                // player-local work.
                PlayerCommandAction::StartRidingJump
                | PlayerCommandAction::StopRidingJump
                | PlayerCommandAction::OpenVehicleInventory => ScheduledPacketExecution::Exclusive,
            },
            ScheduledPlayPacketKind::PlayerAction(packet) => match packet.action {
                PlayerAction::AbortDestroyBlock | PlayerAction::SwapItemWithOffhand => {
                    ScheduledPacketExecution::PlayerLocal
                }
                PlayerAction::StartDestroyBlock
                | PlayerAction::StopDestroyBlock
                | PlayerAction::DropAllItems
                | PlayerAction::DropItem => ScheduledPacketExecution::Serialized,
                // Active-use release may invoke item behavior and mutate inventory, while stab
                // spans independently locked targets; neither can overlap player-local work.
                PlayerAction::ReleaseUseItem | PlayerAction::Stab => {
                    ScheduledPacketExecution::Exclusive
                }
            },
            // Position, world, menu, chat, and domain mutations may overlap player-local work but
            // retain one global mutation order matching the packet submission order.
            ScheduledPlayPacketKind::AcceptTeleportation(_)
            | ScheduledPlayPacketKind::Chat(_)
            | ScheduledPlayPacketKind::ChatAck(_)
            | ScheduledPlayPacketKind::MovePlayer(_)
            | ScheduledPlayPacketKind::MoveVehicle(_)
            | ScheduledPlayPacketKind::ContainerClick(_)
            | ScheduledPlayPacketKind::ContainerSlotStateChanged(_)
            | ScheduledPlayPacketKind::SetBeacon(_)
            | ScheduledPlayPacketKind::SetCommandBlock(_)
            | ScheduledPlayPacketKind::SetCommandMinecart(_)
            | ScheduledPlayPacketKind::SetJigsawBlock(_)
            | ScheduledPlayPacketKind::JigsawGenerate(_)
            | ScheduledPlayPacketKind::SetStructureBlock(_)
            | ScheduledPlayPacketKind::RenameItem(_)
            | ScheduledPlayPacketKind::UseItemOn(_)
            | ScheduledPlayPacketKind::UseItem(_)
            | ScheduledPlayPacketKind::SignUpdate(_)
            | ScheduledPlayPacketKind::SpectatorAction(_)
            | ScheduledPlayPacketKind::ChangeGameMode(_)
            | ScheduledPlayPacketKind::ChangeDifficulty(_) => ScheduledPacketExecution::Serialized,
            // Combat spans source and target state, custom payloads have no constrained resource
            // contract, and the unimplemented menu handlers have no auditable transaction yet.
            ScheduledPlayPacketKind::Attack(_)
            | ScheduledPlayPacketKind::Interact(_)
            | ScheduledPlayPacketKind::CustomClickAction(_)
            | ScheduledPlayPacketKind::CustomPayload(_)
            | ScheduledPlayPacketKind::ContainerButtonClick(_)
            | ScheduledPlayPacketKind::SelectTrade(_) => ScheduledPacketExecution::Exclusive,
        }
    }

    pub(crate) const fn can_process_before_join(&self) -> bool {
        matches!(
            &self.0,
            ScheduledPlayPacketKind::PingRequest(_)
                | ScheduledPlayPacketKind::AcceptTeleportation(_)
                | ScheduledPlayPacketKind::ClientInformation(_)
                | ScheduledPlayPacketKind::ClientTickEnd
                | ScheduledPlayPacketKind::CustomClickAction(_)
                | ScheduledPlayPacketKind::CustomPayload(_)
                | ScheduledPlayPacketKind::ChatAck(_)
                | ScheduledPlayPacketKind::ChatSessionUpdate(_)
                | ScheduledPlayPacketKind::PlayerLoaded
        )
    }

    fn has_illegal_command_characters(&self) -> bool {
        matches!(
            &self.0,
            ScheduledPlayPacketKind::ChatCommand(packet)
                if is_chat_message_illegal(&packet.command)
        )
    }

    /// Routes the packets a player sends while a menu is open.
    ///
    /// Split out of `handle` because the match there is long enough already;
    /// these all reach the same open menu and belong together.
    #[cfg_attr(
        not(test),
        expect(
            clippy::unreachable,
            reason = "packet dispatch is split by kind before this point, and the player's inventory keeps the concrete type it was created with"
        )
    )]
    fn handle_menu(kind: ScheduledPlayPacketKind, player: &Arc<Player>) {
        match kind {
            ScheduledPlayPacketKind::ContainerButtonClick(packet) => {
                player.handle_container_button_click(packet);
            }
            ScheduledPlayPacketKind::ContainerClick(packet) => {
                player.handle_container_click(packet);
            }
            ScheduledPlayPacketKind::ContainerClose(packet) => {
                player.handle_container_close(packet);
            }
            ScheduledPlayPacketKind::ContainerSlotStateChanged(packet) => {
                player.handle_container_slot_state_changed(packet);
            }
            ScheduledPlayPacketKind::EditBook(packet) => player.handle_edit_book(packet),
            ScheduledPlayPacketKind::SelectBundleItem(packet) => {
                player.handle_select_bundle_item(packet);
            }
            ScheduledPlayPacketKind::SelectTrade(packet) => {
                player.handle_select_trade(packet);
            }
            ScheduledPlayPacketKind::SetBeacon(packet) => {
                player.handle_set_beacon(packet);
            }
            _ => unreachable!("handle_menu only takes the menu packets"),
        }
    }

    /// Routes the four gamemaster-block editors.
    ///
    /// Split out of `handle` for the same reason as [`Self::handle_menu`]: they
    /// share a permission gate and belong together.
    #[cfg_attr(
        not(test),
        expect(
            clippy::unreachable,
            reason = "packet dispatch is split by kind before this point, and the player's inventory keeps the concrete type it was created with"
        )
    )]
    fn handle_gamemaster_block(kind: ScheduledPlayPacketKind, player: &Arc<Player>) {
        match kind {
            ScheduledPlayPacketKind::SetCommandBlock(packet) => {
                player.handle_set_command_block(packet);
            }
            ScheduledPlayPacketKind::SetCommandMinecart(packet) => {
                player.handle_set_command_minecart(packet);
            }
            ScheduledPlayPacketKind::SetJigsawBlock(packet) => {
                player.handle_set_jigsaw_block(packet);
            }
            ScheduledPlayPacketKind::JigsawGenerate(packet) => {
                player.handle_jigsaw_generate(packet);
            }
            ScheduledPlayPacketKind::SetStructureBlock(packet) => {
                player.handle_set_structure_block(*packet);
            }
            _ => unreachable!("handle_gamemaster_block only takes the gamemaster-block packets"),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "a flat routing match with one arm per packet; splitting it                   further scatters the routing table rather than shortening it"
    )]
    pub(crate) fn handle(self, player: Arc<Player>, server: &Arc<Server>) {
        if !player.has_joined_world() && !self.can_process_before_join() {
            return;
        }
        if self.has_illegal_command_characters() {
            player.disconnect(translations::MULTIPLAYER_DISCONNECT_ILLEGAL_CHARACTERS.msg());
            return;
        }

        let kind = self.0;
        match kind {
            ScheduledPlayPacketKind::PingRequest(packet) => {
                player.send_packet(CPongResponse::new(packet.time));
            }
            ScheduledPlayPacketKind::AcceptTeleportation(packet) => {
                player.handle_accept_teleportation(packet);
            }
            ScheduledPlayPacketKind::Attack(packet) => player.handle_attack(packet),
            ScheduledPlayPacketKind::Interact(packet) => player.handle_interact(packet),
            ScheduledPlayPacketKind::CustomClickAction(packet) => {
                player.handle_custom_click_action(&packet);
            }
            ScheduledPlayPacketKind::CustomPayload(packet) => {
                player.handle_custom_payload(packet);
            }
            ScheduledPlayPacketKind::Chat(packet) => {
                player.handle_chat(*packet, Arc::clone(&player));
            }
            ScheduledPlayPacketKind::ChatAck(packet) => player.handle_chat_ack(packet),
            ScheduledPlayPacketKind::ChatSessionUpdate(packet) => {
                player.handle_chat_session_update(packet);
            }
            ScheduledPlayPacketKind::ClientInformation(packet) => {
                player.handle_client_information(packet);
            }
            ScheduledPlayPacketKind::ClientTickEnd => player.handle_client_tick_end(),
            ScheduledPlayPacketKind::MovePlayer(packet) => player.handle_move_player(packet),
            ScheduledPlayPacketKind::MoveVehicle(packet) => player.handle_move_vehicle(packet),
            ScheduledPlayPacketKind::PlayerLoaded => {
                if player.mark_client_loaded_from_network() {
                    // Paper fires here rather than on the first world send:
                    // this is the first moment the client is in the world
                    // instead of on a loading screen. Cancelling suppresses
                    // what we send back, not the load, which already happened.
                    let mut loaded = PlayerClientLoadedWorldEvent::new(Arc::clone(&player), true);
                    player.fire_event(&mut loaded);
                    if !loaded.is_cancelled() {
                        player.send_inventory_to_remote();
                    }
                }
            }
            ScheduledPlayPacketKind::ChatCommand(packet) => {
                let mut preprocess = PlayerCommandPreprocessEvent::new(
                    player.gameprofile.id,
                    packet.command.clone(),
                );
                player.fire_event(&mut preprocess);
                if preprocess.is_cancelled() {
                    player.detect_command_rate_spam();
                    return;
                }
                if server
                    .submit_command(
                        CommandSender::Player(Arc::clone(&player)),
                        preprocess.message().to_owned(),
                    )
                    .is_err()
                {
                    player.send_message(
                        &TextComponent::const_plain("Command queue is full").color(Color::Red),
                    );
                }
                player.detect_command_rate_spam();
            }
            ScheduledPlayPacketKind::CommandSuggestion(packet) => {
                if server
                    .submit_command_suggestions(Arc::clone(&player), packet.id, packet.command)
                    .is_err()
                {
                    player.send_packet(CCommandSuggestions::new(packet.id, 0, 0, Vec::new()));
                }
            }
            ScheduledPlayPacketKind::ContainerButtonClick(_)
            | ScheduledPlayPacketKind::ContainerClick(_)
            | ScheduledPlayPacketKind::ContainerClose(_)
            | ScheduledPlayPacketKind::ContainerSlotStateChanged(_)
            | ScheduledPlayPacketKind::EditBook(_)
            | ScheduledPlayPacketKind::SelectBundleItem(_)
            | ScheduledPlayPacketKind::SelectTrade(_)
            | ScheduledPlayPacketKind::SetBeacon(_) => Self::handle_menu(kind, &player),
            ScheduledPlayPacketKind::SetCommandBlock(_)
            | ScheduledPlayPacketKind::SetCommandMinecart(_)
            | ScheduledPlayPacketKind::SetJigsawBlock(_)
            | ScheduledPlayPacketKind::JigsawGenerate(_)
            | ScheduledPlayPacketKind::SetStructureBlock(_) => {
                Self::handle_gamemaster_block(kind, &player);
            }
            ScheduledPlayPacketKind::SetCreativeModeSlot(packet) => {
                player.handle_set_creative_mode_slot(packet);
            }
            ScheduledPlayPacketKind::PlayerInput(packet) => player.handle_player_input(packet),
            ScheduledPlayPacketKind::PlayerCommand(packet) => {
                player.handle_player_command(packet);
            }
            ScheduledPlayPacketKind::PlayerAbilities(packet) => {
                player.handle_player_abilities(packet);
            }
            ScheduledPlayPacketKind::RenameItem(packet) => player.handle_rename_item(packet),
            ScheduledPlayPacketKind::UseItemOn(packet) => player.handle_use_item_on(packet),
            ScheduledPlayPacketKind::UseItem(packet) => player.handle_use_item(packet),
            ScheduledPlayPacketKind::SetCarriedItem(packet) => {
                player.handle_set_carried_item(packet);
            }
            ScheduledPlayPacketKind::Swing(packet) => player.swing(packet.hand, false),
            ScheduledPlayPacketKind::PlayerAction(packet) => {
                player.handle_player_action(packet);
            }
            ScheduledPlayPacketKind::PickItemFromBlock(packet) => {
                player.handle_pick_item_from_block(packet);
            }
            ScheduledPlayPacketKind::SignUpdate(packet) => player.handle_sign_update(packet),
            ScheduledPlayPacketKind::SpectatorAction(packet) => {
                player.handle_spectator_action(packet);
            }
            ScheduledPlayPacketKind::ClientCommand(packet) => {
                player.handle_client_command(packet.action);
            }
            ScheduledPlayPacketKind::SeenAdvancements(packet) => {
                player.handle_seen_advancements(packet.tab);
            }
            ScheduledPlayPacketKind::ChangeGameMode(packet) => {
                handle_client_request(&player, server, packet.gamemode);
            }
            ScheduledPlayPacketKind::ChangeDifficulty(packet) => {
                player.handle_change_difficulty(packet.difficulty);
            }
        }
    }
}

/// Builder for creating packet bundles.
///
/// Used with [`JavaConnection::send_bundle`] to send multiple packets atomically.
pub struct BundleBuilder {
    packets: Vec<EncodedPacket>,
    compression: Option<CompressionInfo>,
}

impl BundleBuilder {
    /// Creates a new `BundleBuilder` with the given compression settings.
    #[must_use]
    pub const fn new(compression: Option<CompressionInfo>) -> Self {
        Self {
            packets: Vec::new(),
            compression,
        }
    }

    /// Adds a packet to the bundle.
    ///
    /// A packet that will not encode is dropped with a warning. The builder has
    /// no connection to disconnect the way the send paths do, and losing one
    /// packet out of a bundle is a smaller wrong than taking the server with
    /// it: `from_bare` refuses anything past `MAX_PACKET_SIZE`, which a
    /// container full of written books can reach.
    pub fn add<P: ClientPacket>(&mut self, packet: P) {
        let Ok(encoded) =
            EncodedPacket::from_bare(packet, self.compression, ConnectionProtocol::Play)
        else {
            log::warn!("Dropping a bundled packet that failed to encode");
            return;
        };
        self.packets.push(encoded);
    }

    /// Consumes the builder and returns the collected encoded packets.
    #[must_use]
    pub fn into_packets(self) -> Vec<EncodedPacket> {
        self.packets
    }
}

/// Milliseconds since the Unix epoch for the opaque keep-alive identifier.
///
/// `duration_since(UNIX_EPOCH)` only fails when the wall clock is set before
/// 1970, which a bad RTC or a container with no clock source can produce. The
/// release profile is `panic = "abort"`, so panicking here would kill the
/// server outright. Elapsed-time decisions use [`Instant`] instead.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

struct KeepAliveTracker {
    sent_at: Option<Instant>,
    pending: bool,
    id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeepAliveTick {
    None,
    Send(u64),
    Timeout,
}

impl KeepAliveTracker {
    const fn new() -> Self {
        Self {
            sent_at: None,
            pending: false,
            id: 0,
        }
    }

    fn tick_at(&mut self, now: Instant, id: u64) -> KeepAliveTick {
        if self
            .sent_at
            .is_some_and(|sent_at| now.saturating_duration_since(sent_at) < KEEP_ALIVE_INTERVAL)
        {
            return KeepAliveTick::None;
        }
        if self.pending {
            return KeepAliveTick::Timeout;
        }

        self.sent_at = Some(now);
        self.pending = true;
        self.id = id;
        KeepAliveTick::Send(id)
    }

    fn acknowledge_at(&mut self, id: u64, now: Instant) -> Option<Duration> {
        if !self.pending || id != self.id {
            return None;
        }
        let sent_at = self.sent_at?;
        self.pending = false;
        Some(now.saturating_duration_since(sent_at))
    }
}

/// A connection to a Java client.
pub struct JavaConnection {
    outgoing_packets: OutboundPacketSender,
    cancel_token: CancellationToken,
    compression: Option<CompressionInfo>,
    network_writer: JavaNetworkWriter,
    id: u64,
    remote_address: SocketAddr,
    translation: Option<PacketTranslation>,
    translated_serverbound: Option<Sender<Vec<u8>>>,

    player: Weak<Player>,
    keep_alive_tracker: SyncMutex<KeepAliveTracker>,
    latency: SyncMutex<u32>,
    /// Play packet ids already reported as unhandled on this connection.
    unhandled_packet_ids: SyncMutex<Vec<i32>>,
}

impl JavaConnection {
    /// Creates a new `JavaConnection`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor receives the established socket state plus its optional protocol bridge; hiding it in another bag would not simplify ownership"
    )]
    pub const fn new(
        outgoing_packets: OutboundPacketSender,
        cancel_token: CancellationToken,
        compression: Option<CompressionInfo>,
        network_writer: JavaNetworkWriter,
        id: u64,
        remote_address: SocketAddr,
        player: Weak<Player>,
        translation: Option<PacketTranslation>,
        translated_serverbound: Option<Sender<Vec<u8>>>,
    ) -> Self {
        Self {
            outgoing_packets,
            cancel_token,
            compression,
            network_writer,
            id,
            remote_address,
            translation,
            translated_serverbound,
            player,
            keep_alive_tracker: SyncMutex::new(KeepAliveTracker::new()),
            latency: SyncMutex::new(0),
            unhandled_packet_ids: SyncMutex::new(Vec::new()),
        }
    }

    /// Returns the address of the connected Java client.
    pub const fn remote_address(&self) -> SocketAddr {
        self.remote_address
    }

    async fn write_packet_now(&self, packet: &EncodedPacket) -> Result<(), PacketError> {
        let mut network_writer = self.network_writer.lock().await;
        let Some(network_writer) = network_writer.as_mut() else {
            return Err(PacketError::ConnectionClosed);
        };
        self.write_packet_with_writer(packet, network_writer).await
    }

    async fn write_packet_with_writer(
        &self,
        packet: &EncodedPacket,
        network_writer: &mut JavaNetworkEncoder,
    ) -> Result<(), PacketError> {
        let Some(translation) = &self.translation else {
            return Self::write_encoded_with_writer(packet, network_writer).await;
        };
        let batch = translation
            .exchange(
                PacketDirection::Clientbound,
                packet.to_packet_data(self.compression)?,
            )
            .await?;
        let Some(serverbound_sender) = &self.translated_serverbound else {
            return Err(PacketError::ConnectionClosed);
        };
        for serverbound in batch.serverbound {
            serverbound_sender.try_send(serverbound).map_err(|_| {
                PacketError::SendError("translated serverbound queue is full or closed".to_owned())
            })?;
        }
        for clientbound in batch.clientbound {
            let encoded = EncodedPacket::from_packet_data(&clientbound, self.compression)?;
            Self::write_encoded_with_writer(&encoded, network_writer).await?;
        }
        Ok(())
    }

    #[cfg(test)]
    async fn write_encoded_raw(&self, packet: &EncodedPacket) -> Result<(), PacketError> {
        let mut network_writer = self.network_writer.lock().await;
        let Some(network_writer) = network_writer.as_mut() else {
            return Err(PacketError::ConnectionClosed);
        };
        Self::write_encoded_with_writer(packet, network_writer).await
    }

    async fn write_encoded_with_writer(
        packet: &EncodedPacket,
        network_writer: &mut JavaNetworkEncoder,
    ) -> Result<(), PacketError> {
        timeout(NETWORK_WRITE_TIMEOUT, network_writer.write_packet(packet))
            .await
            .map_err(|_| PacketError::WriteTimeout)?
    }

    async fn process_translation_batch(
        &self,
        batch: TranslationBatch,
        player: &Arc<Player>,
        server: &Arc<Server>,
    ) -> Result<(), PacketError> {
        if !batch.clientbound.is_empty() {
            let mut network_writer = self.network_writer.lock().await;
            let Some(network_writer) = network_writer.as_mut() else {
                return Err(PacketError::ConnectionClosed);
            };
            for clientbound in batch.clientbound {
                let encoded = EncodedPacket::from_packet_data(&clientbound, self.compression)?;
                Self::write_encoded_with_writer(&encoded, network_writer).await?;
            }
        }
        for serverbound in batch.serverbound {
            self.process_packet(
                RawPacket::from_packet_data(serverbound)?,
                Arc::clone(player),
                server,
            )?;
        }
        Ok(())
    }

    async fn write_envelope(&self, envelope: QueuedOutboundEnvelope) -> Result<bool, PacketError> {
        let mut network_writer = self.network_writer.lock().await;
        let Some(network_writer) = network_writer.as_mut() else {
            return Err(PacketError::ConnectionClosed);
        };
        match envelope {
            QueuedOutboundEnvelope::Single(outbound) => {
                let (packet, close_after_write) = outbound.write_parts();
                self.write_packet_with_writer(packet, network_writer)
                    .await?;
                Ok(close_after_write)
            }
            QueuedOutboundEnvelope::Batch(packets) => {
                for outbound in packets {
                    let (packet, close_after_write) = outbound.write_parts();
                    self.write_packet_with_writer(packet, network_writer)
                        .await?;
                    if close_after_write {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    async fn release_network_writer(&self) {
        self.network_writer.lock().await.take();
    }

    /// Ticks the connection.
    pub fn tick(&self) {
        self.keep_connection_alive();
    }

    fn keep_connection_alive(&self) {
        let mut tracker = self.keep_alive_tracker.lock();
        let action = tracker.tick_at(Instant::now(), now_millis());
        drop(tracker);

        match action {
            KeepAliveTick::None => {}
            KeepAliveTick::Send(id) => self.send_control_packet(CKeepAlive::new(id as i64)),
            KeepAliveTick::Timeout => {
                self.disconnect(translations::DISCONNECT_TIMEOUT.msg());
            }
        }
    }

    /// Handles a keep alive packet.
    fn handle_keep_alive(&self, packet: SKeepAlive) {
        let mut tracker = self.keep_alive_tracker.lock();
        let elapsed = tracker.acknowledge_at(packet.id as u64, Instant::now());
        drop(tracker);
        let Some(elapsed) = elapsed else {
            self.disconnect(translations::DISCONNECT_TIMEOUT.msg());
            return;
        };
        let time = u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);
        let mut latency = self.latency.lock();
        let smoothed = (u64::from(*latency) * 3 + u64::from(time)) / 4;
        *latency = u32::try_from(smoothed).unwrap_or(u32::MAX);
    }

    /// Returns the current latency in milliseconds.
    /// This is a smoothed average calculated from keep-alive round-trip times.
    #[must_use]
    pub fn latency(&self) -> i32 {
        *self.latency.lock() as i32
    }

    /// Disconnects the client.
    pub fn disconnect(&self, reason: impl Into<TextComponent>) {
        let packet = match EncodedPacket::from_bare(
            CDisconnect::new(&reason.into(), self),
            self.compression,
            ConnectionProtocol::Play,
        ) {
            Ok(packet) => packet,
            Err(err) => {
                log::warn!(
                    "Failed to encode disconnect packet for client {}: {err}",
                    self.id
                );
                self.close();
                return;
            }
        };
        if self
            .outgoing_packets
            .try_send_control(OutboundPacket::Disconnect(packet))
            .is_err()
        {
            self.close();
            return;
        }
        self.close();
    }

    /// Sends a packet to the client.
    ///
    /// A packet that will not encode disconnects that client, which is what
    /// vanilla does: `PacketEncoder.encode` throws, `Connection.exceptionCaught`
    /// catches it and drops the connection. It used to abort the whole server
    /// instead, and `from_bare` refuses anything past `MAX_PACKET_SIZE` -- a
    /// container of written books or a command tree grown by plugins gets
    /// there.
    pub fn send_packet<P: ClientPacket>(&self, packet: P) {
        let Ok(packet) =
            EncodedPacket::from_bare(packet, self.compression, ConnectionProtocol::Play)
        else {
            log::warn!(
                "Client {}: disconnecting, a packet for it could not be encoded",
                self.id
            );
            self.close();
            return;
        };
        match self
            .outgoing_packets
            .try_send(OutboundPacket::Packet(packet))
        {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => {
                self.close();
            }
        }
    }

    fn send_control_packet<P: ClientPacket>(&self, packet: P) {
        let Ok(packet) =
            EncodedPacket::from_bare(packet, self.compression, ConnectionProtocol::Play)
        else {
            self.close();
            return;
        };
        if self
            .outgoing_packets
            .try_send_control(OutboundPacket::Packet(packet))
            .is_err()
        {
            self.close();
        }
    }

    /// Sends an encoded packet to the client.
    ///
    /// Saturation closes the connection rather than silently losing an
    /// arbitrary state transition. Chunk streams use the retryable atomic path.
    pub fn send_encoded_packet(&self, packet: EncodedPacket) {
        match self
            .outgoing_packets
            .try_send(OutboundPacket::Packet(packet))
        {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => {
                self.close();
            }
        }
    }

    /// Atomically queues every encoded packet in a chunk batch.
    pub(crate) fn try_send_chunk_batch(
        &self,
        packets: Vec<EncodedPacket>,
    ) -> Result<(), mpsc::error::TrySendError<Vec<EncodedPacket>>> {
        self.outgoing_packets.try_send_chunk_batch(packets)
    }

    fn send_encoded_packet_bundle(&self, content: Vec<EncodedPacket>) {
        let Ok(start) =
            EncodedPacket::from_bare(CBundleDelimiter, self.compression, ConnectionProtocol::Play)
        else {
            self.close();
            return;
        };
        let Ok(finished) =
            EncodedPacket::from_bare(CBundleDelimiter, self.compression, ConnectionProtocol::Play)
        else {
            self.close();
            return;
        };
        let mut packets = Vec::with_capacity(content.len() + 2);
        packets.push(start);
        packets.extend(content);
        packets.push(finished);
        if self
            .outgoing_packets
            .try_send_packet_batch(packets, OutboundBatchLane::Reliable)
            .is_err()
        {
            self.close();
        }
    }

    /// Maximum encoded bytes an otherwise-empty chunk queue can admit atomically.
    pub(crate) fn chunk_batch_byte_capacity(&self) -> usize {
        self.outgoing_packets.chunk_batch_byte_capacity()
    }

    /// Closes the connection.
    pub fn close(&self) {
        self.cancel_token.cancel();
    }

    /// Returns whether the connection is closed.
    #[must_use]
    pub fn closed(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Waits for the connection to be closed.
    pub async fn wait_for_close(&self) {
        self.cancel_token.cancelled().await;
    }

    const fn can_process_before_join(packet_id: i32) -> bool {
        matches!(
            packet_id,
            play::S_ACCEPT_TELEPORTATION
                | play::S_KEEP_ALIVE
                | play::S_PING_REQUEST
                | play::S_CLIENT_INFORMATION
                | play::S_CUSTOM_PAYLOAD
                | play::S_CHUNK_BATCH_RECEIVED
                | play::S_CHAT_SESSION_UPDATE
                | play::S_CHAT_ACK
                | play::S_CLIENT_TICK_END
                | play::S_PLAYER_LOADED
        )
    }

    const fn can_process_during_domain_handshake(packet_id: i32) -> bool {
        matches!(
            packet_id,
            play::S_ACCEPT_TELEPORTATION | play::S_CHUNK_BATCH_RECEIVED | play::S_PLAYER_LOADED
        )
    }

    /// Decodes and dispatches one packet received from the client.
    fn process_packet(
        &self,
        packet: RawPacket,
        player: Arc<Player>,
        server: &Server,
    ) -> Result<(), PacketError> {
        if !player.has_joined_world() && !Self::can_process_before_join(packet.id) {
            return Ok(());
        }

        let payload_bytes = packet.payload().len();
        let Some(packet) = Self::decode_domain_gated_packet(packet, &player)? else {
            return Ok(());
        };

        match packet {
            DecodedPlayPacket::Scheduled(packet) => {
                server.schedule_play_packet(player, packet, payload_bytes);
            }
            DecodedPlayPacket::Immediate(packet) => {
                self.handle_immediate_packet(packet, &player);
            }
        }
        Ok(())
    }

    fn decode_domain_gated_packet(
        packet: RawPacket,
        player: &Player,
    ) -> Result<Option<DecodedPlayPacket>, PacketError> {
        let maintenance_packet = matches!(packet.id, play::S_KEEP_ALIVE | play::S_PING_REQUEST);
        let handshake_packet = Self::can_process_during_domain_handshake(packet.id);
        if packet.id == play::S_CLIENT_COMMAND {
            let decoded = Self::decode_play_packet(packet)?;
            let perform_respawn = matches!(
                &decoded,
                DecodedPlayPacket::Scheduled(packet) if packet.is_perform_respawn()
            );
            if player.gate_domain_switch_packet(false, perform_respawn) {
                return Ok(Some(decoded));
            }
            return Ok(None);
        }

        if !maintenance_packet && !player.gate_domain_switch_packet(handshake_packet, false) {
            return Ok(None);
        }

        Self::decode_play_packet(packet).map(Some)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "single match decode over all implemented play packets keeps protocol routing auditable"
    )]
    fn decode_play_packet(packet: RawPacket) -> Result<DecodedPlayPacket, PacketError> {
        let data = &mut Cursor::new(packet.payload());
        let scheduled = |packet| DecodedPlayPacket::Scheduled(ScheduledPlayPacket(packet));

        Ok(match packet.id {
            play::S_ACCEPT_TELEPORTATION => {
                scheduled(ScheduledPlayPacketKind::AcceptTeleportation(
                    SAcceptTeleportation::read_packet(data)?,
                ))
            }
            play::S_ATTACK => {
                scheduled(ScheduledPlayPacketKind::Attack(SAttack::read_packet(data)?))
            }
            play::S_INTERACT => scheduled(ScheduledPlayPacketKind::Interact(
                SInteract::read_packet(data)?,
            )),
            play::S_CUSTOM_CLICK_ACTION => scheduled(ScheduledPlayPacketKind::CustomClickAction(
                SCustomClickAction::read_packet(data)?,
            )),
            play::S_CUSTOM_PAYLOAD => scheduled(ScheduledPlayPacketKind::CustomPayload(
                SCustomPayload::read_packet(data)?,
            )),
            play::S_CHAT => scheduled(ScheduledPlayPacketKind::Chat(Box::new(SChat::read_packet(
                data,
            )?))),
            play::S_CHAT_SESSION_UPDATE => scheduled(ScheduledPlayPacketKind::ChatSessionUpdate(
                SChatSessionUpdate::read_packet(data)?,
            )),
            play::S_CHAT_ACK => scheduled(ScheduledPlayPacketKind::ChatAck(SChatAck::read_packet(
                data,
            )?)),
            play::S_CLIENT_INFORMATION => scheduled(ScheduledPlayPacketKind::ClientInformation(
                SClientInformation::read_packet(data)?,
            )),
            play::S_CLIENT_TICK_END => {
                let _ = SClientTickEnd::read_packet(data)?;
                scheduled(ScheduledPlayPacketKind::ClientTickEnd)
            }
            play::S_CHUNK_BATCH_RECEIVED => DecodedPlayPacket::Immediate(
                ImmediatePlayPacket::ChunkBatchReceived(SChunkBatchReceived::read_packet(data)?),
            ),
            play::S_KEEP_ALIVE => DecodedPlayPacket::Immediate(ImmediatePlayPacket::KeepAlive(
                SKeepAlive::read_packet(data)?,
            )),
            play::S_MOVE_PLAYER_POS => scheduled(ScheduledPlayPacketKind::MovePlayer(
                SMovePlayerPos::read_packet(data)?.into(),
            )),
            play::S_MOVE_PLAYER_POS_ROT => scheduled(ScheduledPlayPacketKind::MovePlayer(
                SMovePlayerPosRot::read_packet(data)?.into(),
            )),
            play::S_MOVE_PLAYER_ROT => scheduled(ScheduledPlayPacketKind::MovePlayer(
                SMovePlayerRot::read_packet(data)?.into(),
            )),
            play::S_MOVE_PLAYER_STATUS_ONLY => scheduled(ScheduledPlayPacketKind::MovePlayer(
                SMovePlayerStatusOnly::read_packet(data)?.into(),
            )),
            play::S_MOVE_VEHICLE => scheduled(ScheduledPlayPacketKind::MoveVehicle(
                SMoveVehicle::read_packet(data)?,
            )),
            play::S_PLAYER_LOADED => {
                let _ = SPlayerLoad::read_packet(data)?;
                scheduled(ScheduledPlayPacketKind::PlayerLoaded)
            }
            play::S_CHAT_COMMAND => scheduled(ScheduledPlayPacketKind::ChatCommand(
                SChatCommand::read_packet(data)?,
            )),
            play::S_COMMAND_SUGGESTION => scheduled(ScheduledPlayPacketKind::CommandSuggestion(
                SCommandSuggestion::read_packet(data)?,
            )),
            play::S_CONTAINER_BUTTON_CLICK => {
                scheduled(ScheduledPlayPacketKind::ContainerButtonClick(
                    SContainerButtonClick::read_packet(data)?,
                ))
            }
            play::S_CONTAINER_CLICK => scheduled(ScheduledPlayPacketKind::ContainerClick(
                SContainerClick::read_packet(data)?,
            )),
            play::S_CONTAINER_CLOSE => scheduled(ScheduledPlayPacketKind::ContainerClose(
                SContainerClose::read_packet(data)?,
            )),
            play::S_CONTAINER_SLOT_STATE_CHANGED => {
                scheduled(ScheduledPlayPacketKind::ContainerSlotStateChanged(
                    SContainerSlotStateChanged::read_packet(data)?,
                ))
            }
            play::S_EDIT_BOOK => scheduled(ScheduledPlayPacketKind::EditBook(
                SEditBook::read_packet(data)?,
            )),
            play::S_BUNDLE_ITEM_SELECTED => scheduled(ScheduledPlayPacketKind::SelectBundleItem(
                SSelectBundleItem::read_packet(data)?,
            )),
            play::S_SELECT_TRADE => scheduled(ScheduledPlayPacketKind::SelectTrade(
                SSelectTrade::read_packet(data)?,
            )),
            play::S_SET_BEACON => scheduled(ScheduledPlayPacketKind::SetBeacon(
                SSetBeacon::read_packet(data)?,
            )),
            play::S_SET_COMMAND_BLOCK => scheduled(ScheduledPlayPacketKind::SetCommandBlock(
                SSetCommandBlock::read_packet(data)?,
            )),
            play::S_SET_COMMAND_MINECART => scheduled(ScheduledPlayPacketKind::SetCommandMinecart(
                SSetCommandMinecart::read_packet(data)?,
            )),
            play::S_SET_JIGSAW_BLOCK => scheduled(ScheduledPlayPacketKind::SetJigsawBlock(
                SSetJigsawBlock::read_packet(data)?,
            )),
            play::S_JIGSAW_GENERATE => scheduled(ScheduledPlayPacketKind::JigsawGenerate(
                SJigsawGenerate::read_packet(data)?,
            )),
            play::S_SET_STRUCTURE_BLOCK => scheduled(ScheduledPlayPacketKind::SetStructureBlock(
                Box::new(SSetStructureBlock::read_packet(data)?),
            )),
            play::S_SET_CREATIVE_MODE_SLOT => {
                scheduled(ScheduledPlayPacketKind::SetCreativeModeSlot(
                    SSetCreativeModeSlot::read_packet(data)?,
                ))
            }
            play::S_PLAYER_INPUT => scheduled(ScheduledPlayPacketKind::PlayerInput(
                SPlayerInput::read_packet(data)?,
            )),
            play::S_PLAYER_COMMAND => scheduled(ScheduledPlayPacketKind::PlayerCommand(
                SPlayerCommand::read_packet(data)?,
            )),
            play::S_PLAYER_ABILITIES => scheduled(ScheduledPlayPacketKind::PlayerAbilities(
                SPlayerAbilities::read_packet(data)?,
            )),
            play::S_RENAME_ITEM => scheduled(ScheduledPlayPacketKind::RenameItem(
                SRenameItem::read_packet(data)?,
            )),
            play::S_USE_ITEM_ON => scheduled(ScheduledPlayPacketKind::UseItemOn(
                SUseItemOn::read_packet(data)?,
            )),
            play::S_USE_ITEM => scheduled(ScheduledPlayPacketKind::UseItem(SUseItem::read_packet(
                data,
            )?)),
            play::S_SET_CARRIED_ITEM => scheduled(ScheduledPlayPacketKind::SetCarriedItem(
                SSetCarriedItem::read_packet(data)?,
            )),
            play::S_SWING => scheduled(ScheduledPlayPacketKind::Swing(SSwing::read_packet(data)?)),
            play::S_PLAYER_ACTION => scheduled(ScheduledPlayPacketKind::PlayerAction(
                SPlayerAction::read_packet(data)?,
            )),
            play::S_PICK_ITEM_FROM_BLOCK => scheduled(ScheduledPlayPacketKind::PickItemFromBlock(
                SPickItemFromBlock::read_packet(data)?,
            )),
            play::S_SIGN_UPDATE => scheduled(ScheduledPlayPacketKind::SignUpdate(
                SSignUpdate::read_packet(data)?,
            )),
            play::S_SPECTATOR_ACTION => scheduled(ScheduledPlayPacketKind::SpectatorAction(
                SSpectatorAction::read_packet(data)?,
            )),
            play::S_CLIENT_COMMAND => scheduled(ScheduledPlayPacketKind::ClientCommand(
                SClientCommand::read_packet(data)?,
            )),
            play::S_SEEN_ADVANCEMENTS => scheduled(ScheduledPlayPacketKind::SeenAdvancements(
                SSeenAdvancements::read_packet(data)?,
            )),
            play::S_PING_REQUEST => scheduled(ScheduledPlayPacketKind::PingRequest(
                SPingRequest::read_packet(data)?,
            )),
            play::S_CHANGE_GAME_MODE => scheduled(ScheduledPlayPacketKind::ChangeGameMode(
                SChangeGameMode::read_packet(data)?,
            )),
            play::S_CHANGE_DIFFICULTY => scheduled(ScheduledPlayPacketKind::ChangeDifficulty(
                SChangeDifficulty::read_packet(data)?,
            )),
            id => DecodedPlayPacket::Immediate(ImmediatePlayPacket::Unknown(id)),
        })
    }

    fn handle_immediate_packet(&self, packet: ImmediatePlayPacket, player: &Player) {
        match packet {
            ImmediatePlayPacket::KeepAlive(packet) => self.handle_keep_alive(packet),
            ImmediatePlayPacket::ChunkBatchReceived(packet) => {
                player
                    .chunk_sender
                    .lock()
                    .on_chunk_batch_received_by_client(packet.desired_chunks_per_tick);
            }
            ImmediatePlayPacket::Unknown(id) => self.report_unhandled_play_packet(id),
        }
    }

    /// Reports a serverbound play packet id that no handler claims, once each.
    ///
    /// Vanilla throws `DecoderException` on an unknown id and drops the
    /// connection (`IdDispatchCodec.decode`), because vanilla has a handler for
    /// every id it defines. Foton does not: `lock_difficulty`,
    /// `pick_item_from_entity`, `place_recipe`, both recipe-book packets,
    /// `entity_tag_query` and `teleport_to_entity` are all sent by ordinary
    /// clients and none of them is handled here, so kicking would punish a
    /// player for pressing a button Foton has not implemented yet.
    ///
    /// What could not stay is the reporting. `Unknown` is an immediate packet,
    /// so it never passes `PacketProcessor::schedule` and is not counted
    /// against the per-player outstanding budget; one `info!` per packet let
    /// any player fill the log as fast as the socket allowed, and filled it
    /// during ordinary play besides. One line per id per connection keeps the
    /// signal and removes the firehose.
    fn report_unhandled_play_packet(&self, id: i32) {
        /// Enough for every unimplemented vanilla id; beyond that the client is
        /// inventing ids, and there is nothing left to learn from listing them.
        const MAX_REPORTED_IDS: usize = 32;

        {
            let mut reported = self.unhandled_packet_ids.lock();
            if reported.len() >= MAX_REPORTED_IDS || reported.contains(&id) {
                return;
            }
            reported.push(id);
        }

        log::debug!("Client {} sent unhandled play packet id {id}", self.id);
    }

    /// Listens for packets from the client.
    pub async fn listener(
        &self,
        mut reader: TCPNetworkDecoder<BufReader<OwnedReadHalf>>,
        server: Arc<Server>,
        pending_serverbound: Vec<Vec<u8>>,
        mut translated_serverbound_recv: Receiver<Vec<u8>>,
    ) {
        if let Some(player) = self.player.upgrade() {
            for pending in pending_serverbound {
                let result = RawPacket::from_packet_data(pending)
                    .and_then(|packet| self.process_packet(packet, Arc::clone(&player), &server));
                if let Err(error) = result {
                    log::warn!(
                        "Translated handoff packet failed for client {}: {error}",
                        self.id
                    );
                    self.close();
                    return;
                }
            }
        }
        let mut translation_poll = interval(Duration::from_millis(50));
        translation_poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            select! {
                () = self.wait_for_close() => {
                    break;
                }
                packet = reader.get_raw_packet() => {
                    match packet {
                        Ok(packet) => {
                            if let Some(player) = self.player.upgrade() {
                                let processed = if let Some(translation) = &self.translation {
                                    match packet.to_packet_data() {
                                        Ok(data) => match translation.exchange(PacketDirection::Serverbound, data).await {
                                            Ok(batch) => self.process_translation_batch(batch, &player, &server).await,
                                            Err(error) => Err(error),
                                        },
                                        Err(error) => Err(error),
                                    }
                                } else {
                                    self.process_packet(packet, player, &server)
                                };
                                if let Err(err) = processed {
                                // Vanilla parity: `Connection.exceptionCaught`
                                // disconnects on anything but a
                                // `SkipPacketException`. Logging and carrying on
                                // gave a client an unmetered way to write to the
                                // server's log file, and -- worse -- it hid real
                                // parity bugs: the chat message bound was one
                                // byte-vs-UTF-16 mistake away from silently
                                // dropping every accented message, and nothing
                                // surfaced it because the failure only ever
                                // reached this line.
                                log::warn!(
                                    "Disconnecting client {} after a packet it sent failed to decode: {err}",
                                    self.id
                                );
                                self.close();
                                }
                            }
                        }
                        Err(err) => {
                            log::debug!("Failed to get raw packet from client {}: {err}", self.id);
                            self.close();
                        }
                    }
                }
                translated = translated_serverbound_recv.recv() => {
                    let Some(translated) = translated else { self.close(); break; };
                    if let Some(player) = self.player.upgrade() {
                        let result = RawPacket::from_packet_data(translated)
                            .and_then(|packet| self.process_packet(packet, player, &server));
                        if let Err(error) = result {
                            log::warn!("Translated packet failed for client {}: {error}", self.id);
                            self.close();
                        }
                    }
                }
                _ = translation_poll.tick(), if self.translation.is_some() => {
                    let Some(player) = self.player.upgrade() else { self.close(); break; };
                    let result = match &self.translation {
                        Some(translation) => translation.poll().await,
                        None => continue,
                    };
                    if let Err(error) = match result {
                        Ok(batch) => self.process_translation_batch(batch, &player, &server).await,
                        Err(error) => Err(error),
                    } {
                        log::warn!("Via scheduled output failed for client {}: {error}", self.id);
                        self.close();
                    }
                }
            }
        }
    }

    /// Sends packets to the client.
    ///
    pub async fn sender(&self, mut sender_recv: OutboundPacketReceiver) {
        loop {
            select! {
                biased;
                () = self.wait_for_close() => {
                    self.write_queued_disconnect(&mut sender_recv).await;
                    break;
                }
                envelope = sender_recv.recv_envelope() => {
                    if let Some(envelope) = envelope {
                        // Once an envelope starts, it owns the wire until its
                        // final translated output is written. Cancelling this
                        // future halfway through a bundle would expose a
                        // partial chunk batch and let a disconnect or a Via
                        // scheduled packet split the bundle.
                        match self.write_envelope(envelope).await {
                            Ok(true) => {
                                self.close();
                                break;
                            }
                            Ok(false) => {}
                            Err(err) => {
                                log::warn!("Failed to send packet envelope to client {}: {err}", self.id);
                                self.close();
                                break;
                            }
                        }
                    } else {
                        //log::warn!(
                        //    "Internal packet_sender_recv channel closed for client {}",
                        //    self.id
                        //);
                        self.close();
                    }
                }
            }
        }

        self.release_network_writer().await;
        if let Some(translation) = &self.translation
            && let Err(error) = translation.close().await
        {
            log::debug!(
                "Failed to close protocol translator for client {}: {error}",
                self.id
            );
        }

        let Some(player) = self.player.upgrade() else {
            return;
        };
        if !player.has_joined_world() || player.server().cancel_token.is_cancelled() {
            return;
        }
        player.server().queue_player_disconnect(player);
    }

    async fn write_queued_disconnect(&self, sender_recv: &mut OutboundPacketReceiver) {
        let mut disconnect_packet = None;
        while let Ok(outbound) = sender_recv.try_recv() {
            if outbound.write_parts().1 {
                disconnect_packet = Some(outbound);
            }
        }

        let Some(outbound) = disconnect_packet else {
            return;
        };
        let (packet, _) = outbound.write_parts();
        if let Err(err) = self.write_packet_now(packet).await {
            log::warn!(
                "Failed to send disconnect packet to client {} during close: {err}",
                self.id
            );
        }
    }
}

impl TextResolutor for JavaConnection {
    fn resolve_content(&self, _resolvable: &Resolvable) -> TextComponent {
        TextComponent::new()
    }

    fn resolve_custom(&self, _data: &CustomData) -> Option<TextComponent> {
        None
    }

    fn translate(&self, _key: &str) -> Option<String> {
        None
    }
}

impl NetworkConnection for JavaConnection {
    fn compression(&self) -> Option<CompressionInfo> {
        self.compression
    }

    fn send_encoded(&self, packet: EncodedPacket) {
        self.send_encoded_packet(packet);
    }

    fn send_encoded_bundle(&self, packets: Vec<EncodedPacket>) {
        self.send_encoded_packet_bundle(packets);
    }

    fn disconnect_with_reason(&self, reason: TextComponent) {
        self.disconnect(reason);
    }

    fn tick(&self) {
        self.keep_connection_alive();
    }

    fn latency(&self) -> i32 {
        *self.latency.lock() as i32
    }

    fn close(&self) {
        self.cancel_token.cancel();
    }

    fn closed(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    fn remote_address(&self) -> Option<SocketAddr> {
        Some(self.remote_address)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        array,
        future::pending,
        time::{Duration, Instant},
    };

    use crate::{
        entity::{Entity as _, LivingEntity as _},
        test_support::{TestPlayerBuilder, fresh_test_world},
    };
    use foton_protocol::packets::common::{ChatVisibility, HumanoidArm, ParticleStatus};
    use foton_protocol::packets::game::{ClickType, ClientCommandAction, HashedStack};
    use foton_registry::{blocks::properties::Direction, item_stack::ItemStack};
    use foton_utils::{BlockPos, codec::VarInt, types::InteractionHand};
    use rustc_hash::FxHashMap;
    use tokio::{
        io::AsyncReadExt,
        net::{TcpListener, TcpStream},
        time::{sleep, timeout},
    };
    use uuid::Uuid;

    use super::*;

    fn encoded_packet(byte_len: usize) -> EncodedPacket {
        let mut encoded_data = foton_utils::FrontVec::new(0);
        encoded_data.extend_from_slice(&vec![0; byte_len]);
        EncodedPacket {
            encoded_data: Arc::new(encoded_data),
        }
    }

    fn encoded_packets(count: usize, byte_len: usize) -> Vec<EncodedPacket> {
        (0..count).map(|_| encoded_packet(byte_len)).collect()
    }

    fn tagged_packet(tag: u8) -> EncodedPacket {
        let mut encoded_data = foton_utils::FrontVec::new(0);
        encoded_data.push(tag);
        EncodedPacket {
            encoded_data: Arc::new(encoded_data),
        }
    }

    fn tagged_packet_with_size(tag: u8, byte_len: usize) -> EncodedPacket {
        let mut encoded_data = foton_utils::FrontVec::new(0);
        encoded_data.extend_from_slice(&vec![tag; byte_len]);
        EncodedPacket {
            encoded_data: Arc::new(encoded_data),
        }
    }

    fn queued_tag(packet: &QueuedOutboundPacket) -> u8 {
        packet
            .write_parts()
            .0
            .encoded_data
            .first()
            .copied()
            .expect("tagged packets are non-empty")
    }

    async fn test_java_connection(sender: OutboundPacketSender) -> (JavaConnection, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should have an address");
        let (client, accepted) = tokio::join!(TcpStream::connect(address), listener.accept());
        let client = client.expect("test client should connect");
        let (server_stream, remote_address) = accepted.expect("test listener should accept");
        let (_read_half, write_half) = server_stream.into_split();
        let connection = JavaConnection::new(
            sender,
            CancellationToken::new(),
            None,
            Arc::new(AsyncMutex::new(Some(TCPNetworkEncoder::new(
                BufWriter::new(write_half),
            )))),
            1,
            remote_address,
            Weak::new(),
            None,
            None,
        );
        (connection, client)
    }

    #[tokio::test]
    async fn outbound_packet_queue_holds_its_byte_budget_through_the_write() {
        let (sender, mut receiver) = outbound_packet_channel_with_budget(8);
        assert!(
            sender
                .try_send(OutboundPacket::Packet(encoded_packet(5)))
                .is_ok()
        );
        assert!(matches!(
            sender.try_send(OutboundPacket::Packet(encoded_packet(4))),
            Err(mpsc::error::TrySendError::Full(_))
        ));

        let queued = receiver
            .recv()
            .await
            .expect("the queued packet should still be available");
        assert!(matches!(
            sender.try_send(OutboundPacket::Packet(encoded_packet(4))),
            Err(mpsc::error::TrySendError::Full(_))
        ));

        drop(queued);
        assert!(
            sender
                .try_send(OutboundPacket::Packet(encoded_packet(4)))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn sixty_four_chunk_batch_is_atomic_and_recovers_after_one_entry_drains() {
        let (sender, mut receiver) = outbound_packet_channel_with_budget(1024);
        for _ in 0..191 {
            assert!(sender.try_send_chunk_batch(encoded_packets(1, 1)).is_ok());
        }

        assert!(matches!(
            sender.try_send_chunk_batch(encoded_packets(66, 1)),
            Err(mpsc::error::TrySendError::Full(_))
        ));

        let first = receiver
            .recv()
            .await
            .expect("the chunk packet used to fill the queue should be available");
        drop(first);
        assert!(sender.try_send_chunk_batch(encoded_packets(66, 1)).is_ok());

        let mut queued = 0;
        while let Ok(packet) = receiver.try_recv() {
            queued += 1;
            drop(packet);
        }
        assert_eq!(queued, OUTBOUND_CHUNK_QUEUE_ENTRY_CAPACITY);
    }

    #[tokio::test]
    async fn chunk_saturation_preserves_control_admission_and_fifo_fairness() {
        let (sender, mut receiver) = outbound_packet_channel_with_budget(1024);
        for _ in 0..OUTBOUND_CHUNK_QUEUE_ENTRY_CAPACITY {
            assert!(sender.try_send_chunk_batch(vec![tagged_packet(1)]).is_ok());
        }
        assert!(matches!(
            sender.try_send_chunk_batch(encoded_packets(1, 1)),
            Err(mpsc::error::TrySendError::Full(_))
        ));

        assert!(
            sender
                .try_send_control(OutboundPacket::Packet(tagged_packet(2)))
                .is_ok()
        );
        assert!(
            sender
                .try_send(OutboundPacket::Packet(tagged_packet(3)))
                .is_ok()
        );
        let control = receiver
            .recv()
            .await
            .expect("reserved control must bypass the bounded data backlog");
        assert_eq!(queued_tag(&control), 2);
        for _ in 0..OUTBOUND_CHUNK_QUEUE_ENTRY_CAPACITY {
            let chunk = receiver
                .recv()
                .await
                .expect("each admitted chunk must drain");
            assert_eq!(queued_tag(&chunk), 1);
        }
        let reliable = receiver.recv().await.expect("reliable traffic must drain");
        assert_eq!(queued_tag(&reliable), 3);
    }

    #[tokio::test]
    async fn data_causal_order_and_batch_boundaries_are_preserved_at_receiver() {
        let (sender, mut receiver) = outbound_packet_channel_with_budget(1024);
        assert!(
            sender
                .try_send(OutboundPacket::Packet(tagged_packet(1)))
                .is_ok()
        );
        assert!(
            sender
                .try_send_chunk_batch(vec![tagged_packet(2), tagged_packet(3), tagged_packet(4)])
                .is_ok()
        );
        assert!(
            sender
                .try_send(OutboundPacket::Packet(tagged_packet(6)))
                .is_ok()
        );

        let mut actual = Vec::new();
        for _ in 0..5 {
            let packet = receiver.recv().await.expect("every packet must drain");
            actual.push(queued_tag(&packet));
        }
        assert_eq!(actual, [1, 2, 3, 4, 6]);
    }

    #[tokio::test]
    async fn batch_is_written_without_control_or_reliable_interleaving() {
        let (sender, receiver) = outbound_packet_channel_with_budget(1024);
        assert!(
            sender
                .try_send_chunk_batch(vec![tagged_packet(1), tagged_packet(2), tagged_packet(3)])
                .is_ok()
        );
        assert!(
            sender
                .try_send_control(OutboundPacket::Packet(tagged_packet(4)))
                .is_ok()
        );
        assert!(
            sender
                .try_send_control(OutboundPacket::Packet(tagged_packet(5)))
                .is_ok()
        );
        assert!(
            sender
                .try_send(OutboundPacket::Packet(tagged_packet(6)))
                .is_ok()
        );
        let (connection, mut client) = test_java_connection(sender).await;
        let mut wire = [0; 6];

        tokio::join!(connection.sender(receiver), async {
            client
                .read_exact(&mut wire)
                .await
                .expect("all queued bytes must reach the wire");
            connection.close();
        });

        assert_eq!(wire, [4, 1, 2, 3, 5, 6]);
    }

    #[tokio::test]
    async fn synthetic_write_waits_for_the_complete_outbound_envelope() {
        const FIRST_PACKET_BYTES: usize = 4 * 1024 * 1024;

        let (sender, receiver) = outbound_packet_channel();
        assert!(
            sender
                .try_send_chunk_batch(vec![
                    tagged_packet_with_size(1, FIRST_PACKET_BYTES),
                    tagged_packet(2),
                ])
                .is_ok()
        );
        let (connection, mut client) = test_java_connection(sender).await;

        tokio::join!(connection.sender(receiver), async {
            let mut first_byte = [0];
            client
                .read_exact(&mut first_byte)
                .await
                .expect("the batch should start writing");
            assert_eq!(first_byte, [1]);

            let synthetic_packet = tagged_packet(9);
            let synthetic_write = connection.write_encoded_raw(&synthetic_packet);
            tokio::pin!(synthetic_write);
            tokio::select! {
                biased;
                result = &mut synthetic_write => panic!("synthetic write crossed the active envelope: {result:?}"),
                () = sleep(Duration::from_millis(10)) => {}
            }

            let mut remaining = vec![0; FIRST_PACKET_BYTES + 1];
            let (read, synthetic) =
                tokio::join!(client.read_exact(&mut remaining), &mut synthetic_write,);
            read.expect("the complete batch and synthetic packet should arrive");
            synthetic.expect("synthetic packet should write after the batch");
            assert!(
                remaining[..FIRST_PACKET_BYTES - 1]
                    .iter()
                    .all(|byte| *byte == 1)
            );
            assert_eq!(&remaining[FIRST_PACKET_BYTES - 1..], &[2, 9]);
            connection.close();
        });
    }

    #[test]
    fn chunk_batches_leave_the_control_byte_reserve_available() {
        let (sender, _receiver) = outbound_packet_channel_with_budget(1024);
        assert!(sender.try_send_chunk_batch(encoded_packets(1, 576)).is_ok());
        assert!(
            sender
                .try_send(OutboundPacket::Packet(encoded_packet(192)))
                .is_ok()
        );
        assert!(
            sender
                .try_send_control(OutboundPacket::Packet(encoded_packet(256)))
                .is_ok()
        );
    }

    #[test]
    fn chunk_batch_admission_distinguishes_a_closed_receiver() {
        let (sender, receiver) = outbound_packet_channel_with_budget(1024);
        drop(receiver);

        assert!(matches!(
            sender.try_send_chunk_batch(encoded_packets(1, 1)),
            Err(mpsc::error::TrySendError::Closed(_))
        ));
    }

    #[tokio::test]
    async fn process_budget_is_shared_and_keeps_global_control_headroom() {
        let global_budget = Arc::new(GlobalOutboundBudget {
            entry_budget: Arc::new(Semaphore::new(10)),
            non_control_entry_budget: Arc::new(Semaphore::new(8)),
            byte_budget: Arc::new(Semaphore::new(10)),
            non_control_byte_budget: Arc::new(Semaphore::new(8)),
            chunk_byte_budget: Arc::new(Semaphore::new(6)),
        });
        let (first, mut first_receiver) =
            outbound_packet_channel_with_budgets(100, Arc::clone(&global_budget));
        let (second, _second_receiver) = outbound_packet_channel_with_budgets(100, global_budget);

        assert!(
            first
                .try_send(OutboundPacket::Packet(encoded_packet(8)))
                .is_ok()
        );
        assert!(matches!(
            second.try_send(OutboundPacket::Packet(encoded_packet(1))),
            Err(mpsc::error::TrySendError::Full(_))
        ));
        assert!(
            second
                .try_send_control(OutboundPacket::Packet(encoded_packet(2)))
                .is_ok()
        );

        let queued = first_receiver
            .recv()
            .await
            .expect("the first connection should own the global capacity");
        drop(queued);
        assert!(
            second
                .try_send(OutboundPacket::Packet(encoded_packet(8)))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn process_entry_budget_is_shared_and_reserves_control_capacity() {
        let global_budget = Arc::new(GlobalOutboundBudget {
            entry_budget: Arc::new(Semaphore::new(4)),
            non_control_entry_budget: Arc::new(Semaphore::new(3)),
            byte_budget: Arc::new(Semaphore::new(100)),
            non_control_byte_budget: Arc::new(Semaphore::new(90)),
            chunk_byte_budget: Arc::new(Semaphore::new(80)),
        });
        let (first, mut first_receiver) =
            outbound_packet_channel_with_budgets(100, Arc::clone(&global_budget));
        let (second, _second_receiver) = outbound_packet_channel_with_budgets(100, global_budget);

        for _ in 0..3 {
            assert!(
                first
                    .try_send(OutboundPacket::Packet(encoded_packet(1)))
                    .is_ok()
            );
        }
        assert!(matches!(
            second.try_send(OutboundPacket::Packet(encoded_packet(1))),
            Err(mpsc::error::TrySendError::Full(_))
        ));
        assert!(
            second
                .try_send_control(OutboundPacket::Packet(encoded_packet(1)))
                .is_ok()
        );

        let queued = first_receiver
            .recv()
            .await
            .expect("one global entry should drain");
        drop(queued);
        assert!(
            second
                .try_send(OutboundPacket::Packet(encoded_packet(1)))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn generic_packet_saturation_closes_instead_of_losing_state() {
        let (sender, _receiver) = outbound_packet_channel_with_budget(8192);
        for _ in 0..OUTBOUND_PACKET_QUEUE_ENTRY_CAPACITY {
            assert!(
                sender
                    .try_send(OutboundPacket::Packet(encoded_packet(1)))
                    .is_ok()
            );
        }
        let (connection, _client) = test_java_connection(sender).await;

        connection.send_encoded_packet(encoded_packet(1));

        assert!(connection.closed());
    }

    #[tokio::test]
    async fn saturated_bundle_closes_without_enqueuing_either_delimiter() {
        let (sender, mut receiver) = outbound_packet_channel_with_budget(8192);
        for _ in 0..OUTBOUND_PACKET_QUEUE_ENTRY_CAPACITY - 1 {
            assert!(
                sender
                    .try_send(OutboundPacket::Packet(encoded_packet(1)))
                    .is_ok()
            );
        }
        let (connection, _client) = test_java_connection(sender).await;

        connection.send_encoded_packet_bundle(vec![encoded_packet(1)]);

        assert!(connection.closed());
        let mut queued = 0;
        while let Ok(packet) = receiver.try_recv() {
            queued += 1;
            drop(packet);
        }
        assert_eq!(queued, OUTBOUND_PACKET_QUEUE_ENTRY_CAPACITY - 1);
    }

    fn decode(packet: RawPacket) -> DecodedPlayPacket {
        let Ok(decoded) = JavaConnection::decode_play_packet(packet) else {
            panic!("test play packet should decode");
        };
        decoded
    }

    fn execution(kind: ScheduledPlayPacketKind) -> ScheduledPacketExecution {
        ScheduledPlayPacket(kind).execution()
    }

    #[test]
    fn pre_join_custom_payload_uses_serverbound_play_packet_id() {
        assert!(JavaConnection::can_process_before_join(
            play::S_CUSTOM_PAYLOAD
        ));
        assert!(!JavaConnection::can_process_before_join(
            play::C_CUSTOM_PAYLOAD
        ));
    }

    #[test]
    fn queued_domain_switch_records_only_perform_respawn_at_connection_gate() {
        let world = fresh_test_world("queued_domain_switch_respawn_packet");
        let player = TestPlayerBuilder::new(world, "RespawnTester", 1).build();
        let Some(token) = player.begin_pending_world_change() else {
            panic!("test player should acquire a world-change token");
        };
        assert!(player.begin_domain_switch(token));
        player.set_health(0.0);

        let request_stats = JavaConnection::decode_domain_gated_packet(
            RawPacket::new(
                play::S_CLIENT_COMMAND,
                vec![ClientCommandAction::RequestStats as u8],
            ),
            &player,
        );
        assert!(matches!(request_stats, Ok(None)));
        assert!(!player.has_deferred_death_respawn_for_test());

        let perform_respawn = JavaConnection::decode_domain_gated_packet(
            RawPacket::new(
                play::S_CLIENT_COMMAND,
                vec![ClientCommandAction::PerformRespawn as u8],
            ),
            &player,
        );
        assert!(matches!(perform_respawn, Ok(None)));
        assert!(player.has_deferred_death_respawn_for_test());

        assert!(player.finish_domain_switch(token));
        assert!(player.finish_pending_world_change(token));
    }

    #[test]
    fn ping_during_domain_maintenance_uses_player_local_scheduling() {
        let world = fresh_test_world("scheduled_domain_maintenance_ping");
        let player = TestPlayerBuilder::new(world, "PingTester", 1).build();
        let Some(token) = player.begin_pending_world_change() else {
            panic!("test player should acquire a world-change token");
        };
        assert!(player.begin_domain_switch(token));

        let decoded = JavaConnection::decode_domain_gated_packet(
            RawPacket::new(play::S_PING_REQUEST, 42_i64.to_be_bytes().to_vec()),
            &player,
        );
        let Ok(Some(DecodedPlayPacket::Scheduled(packet))) = decoded else {
            panic!("ping during domain maintenance should be scheduled");
        };

        assert_eq!(packet.execution(), ScheduledPacketExecution::PlayerLocal);

        assert!(player.finish_domain_switch(token));
        assert!(player.finish_pending_world_change(token));
    }

    #[test]
    fn custom_payload_defaults_to_global_exclusive_scheduling() {
        let channel = b"minecraft:brand";
        let mut payload = vec![channel.len() as u8];
        payload.extend_from_slice(channel);
        payload.extend_from_slice(b"foton");
        let decoded = decode(RawPacket::new(play::S_CUSTOM_PAYLOAD, payload));
        let DecodedPlayPacket::Scheduled(
            packet @ ScheduledPlayPacket(ScheduledPlayPacketKind::CustomPayload(_)),
        ) = decoded
        else {
            panic!("custom payload should use the scheduled packet path");
        };

        assert_eq!(packet.execution(), ScheduledPacketExecution::Exclusive);
    }

    #[test]
    fn pre_join_allows_initial_play_acknowledgements() {
        assert!(JavaConnection::can_process_before_join(
            play::S_ACCEPT_TELEPORTATION
        ));
        assert!(JavaConnection::can_process_before_join(
            play::S_CHUNK_BATCH_RECEIVED
        ));
        assert!(JavaConnection::can_process_before_join(
            play::S_PLAYER_LOADED
        ));
        assert!(JavaConnection::can_process_during_domain_handshake(
            play::S_ACCEPT_TELEPORTATION
        ));
        assert!(JavaConnection::can_process_during_domain_handshake(
            play::S_CHUNK_BATCH_RECEIVED
        ));
        assert!(JavaConnection::can_process_during_domain_handshake(
            play::S_PLAYER_LOADED
        ));
        assert!(!JavaConnection::can_process_during_domain_handshake(
            play::S_MOVE_PLAYER_POS
        ));
    }

    #[test]
    fn scheduled_domain_handshake_classification_is_narrow() {
        let accept = decode(RawPacket::new(play::S_ACCEPT_TELEPORTATION, vec![0]));
        let DecodedPlayPacket::Scheduled(accept) = accept else {
            panic!("teleport acknowledgement should be scheduled");
        };
        assert!(accept.is_domain_handshake_packet());

        let client_tick_end = decode(RawPacket::new(play::S_CLIENT_TICK_END, Vec::new()));
        let DecodedPlayPacket::Scheduled(client_tick_end) = client_tick_end else {
            panic!("client tick end should be scheduled");
        };
        assert!(!client_tick_end.is_domain_handshake_packet());
    }

    #[test]
    fn client_tick_end_is_scheduled_for_the_inter_tick_phase() {
        let decoded = decode(RawPacket::new(play::S_CLIENT_TICK_END, Vec::new()));

        assert!(matches!(
            decoded,
            DecodedPlayPacket::Scheduled(ScheduledPlayPacket(
                ScheduledPlayPacketKind::ClientTickEnd
            ))
        ));
    }

    #[test]
    fn packet_execution_classification_separates_local_and_serialized_work() {
        assert_eq!(
            execution(ScheduledPlayPacketKind::PlayerAbilities(SPlayerAbilities {
                flags: 0
            },)),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::MovePlayer(
                SMovePlayerStatusOnly { packed_byte: 0 }.into(),
            )),
            ScheduledPacketExecution::Serialized
        );
    }

    #[test]
    fn inventory_execution_reflects_complete_transaction_boundaries() {
        let click = SContainerClick {
            container_id: 0,
            state_id: 0,
            slot_num: 0,
            button_num: 0,
            click_type: ClickType::Pickup,
            changed_slots: FxHashMap::default(),
            carried_item: HashedStack::Empty,
        };

        assert_eq!(
            execution(ScheduledPlayPacketKind::ContainerClick(click)),
            ScheduledPacketExecution::Serialized
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::ContainerClose(SContainerClose {
                container_id: 0,
            })),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::SetCreativeModeSlot(
                SSetCreativeModeSlot {
                    slot_num: 1,
                    item_stack: ItemStack::empty(),
                },
            )),
            ScheduledPacketExecution::PlayerLocal
        );
    }

    #[test]
    fn player_command_execution_is_action_sensitive() {
        let command = |action| {
            execution(ScheduledPlayPacketKind::PlayerCommand(SPlayerCommand {
                entity_id: 1,
                action,
                data: 0,
            }))
        };

        assert_eq!(
            command(PlayerCommandAction::StartSprinting),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            command(PlayerCommandAction::StartFallFlying),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            command(PlayerCommandAction::LeaveBed),
            ScheduledPacketExecution::Serialized
        );
        assert_eq!(
            command(PlayerCommandAction::OpenVehicleInventory),
            ScheduledPacketExecution::Exclusive
        );
    }

    #[test]
    fn player_action_execution_is_action_sensitive() {
        let action = |action| {
            execution(ScheduledPlayPacketKind::PlayerAction(SPlayerAction {
                action,
                pos: BlockPos::new(0, 64, 0),
                direction: Direction::Down,
                sequence: 0,
            }))
        };

        assert_eq!(
            action(PlayerAction::AbortDestroyBlock),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            action(PlayerAction::SwapItemWithOffhand),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            action(PlayerAction::StartDestroyBlock),
            ScheduledPacketExecution::Serialized
        );
        assert_eq!(
            action(PlayerAction::Stab),
            ScheduledPacketExecution::Exclusive
        );
    }

    #[test]
    fn chat_message_and_ack_share_the_serialized_commit_lane() {
        assert_eq!(
            execution(ScheduledPlayPacketKind::Chat(Box::new(SChat {
                message: "hello".to_owned(),
                timestamp: 0,
                salt: 0,
                signature: None,
                offset: 0,
                acknowledged: [0; 3],
                checksum: 0,
            }))),
            ScheduledPacketExecution::Serialized
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::ChatAck(SChatAck {
                offset: VarInt(0),
            })),
            ScheduledPacketExecution::Serialized
        );
    }

    #[test]
    fn cross_player_and_unimplemented_handlers_remain_global_barriers() {
        assert_eq!(
            execution(ScheduledPlayPacketKind::Attack(SAttack { entity_id: 1 })),
            ScheduledPacketExecution::Exclusive
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::ContainerButtonClick(
                SContainerButtonClick {
                    container_id: 1,
                    button_id: 0,
                },
            )),
            ScheduledPacketExecution::Exclusive
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::PlayerAction(SPlayerAction {
                action: PlayerAction::ReleaseUseItem,
                pos: BlockPos::new(0, 64, 0),
                direction: Direction::Down,
                sequence: 0,
            })),
            ScheduledPacketExecution::Exclusive
        );
    }

    #[test]
    fn audited_handlers_use_the_narrowest_safe_execution_class() {
        assert_eq!(
            execution(ScheduledPlayPacketKind::AcceptTeleportation(
                SAcceptTeleportation { teleport_id: 1 },
            )),
            ScheduledPacketExecution::Serialized
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::PlayerInput(SPlayerInput {
                flags: 0,
            })),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::ChatSessionUpdate(
                SChatSessionUpdate {
                    session_id: Uuid::nil(),
                    expires_at: 0,
                    public_key: Vec::new(),
                    key_signature: Vec::new(),
                },
            )),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::ClientInformation(
                SClientInformation {
                    language: "en_us".to_owned(),
                    view_distance: 8,
                    chat_visibility: ChatVisibility::Full,
                    chat_colors: true,
                    model_customization: 0,
                    main_hand: HumanoidArm::Right,
                    text_filtering_enabled: false,
                    allows_listing: true,
                    particle_status: ParticleStatus::All,
                },
            )),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::ChatCommand(SChatCommand {
                command: "help".to_owned(),
            })),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::PickItemFromBlock(
                SPickItemFromBlock {
                    pos: BlockPos::new(0, 64, 0),
                    include_data: false,
                },
            )),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::SignUpdate(SSignUpdate {
                pos: BlockPos::new(0, 64, 0),
                is_front_text: true,
                lines: array::from_fn(|_| String::new()),
            })),
            ScheduledPacketExecution::Serialized
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::Swing(SSwing {
                hand: InteractionHand::MainHand,
            })),
            ScheduledPacketExecution::PlayerLocal
        );
        assert_eq!(
            execution(ScheduledPlayPacketKind::ClientCommand(SClientCommand {
                action: ClientCommandAction::PerformRespawn,
            })),
            ScheduledPacketExecution::PlayerLocal
        );
    }

    #[test]
    fn command_character_validation_runs_before_command_dispatch() {
        let command = |command: &str| {
            ScheduledPlayPacket(ScheduledPlayPacketKind::ChatCommand(SChatCommand {
                command: command.to_owned(),
            }))
        };

        assert!(!command("say bonjour 😀").has_illegal_command_characters());
        for illegal in ["say line\nforge", "say §cformat", "say \u{7f}"] {
            assert!(command(illegal).has_illegal_command_characters());
        }
    }

    #[test]
    fn keep_alive_remains_on_the_immediate_connection_path() {
        let decoded = decode(RawPacket::new(
            play::S_KEEP_ALIVE,
            42_i64.to_be_bytes().to_vec(),
        ));

        assert!(matches!(
            decoded,
            DecodedPlayPacket::Immediate(ImmediatePlayPacket::KeepAlive(SKeepAlive { id: 42 }))
        ));
    }

    #[test]
    fn backward_wall_clock_does_not_delay_monotonic_keep_alive_timeout() {
        let started_at = Instant::now();
        let mut tracker = KeepAliveTracker::new();

        assert_eq!(
            tracker.tick_at(started_at, 10_000),
            KeepAliveTick::Send(10_000)
        );
        assert_eq!(
            tracker.tick_at(started_at + Duration::from_secs(14), 1),
            KeepAliveTick::None
        );
        assert_eq!(
            tracker.tick_at(started_at + Duration::from_secs(15), 2),
            KeepAliveTick::Timeout
        );
    }

    #[test]
    fn keep_alive_round_trip_uses_monotonic_time_after_wall_clock_rollback() {
        let started_at = Instant::now();
        let mut tracker = KeepAliveTracker::new();

        assert_eq!(
            tracker.tick_at(started_at, 10_000),
            KeepAliveTick::Send(10_000)
        );
        assert_eq!(
            tracker.tick_at(started_at + Duration::from_millis(10), 1),
            KeepAliveTick::None
        );
        assert_eq!(
            tracker.acknowledge_at(10_000, started_at + Duration::from_millis(42)),
            Some(Duration::from_millis(42))
        );
    }

    #[test]
    fn wrong_keep_alive_id_keeps_pending_state_until_timeout() {
        let started_at = Instant::now();
        let mut tracker = KeepAliveTracker::new();

        assert_eq!(
            tracker.tick_at(started_at, 10_000),
            KeepAliveTick::Send(10_000)
        );
        assert_eq!(
            tracker.acknowledge_at(10_001, started_at + Duration::from_millis(10)),
            None
        );
        assert_eq!(
            tracker.tick_at(started_at + Duration::from_secs(15), 10_002),
            KeepAliveTick::Timeout
        );
    }

    #[test]
    fn valid_keep_alive_ack_clears_pending_state_for_the_next_probe() {
        let started_at = Instant::now();
        let mut tracker = KeepAliveTracker::new();

        assert_eq!(
            tracker.tick_at(started_at, 10_000),
            KeepAliveTick::Send(10_000)
        );
        assert_eq!(
            tracker.acknowledge_at(10_000, started_at + Duration::from_millis(10)),
            Some(Duration::from_millis(10))
        );
        assert_eq!(
            tracker.tick_at(started_at + Duration::from_secs(15), 10_001),
            KeepAliveTick::Send(10_001)
        );
    }

    #[test]
    fn chunk_batch_ack_uses_the_immediate_connection_path() {
        let decoded = decode(RawPacket::new(
            play::S_CHUNK_BATCH_RECEIVED,
            12.5_f32.to_be_bytes().to_vec(),
        ));

        assert!(matches!(
            decoded,
            DecodedPlayPacket::Immediate(ImmediatePlayPacket::ChunkBatchReceived(
                SChunkBatchReceived {
                    desired_chunks_per_tick: 12.5
                }
            ))
        ));
    }

    #[tokio::test]
    async fn final_disconnect_write_is_deadline_bounded() {
        let result = timeout(
            Duration::from_secs(1),
            with_disconnect_write_deadline(
                Duration::from_millis(20),
                pending::<Result<(), PacketError>>(),
            ),
        )
        .await;

        assert!(matches!(result, Ok(Err(PacketError::SendError(_)))));
    }
}
