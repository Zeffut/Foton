//! Pre-play TCP client connection handler.
//!
//! Handles the connection lifecycle from handshake through login and configuration,
//! until the connection is upgraded to play state.

use std::{
    cmp::Ordering,
    fmt::{self, Debug, Formatter},
    future::Future,
    io::Cursor,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use crossbeam::atomic::AtomicCell;
use foton_core::player::{
    ClientInformation, PlayerConnection,
    connection::{
        JavaNetworkWriter, OutboundPacket, OutboundPacketReceiver, OutboundPacketSender,
        outbound_packet_channel,
    },
};
use foton_core::server::Server;
use foton_protocol::{
    packet_reader::TCPNetworkDecoder,
    packet_traits::{
        ClientPacket, CompressionInfo, EncodedPacket, PacketDirection, PacketTranslation,
        ServerPacket, TranslationBatch,
    },
    packet_writer::TCPNetworkEncoder,
    packets::{
        common::{CDisconnect, SClientInformation, SCustomPayload, SPingRequest},
        config::SSelectKnownPacks,
        handshake::{ClientIntent, SClientIntention},
        login::{CLoginDisconnect, SHello, SKey},
    },
    utils::{ConnectionProtocol, PacketError, RawPacket},
};
use foton_registry::packets::{
    CURRENT_MC_PROTOCOL, config, handshake, login as login_packets, status,
};
use foton_utils::{
    MC_VERSION,
    locks::{AsyncMutex, SyncMutex},
    translations,
};
use text_components::{
    TextComponent, content::Resolvable, custom::CustomData, resolving::TextResolutor,
};
use tokio::{
    io::{BufReader, BufWriter},
    net::{TcpStream, tcp::OwnedReadHalf},
    select,
    sync::{
        Notify, OwnedSemaphorePermit, Semaphore,
        broadcast::{self, Sender, error::RecvError},
        mpsc,
    },
    time::{Instant, MissedTickBehavior, interval, sleep_until, timeout, timeout_at},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

use foton_bedrock::config::BedrockConfig;

use crate::pre_play_state::{PacketSequenceError, PrePlayPacket, PrePlayState};

/// How long a connection may stay silent before it is dropped.
///
/// Vanilla parity: the `new ReadTimeoutHandler(30)` that
/// `ServerConnectionListener` puts on every accepted channel. Foton had no
/// equivalent anywhere before the play phase, and the play phase is only
/// covered because keep-alive disconnects an unresponsive client. That left
/// handshake, status and login able to hold a socket, two tasks, two buffered
/// halves and a broadcast channel open forever while sending nothing at all --
/// and nothing existed to kick them, since bans and player caps come later.
const PRE_PLAY_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Absolute ceiling for handshake, login and configuration combined.
///
/// The read timeout above is intentionally idle-based for Vanilla parity, but
/// configuration packets and registry writes are progress too. Without a
/// separate absolute deadline, a peer can send one permitted packet before
/// each idle timeout and retain its pre-play resources forever.
const PRE_PLAY_LIFETIME_TIMEOUT: Duration = Duration::from_secs(120);

/// A peer that stops reading must not keep login or shutdown tasks alive.
const NETWORK_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum number of accepted Java sockets alive at the same time.
///
/// This is deliberately independent of `max_players`: status requests and
/// connections still negotiating login are not players yet, but each owns a
/// socket, buffered halves, channels and two tasks. Five hundred and twelve
/// leaves ample room for ordinary server-list traffic while putting a fixed
/// ceiling on the resources silent pre-play peers can retain for their 30
/// second timeout.
const MAX_LIVE_JAVA_CONNECTIONS: usize = 512;

const TRANSLATED_SERVERBOUND_CAPACITY: usize = 256;
const TRANSLATION_POLL_INTERVAL: Duration = Duration::from_millis(50);

fn translation_handoff<T>(
    upgrade: T,
    remaining: impl Iterator<Item = Vec<u8>>,
) -> (T, Vec<Vec<u8>>) {
    (upgrade, remaining.collect())
}

/// One admission permit shared by both halves of an accepted connection.
/// The semaphore slot is returned only after the final clone is dropped.
#[derive(Clone)]
struct ConnectionLifetimePermit {
    _permit: Arc<OwnedSemaphorePermit>,
}

impl ConnectionLifetimePermit {
    fn new(permit: OwnedSemaphorePermit) -> Self {
        Self {
            _permit: Arc::new(permit),
        }
    }
}

/// Represents updates to the connection state.
#[derive(Clone)]
pub enum ConnectionUpdate {
    /// Enable encryption on the connection.
    EnableEncryption([u8; 16]),
    /// Upgrade the connection to the play state.
    Upgrade(Arc<PlayerConnection>),
}

impl Debug for ConnectionUpdate {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnableEncryption(arg0) => f.debug_tuple("EnableEncryption").field(arg0).finish(),
            Self::Upgrade(_) => f.debug_tuple("Upgrade").finish(),
        }
    }
}

/// Session id owned by the active server connection listener
#[derive(Default)]
pub struct ServerConnectionSession {
    session_id: SyncMutex<Option<Uuid>>,
}

impl ServerConnectionSession {
    /// Returns the listener session id generating it on first use
    #[must_use]
    pub fn session_id(&self) -> Uuid {
        let mut session_id = self.session_id.lock();
        *session_id.get_or_insert_with(Uuid::new_v4)
    }
}

#[derive(Default)]
pub(crate) struct ConnectionAction {
    reader_encryption: Option<[u8; 16]>,
    reader_compression: Option<CompressionInfo>,
    upgrade: Option<Arc<PlayerConnection>>,
}

impl ConnectionAction {
    pub(crate) const fn none() -> Self {
        Self {
            reader_encryption: None,
            reader_compression: None,
            upgrade: None,
        }
    }

    pub(crate) const fn reader_compression(compression: CompressionInfo) -> Self {
        Self {
            reader_encryption: None,
            reader_compression: Some(compression),
            upgrade: None,
        }
    }

    pub(crate) const fn upgrade(connection: Arc<PlayerConnection>) -> Self {
        Self {
            reader_encryption: None,
            reader_compression: None,
            upgrade: Some(connection),
        }
    }

    pub(crate) const fn with_reader_encryption(mut self, key: [u8; 16]) -> Self {
        self.reader_encryption = Some(key);
        self
    }
}

/// Connection for pre-play packets.
///
/// Gets dropped by `incoming_packet_task` if closed or upgraded to play connection.
pub struct JavaTcpClient {
    /// The unique ID of the client.
    pub id: u64,
    /// The client's settings (view distance, language, etc.) received during config.
    pub client_information: AsyncMutex<ClientInformation>,
    /// The current connection state of the client (e.g., Handshaking, Status, Play).
    pub protocol: Arc<AtomicCell<ConnectionProtocol>>,
    /// The client's IP address.
    pub address: SocketAddr,
    /// A token to cancel the client's operations. Called when the connection is closed.
    pub cancel_token: CancellationToken,

    /// A queue of encoded packets to send to the network.
    pub outgoing_queue: OutboundPacketSender,
    /// The packet encoder for outgoing packets.
    pub network_writer: JavaNetworkWriter,
    /// Current compression settings.
    pub compression: Arc<AtomicCell<Option<CompressionInfo>>>,
    /// Optional Via pipeline retained unchanged through every protocol state.
    pub translation: Option<PacketTranslation>,
    pub(crate) translated_serverbound: mpsc::Sender<Vec<u8>>,
    translated_serverbound_recv: SyncMutex<Option<mpsc::Receiver<Vec<u8>>>>,

    /// The shared server state.
    pub server: Arc<Server>,
    /// The session id state for the active server connection listener
    pub connection_session: Arc<ServerConnectionSession>,
    /// The challenge sent to the client during login.
    pub challenge: AtomicCell<[u8; 4]>,

    /// Channel for broadcasting connection state updates.
    pub connection_updates: Sender<ConnectionUpdate>,
    /// Notification for when connection updates are processed.
    pub connection_updated: Arc<Notify>,

    pub(crate) pre_play_state: SyncMutex<PrePlayState>,
    /// The hostname the client's handshake declared, captured so the login
    /// handler can check it for an encrypted Floodgate payload.
    pub(crate) hostname: SyncMutex<String>,
    /// Bedrock login policy for this connection.
    ///
    /// Handed in at accept time from the server's `[server.bedrock]` section,
    /// so it is fixed for the life of the connection. Disabled means every
    /// hostname takes the ordinary Java path, including one carrying a
    /// Floodgate payload.
    pub(crate) bedrock: BedrockConfig,
    /// Admission slot retained by both network tasks through play shutdown.
    connection_lifetime: ConnectionLifetimePermit,
    /// Per-source admission retained only until pre-play finishes.
    ///
    /// This is type-erased so the listener can own its IP accounting without
    /// making `foton-login` depend on the binary crate. Dropping the client at
    /// the play handoff releases the source slot while the global lifetime
    /// permit continues with both network halves.
    _pre_play_source_permit: Box<dyn Send + Sync>,
    task_tracker: TaskTracker,
}

impl JavaTcpClient {
    /// Maximum number of accepted Java sockets alive at once.
    pub const MAX_LIVE_CONNECTIONS: usize = MAX_LIVE_JAVA_CONNECTIONS;

    /// Creates the one global admission limiter owned by the Java listener.
    #[must_use]
    pub fn connection_limiter() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(MAX_LIVE_JAVA_CONNECTIONS))
    }

    /// Attempts to reserve one live-connection slot without delaying accept.
    #[must_use]
    pub fn try_acquire_connection_slot(limiter: &Arc<Semaphore>) -> Option<OwnedSemaphorePermit> {
        Arc::clone(limiter).try_acquire_owned().ok()
    }

    /// Creates a new `JavaTcpClient`.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "every argument is a distinct piece of per-connection state the \
                  accept loop already holds; bundling them into a struct would move \
                  the same list one indirection away without removing anything"
    )]
    pub fn new<S>(
        tcp_stream: TcpStream,
        address: SocketAddr,
        id: u64,
        cancel_token: CancellationToken,
        server: Arc<Server>,
        connection_session: Arc<ServerConnectionSession>,
        task_tracker: TaskTracker,
        bedrock: BedrockConfig,
        connection_permit: OwnedSemaphorePermit,
        pre_play_source_permit: S,
        translation: Option<PacketTranslation>,
    ) -> (
        Self,
        OutboundPacketReceiver,
        TCPNetworkDecoder<BufReader<OwnedReadHalf>>,
    )
    where
        S: Send + Sync + 'static,
    {
        let (read, write) = tcp_stream.into_split();
        let (outgoing_queue, recv) = outbound_packet_channel();
        let (connection_updates, _) = broadcast::channel(128);
        let (translated_serverbound, translated_serverbound_recv) =
            mpsc::channel(TRANSLATED_SERVERBOUND_CAPACITY);

        let client = Self {
            id,
            client_information: AsyncMutex::new(ClientInformation::default()),
            address,
            protocol: Arc::new(AtomicCell::new(ConnectionProtocol::Handshake)),
            cancel_token,

            outgoing_queue,
            network_writer: Arc::new(AsyncMutex::new(Some(TCPNetworkEncoder::new(
                BufWriter::new(write),
            )))),
            compression: Arc::new(AtomicCell::new(None)),
            translation,
            translated_serverbound,
            translated_serverbound_recv: SyncMutex::new(Some(translated_serverbound_recv)),
            server,
            connection_session,
            challenge: AtomicCell::new([0; 4]),
            connection_updates,
            connection_updated: Arc::new(Notify::new()),
            pre_play_state: SyncMutex::new(PrePlayState::new()),
            hostname: SyncMutex::new(String::new()),
            bedrock,
            connection_lifetime: ConnectionLifetimePermit::new(connection_permit),
            _pre_play_source_permit: Box::new(pre_play_source_permit),
            task_tracker,
        };

        (client, recv, TCPNetworkDecoder::new(BufReader::new(read)))
    }

    /// Closes the connection.
    pub fn close(&self) {
        self.cancel_token.cancel();
    }

    /// Sends a packet immediately, without queuing.
    ///
    /// A packet that will not encode is dropped with a warning rather than
    /// taken as an invariant violation: `from_bare` refuses anything past
    /// `MAX_PACKET_SIZE`, and the status response carries an operator-supplied
    /// MOTD and favicon. The same call in `chunk_sender` already answers a
    /// too-large packet this way, and under the release profile's
    /// `panic = "abort"` the alternative is killing the server from the login
    /// path.
    pub async fn send_bare_packet_now<P: ClientPacket>(&self, packet: P) {
        let compression = self.compression.load();
        let protocol = self.protocol.load();
        let Ok(packet) = EncodedPacket::from_bare(packet, compression, protocol) else {
            log::warn!(
                "Client {}: dropping a packet that failed to encode",
                self.id
            );
            return;
        };

        match Self::write_network_packet_until_cancelled(
            &self.cancel_token,
            &self.network_writer,
            self.translation.as_ref(),
            &self.translated_serverbound,
            self.compression.load(),
            &packet,
        )
        .await
        {
            Ok(_) => {}
            Err(err) => {
                log::warn!("Failed to send packet to client {}: {}", self.id, err);
                self.close();
            }
        }
    }

    /// Sends an already encoded packet immediately, without queuing.
    pub async fn send_packet_now(&self, packet: &EncodedPacket) {
        match Self::write_network_packet_until_cancelled(
            &self.cancel_token,
            &self.network_writer,
            self.translation.as_ref(),
            &self.translated_serverbound,
            self.compression.load(),
            packet,
        )
        .await
        {
            Ok(_) => {}
            Err(err) => {
                log::warn!("Failed to send packet to client {}: {}", self.id, err);
                self.close();
            }
        }
    }

    /// Writes one immediate pre-play packet unless connection shutdown wins.
    ///
    /// Configuration registry synchronization uses immediate writes. A peer
    /// that acknowledged known packs and stopped reading could otherwise make
    /// shutdown wait for the per-write timeout once for every remaining
    /// registry packet.
    async fn write_network_packet_until_cancelled(
        cancel_token: &CancellationToken,
        network_writer: &JavaNetworkWriter,
        translation: Option<&PacketTranslation>,
        translated_serverbound: &mpsc::Sender<Vec<u8>>,
        compression: Option<CompressionInfo>,
        packet: &EncodedPacket,
    ) -> Result<bool, PacketError> {
        select! {
            biased;
            () = cancel_token.cancelled() => Ok(false),
            result = Self::write_clientbound_packet(network_writer, translation, translated_serverbound, compression, packet) => result.map(|()| true),
        }
    }

    async fn write_clientbound_packet(
        network_writer: &JavaNetworkWriter,
        translation: Option<&PacketTranslation>,
        translated_serverbound: &mpsc::Sender<Vec<u8>>,
        compression: Option<CompressionInfo>,
        packet: &EncodedPacket,
    ) -> Result<(), PacketError> {
        let Some(translation) = translation else {
            return Self::write_network_packet(network_writer, packet).await;
        };
        let data = packet.to_packet_data(compression)?;
        let batch = translation
            .exchange(PacketDirection::Clientbound, data)
            .await?;
        for serverbound in batch.serverbound {
            translated_serverbound.try_send(serverbound).map_err(|_| {
                PacketError::SendError("translated serverbound queue is full or closed".to_owned())
            })?;
        }
        for clientbound in batch.clientbound {
            let encoded = EncodedPacket::from_packet_data(&clientbound, compression)?;
            Self::write_network_packet(network_writer, &encoded).await?;
        }
        Ok(())
    }

    async fn write_translation_clientbound(
        &self,
        clientbound: Vec<Vec<u8>>,
    ) -> Result<(), PacketError> {
        let compression = self.compression.load();
        for packet in clientbound {
            let encoded = EncodedPacket::from_packet_data(&packet, compression)?;
            Self::write_network_packet(&self.network_writer, &encoded).await?;
        }
        Ok(())
    }

    async fn translate_serverbound(
        &self,
        packet: RawPacket,
    ) -> Result<TranslationBatch, PacketError> {
        let Some(translation) = &self.translation else {
            return Ok(TranslationBatch {
                serverbound: vec![packet.to_packet_data()?],
                clientbound: Vec::new(),
            });
        };
        translation
            .exchange(PacketDirection::Serverbound, packet.to_packet_data()?)
            .await
    }

    async fn write_network_packet(
        network_writer: &JavaNetworkWriter,
        packet: &EncodedPacket,
    ) -> Result<(), PacketError> {
        timeout(NETWORK_WRITE_TIMEOUT, async {
            let mut network_writer = network_writer.lock().await;
            let Some(network_writer) = network_writer.as_mut() else {
                return Err(PacketError::ConnectionClosed);
            };
            network_writer.write_packet(packet).await
        })
        .await
        .map_err(|_| PacketError::WriteTimeout)?
    }

    async fn release_network_writer(network_writer: &JavaNetworkWriter) {
        network_writer.lock().await.take();
    }

    /// Queues an already encoded packet to be sent.
    pub fn send_packet(&self, packet: EncodedPacket) -> Result<(), PacketError> {
        self.outgoing_queue
            .try_send(OutboundPacket::Packet(packet))
            .map_err(|e| {
                PacketError::SendError(format!(
                    "Failed to send packet to client {}: {}",
                    self.id, e
                ))
            })?;
        Ok(())
    }

    /// Starts a task that will send packets to the client from the outgoing packet queue.
    /// This task will run until the client is closed or the cancellation token is cancelled.
    #[cfg_attr(
        not(test),
        expect(
            clippy::unreachable,
            reason = "this task is only started for a connection this file built as PlayerConnection::Java; the other arm exists to keep the match exhaustive"
        )
    )]
    pub fn start_outgoing_packet_task(self: &Arc<Self>, mut sender_recv: OutboundPacketReceiver) {
        let cancel_token = self.cancel_token.clone();
        let network_writer = self.network_writer.clone();
        let id = self.id;
        let mut connection_updates_recv = self.connection_updates.subscribe();
        let connection_updated = self.connection_updated.clone();
        let connection_lifetime = self.connection_lifetime.clone();
        let translation = self.translation.clone();
        let translated_serverbound = self.translated_serverbound.clone();
        let compression = self.compression.clone();

        self.task_tracker.spawn(async move {
            let mut connection = None;
            loop {
                select! {
                    biased;
                    () = cancel_token.cancelled() => {
                        Self::write_queued_disconnect(&network_writer, translation.as_ref(), &translated_serverbound, compression.load(), &mut sender_recv, id).await;
                        break;
                    }
                    outbound = sender_recv.recv() => {
                        if let Some(outbound) = outbound {
                            let (packet, close_after_write) = outbound.write_parts();

                            if close_after_write {
                                if let Err(err) = Self::write_clientbound_packet(&network_writer, translation.as_ref(), &translated_serverbound, compression.load(), packet).await {
                                    log::warn!("Failed to send disconnect packet to client {id}: {err}");
                                }
                                cancel_token.cancel();
                                break;
                            }

                            let write_result = Self::write_clientbound_packet(&network_writer, translation.as_ref(), &translated_serverbound, compression.load(), packet);
                            select! {
                                biased;
                                () = cancel_token.cancelled() => {
                                    Self::write_queued_disconnect(&network_writer, translation.as_ref(), &translated_serverbound, compression.load(), &mut sender_recv, id).await;
                                    break;
                                },
                                result = write_result => {
                                    if let Err(err) = result {
                                        log::warn!("Failed to send packet to client {id}: {err}");
                                        cancel_token.cancel();
                                    }
                                }
                            }
                        } else {
                            cancel_token.cancel();
                        }
                    }
                    connection_update = connection_updates_recv.recv() => {
                        match connection_update {
                            Ok(connection_update) => {
                                match connection_update {
                                    ConnectionUpdate::EnableEncryption(key) => {
                                        let mut writer = network_writer.lock().await;
                                        let Some(writer) = writer.as_mut() else {
                                            cancel_token.cancel();
                                            continue;
                                        };
                                        writer.set_encryption(&key);
                                        connection_updated.notify_one();
                                    },
                                    ConnectionUpdate::Upgrade(upgrade) => {
                                        connection = Some(upgrade);
                                        connection_updated.notify_one();
                                        break;
                                    }
                                }
                            }
                            Err(err) => {
                                if err != RecvError::Closed {
                                    log::warn!("Internal connection_updates_recv channel closed for client {id}: {err}");
                                }
                                cancel_token.cancel();
                            }
                        }
                    }
                }
            }

            drop(cancel_token);
            drop(connection_updates_recv);
            drop(connection_updated);

            if let Some(connection) = connection {
                drop(network_writer);
                match &*connection {
                    PlayerConnection::Java(java) => java.sender(sender_recv).await,
                    PlayerConnection::Other(_) => unreachable!("Expected Java connection"),
                }
            } else {
                Self::release_network_writer(&network_writer).await;
                drop(network_writer);
                if let Some(translation) = &translation
                    && let Err(error) = translation.close().await
                {
                    log::debug!("Failed to close protocol translator for client {id}: {error}");
                }
            }
            drop(connection_lifetime);
        });
    }

    async fn write_queued_disconnect(
        network_writer: &JavaNetworkWriter,
        translation: Option<&PacketTranslation>,
        translated_serverbound: &mpsc::Sender<Vec<u8>>,
        compression: Option<CompressionInfo>,
        sender_recv: &mut OutboundPacketReceiver,
        id: u64,
    ) {
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
        if let Err(err) = Self::write_clientbound_packet(
            network_writer,
            translation,
            translated_serverbound,
            compression,
            packet,
        )
        .await
        {
            log::warn!("Failed to send disconnect packet to client {id} during close: {err}");
        }
    }

    /// Starts a task that will receive packets from the client.
    /// This task will run until the client is closed or the cancellation token is cancelled.
    #[cfg_attr(
        not(test),
        expect(
            clippy::unreachable,
            reason = "this task is only started for a connection this file built as PlayerConnection::Java; the other arm exists to keep the match exhaustive"
        )
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "the select loop keeps the pre-play lifetime, network, Via and upgrade events in one place so their cancellation and handoff order stays visible"
    )]
    pub fn start_incoming_packet_task(
        self: &Arc<Self>,
        mut reader: TCPNetworkDecoder<BufReader<OwnedReadHalf>>,
    ) {
        let Some(mut translated_serverbound_recv) = self.translated_serverbound_recv.lock().take()
        else {
            self.close();
            return;
        };
        let cancel_token = self.cancel_token.clone();
        let id = self.id;
        let mut connection_updates_recv = self.connection_updates.subscribe();
        let connection_lifetime = self.connection_lifetime.clone();

        let self_clone = self.clone();

        self.task_tracker.spawn(async move {
            let pre_play_deadline = Instant::now() + PRE_PLAY_LIFETIME_TIMEOUT;
            let mut translation_poll = interval(TRANSLATION_POLL_INTERVAL);
            translation_poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
            let mut connection = None;
            loop {
                select! {
                    () = cancel_token.cancelled() => {
                        break;
                    }
                    () = sleep_until(pre_play_deadline) => {
                        Self::close_after_pre_play_timeout(id, pre_play_deadline, &cancel_token);
                        break;
                    }
                    packet = timeout_at(
                        pre_play_deadline.min(Instant::now() + PRE_PLAY_READ_TIMEOUT),
                        reader.get_raw_packet(),
                    ) => {
                        let Ok(packet) = packet else {
                            Self::close_after_pre_play_timeout(id, pre_play_deadline, &cancel_token);
                            break;
                        };
                        match packet {
                            Ok(packet) => {
                                let Some(processed) = Self::run_pre_play_step(&cancel_token, pre_play_deadline, async {
                                    let batch = self_clone.translate_serverbound(packet).await?;
                                    self_clone.process_translation_batch(batch, &mut reader).await
                                }).await else {
                                    cancel_token.cancel();
                                    break;
                                };
                                match processed {
                                    Ok(Some((upgrade, pending))) => { connection = Some((upgrade, pending)); break; }
                                    Ok(None) => {}
                                    Err(err) => {
                                        // Vanilla closes the channel on any
                                        // decode failure. This branch is the
                                        // pre-authentication twin of the one in
                                        // `player::connection::java`, and it is
                                        // the worse of the two: nothing here has
                                        // an identity yet, so logging and
                                        // reading the next packet handed anyone
                                        // an unmetered way to write to the
                                        // server's log file. The sibling arm
                                        // fifteen lines down already cancels.
                                        log::warn!(
                                            "Disconnecting client {id} after a packet it sent failed to decode: {err}",
                                        );
                                        cancel_token.cancel();
                                    }
                                }
                            }
                            Err(PacketError::ConnectionClosed) => {
                                // The ordinary way a client leaves. Worth a
                                // trace and nothing louder: logged as a
                                // failure, it buries the real errors and sends
                                // readers hunting for a protocol bug.
                                log::debug!("Client {id} closed the connection");
                                cancel_token.cancel();
                            }
                            Err(err) => {
                                log::warn!("Failed to get raw packet from client {id}: {err}");
                                cancel_token.cancel();
                            }
                        }
                    }
                    translated = translated_serverbound_recv.recv() => {
                        let Some(translated) = translated else { cancel_token.cancel(); break; };
                        match self_clone.process_translation_batch(
                            TranslationBatch { serverbound: vec![translated], clientbound: Vec::new() },
                            &mut reader,
                        ).await {
                            Ok(Some((upgrade, pending))) => { connection = Some((upgrade, pending)); break; }
                            Ok(None) => {}
                            Err(error) => { log::warn!("Client {id} Via output failed: {error}"); cancel_token.cancel(); }
                        }
                    }
                    _ = translation_poll.tick(), if self_clone.translation.is_some() => {
                        let result = match &self_clone.translation {
                            Some(translation) => translation.poll().await,
                            None => continue,
                        };
                        let processed = match result {
                            Ok(batch) => self_clone.process_translation_batch(batch, &mut reader).await,
                            Err(error) => Err(error),
                        };
                        match processed {
                            Ok(Some((upgrade, pending))) => { connection = Some((upgrade, pending)); break; }
                            Ok(None) => {}
                            Err(error) => { log::warn!("Client {id} Via scheduled output failed: {error}"); cancel_token.cancel(); }
                        }
                    }
                    connection_update = connection_updates_recv.recv() => {
                        match connection_update {
                            Ok(ConnectionUpdate::EnableEncryption(_)) => {}
                            Ok(ConnectionUpdate::Upgrade(upgrade)) => {
                                connection = Some((upgrade, Vec::new()));
                                break;
                            }
                            Err(err) => {
                                if err != RecvError::Closed {
                                    log::info!("Internal connection_updates_recv channel closed for client {id}: {err}");
                                }
                                cancel_token.cancel();
                            }
                        }
                    }
                }
            }

            drop(cancel_token);
            drop(connection_updates_recv);

            if let Some((connection, pending_serverbound)) = connection {
                let server = self_clone.server.clone();
                drop(self_clone);

                match &*connection {
                    PlayerConnection::Java(java) => java.listener(
                        reader,
                        server,
                        pending_serverbound,
                        translated_serverbound_recv,
                    ).await,
                    PlayerConnection::Other(_) => unreachable!("Expected Java connection"),
                }
            }
            drop(connection_lifetime);
        });
    }

    async fn process_translation_batch(
        &self,
        batch: TranslationBatch,
        reader: &mut TCPNetworkDecoder<BufReader<OwnedReadHalf>>,
    ) -> Result<Option<(Arc<PlayerConnection>, Vec<Vec<u8>>)>, PacketError> {
        self.write_translation_clientbound(batch.clientbound)
            .await?;
        let mut serverbound = batch.serverbound.into_iter();
        while let Some(packet) = serverbound.next() {
            let action = self
                .process_packet(RawPacket::from_packet_data(packet)?)
                .await?;
            if let Some(key) = action.reader_encryption {
                reader.set_encryption(&key);
            }
            if let Some(compression) = action.reader_compression {
                reader.set_compression(compression.threshold);
            }
            if let Some(upgrade) = action.upgrade {
                return Ok(Some(translation_handoff(upgrade, serverbound)));
            }
        }
        Ok(None)
    }

    fn close_after_pre_play_timeout(
        id: u64,
        pre_play_deadline: Instant,
        cancel_token: &CancellationToken,
    ) {
        if Instant::now() >= pre_play_deadline {
            log::debug!(
                "Client {id} exceeded the {}s total pre-play deadline; closing the connection",
                PRE_PLAY_LIFETIME_TIMEOUT.as_secs()
            );
        } else {
            log::debug!(
                "Client {id} sent nothing for {}s; closing the connection",
                PRE_PLAY_READ_TIMEOUT.as_secs()
            );
        }
        cancel_token.cancel();
    }

    async fn run_pre_play_step<F, T>(
        cancel_token: &CancellationToken,
        deadline: Instant,
        step: F,
    ) -> Option<T>
    where
        F: Future<Output = T>,
    {
        select! {
            biased;
            () = cancel_token.cancelled() => None,
            result = timeout_at(deadline, step) => result.ok(),
        }
    }

    async fn process_packet(&self, packet: RawPacket) -> Result<ConnectionAction, PacketError> {
        match self.protocol.load() {
            ConnectionProtocol::Handshake => {
                self.handle_handshake(packet).await?;
                Ok(ConnectionAction::none())
            }
            ConnectionProtocol::Status => {
                self.handle_status(packet).await?;
                Ok(ConnectionAction::none())
            }
            ConnectionProtocol::Login => self.handle_login(packet).await,
            ConnectionProtocol::Config => self.handle_config(packet).await,
            ConnectionProtocol::Play => Err(PacketError::InvalidProtocol("Play".to_string())),
        }
    }

    /// Handles a handshake packet.
    pub async fn handle_handshake(&self, packet: RawPacket) -> Result<(), PacketError> {
        let data = &mut Cursor::new(packet.payload());

        match packet.id {
            handshake::S_INTENTION => {
                let packet = SClientIntention::read_packet(data)?;
                // Captured here so the login handler can check it for an
                // encrypted Floodgate payload once the client reaches login.
                self.hostname.lock().clone_from(&packet.hostname);
                let intent = match packet.intention {
                    ClientIntent::Status => ConnectionProtocol::Status,
                    ClientIntent::Login | ClientIntent::Transfer => ConnectionProtocol::Login,
                };
                let sequence_result = self.pre_play_state.lock().select_protocol(intent);
                if let Err(error) = sequence_result {
                    log::warn!("Client {} {error}", self.id);
                    self.kick(TextComponent::translated(
                        translations::MULTIPLAYER_DISCONNECT_INVALID_PACKET.msg(),
                    ))
                    .await;
                    return Ok(());
                }
                self.protocol.store(intent);

                if intent != ConnectionProtocol::Status {
                    let reason = match packet.protocol_version.cmp(&CURRENT_MC_PROTOCOL) {
                        Ordering::Equal => return Ok(()),
                        Ordering::Less => TextComponent::translated(
                            translations::MULTIPLAYER_DISCONNECT_OUTDATED_CLIENT
                                .message([MC_VERSION]),
                        ),
                        Ordering::Greater => TextComponent::translated(
                            translations::MULTIPLAYER_DISCONNECT_INCOMPATIBLE.message([MC_VERSION]),
                        ),
                    };
                    self.kick(reason).await;
                    return Ok(());
                }
            }
            id => {
                log::error!("Received unexpected packet id: {id}");
                return Err(PacketError::InvalidProtocol(id.to_string()));
            }
        }
        Ok(())
    }

    /// Handles a status packet.
    pub async fn handle_status(&self, packet: RawPacket) -> Result<(), PacketError> {
        let data = &mut Cursor::new(packet.payload());

        match packet.id {
            status::S_STATUS_REQUEST => {
                let sequence_result = self.pre_play_state.lock().begin_status_request();
                if let Err(error) = sequence_result {
                    // Answering the second request is what makes the socket an
                    // amplifier, so the connection goes, exactly as vanilla's
                    // `hasRequestedStatus` branch does.
                    self.reject_unexpected_packet(error).await;
                    return Ok(());
                }
                self.handle_status_request().await;
            }
            status::S_PING_REQUEST => {
                self.handle_ping_request(SPingRequest::read_packet(data)?)
                    .await;
            }
            _ => return Err(PacketError::InvalidProtocol("Status".to_string())),
        }
        Ok(())
    }

    /// Handles a login packet.
    pub(crate) async fn handle_login(
        &self,
        packet: RawPacket,
    ) -> Result<ConnectionAction, PacketError> {
        let data = &mut Cursor::new(packet.payload());

        match packet.id {
            login_packets::S_HELLO => {
                if let Err(error) = self.expect_pre_play_packet(PrePlayPacket::Hello) {
                    return Ok(self.reject_unexpected_packet(error).await);
                }
                Ok(self.handle_hello(SHello::read_packet(data)?).await)
            }
            login_packets::S_KEY => {
                if let Err(error) = self.expect_pre_play_packet(PrePlayPacket::Key) {
                    return Ok(self.reject_unexpected_packet(error).await);
                }
                Ok(self.handle_key(SKey::read_packet(data)?).await)
            }
            login_packets::S_LOGIN_ACKNOWLEDGED => {
                if let Err(error) = self.expect_pre_play_packet(PrePlayPacket::LoginAcknowledged) {
                    return Ok(self.reject_unexpected_packet(error).await);
                }
                Ok(self.handle_login_acknowledged().await)
            }
            _ => Err(PacketError::InvalidProtocol("Login".to_string())),
        }
    }

    /// Handles a configuration packet.
    pub(crate) async fn handle_config(
        &self,
        packet: RawPacket,
    ) -> Result<ConnectionAction, PacketError> {
        let data = &mut Cursor::new(packet.payload());

        match packet.id {
            config::S_CUSTOM_PAYLOAD => {
                self.handle_config_custom_payload(SCustomPayload::read_packet(data)?);
                Ok(ConnectionAction::none())
            }
            config::S_CLIENT_INFORMATION => {
                self.handle_client_information(SClientInformation::read_packet(data)?)
                    .await;
                Ok(ConnectionAction::none())
            }
            config::S_SELECT_KNOWN_PACKS => {
                if let Err(error) = self.expect_pre_play_packet(PrePlayPacket::SelectKnownPacks) {
                    return Ok(self.reject_unexpected_packet(error).await);
                }
                self.handle_select_known_packs(SSelectKnownPacks::read_packet(data)?)
                    .await;
                Ok(ConnectionAction::none())
            }
            config::S_FINISH_CONFIGURATION => {
                if let Err(error) = self.expect_pre_play_packet(PrePlayPacket::FinishConfiguration)
                {
                    return Ok(self.reject_unexpected_packet(error).await);
                }
                Ok(self.finish_configuration().await)
            }
            _ => Err(PacketError::InvalidProtocol("Config".to_string())),
        }
    }

    fn expect_pre_play_packet(&self, packet: PrePlayPacket) -> Result<(), PacketSequenceError> {
        self.pre_play_state.lock().expect(packet)
    }

    pub(crate) async fn reject_unexpected_packet(
        &self,
        error: PacketSequenceError,
    ) -> ConnectionAction {
        log::warn!("Client {} {error}", self.id);
        self.kick(TextComponent::translated(
            translations::MULTIPLAYER_DISCONNECT_INVALID_PACKET.msg(),
        ))
        .await;
        ConnectionAction::none()
    }

    /// Kicks the client with a given reason.
    pub async fn kick(&self, reason: TextComponent) {
        log::info!("Kicking client {}: {:p}", self.id, reason);
        match self.protocol.load() {
            ConnectionProtocol::Login => {
                let packet = CLoginDisconnect::new(&reason, self);
                self.send_bare_packet_now(packet).await;
            }
            ConnectionProtocol::Play | ConnectionProtocol::Config => {
                let packet = CDisconnect::new(&reason, self);
                self.send_bare_packet_now(packet).await;
            }
            ConnectionProtocol::Handshake | ConnectionProtocol::Status => (),
        }
        log::debug!("Closing connection for {}", self.id);
        self.close();
    }
}

impl TextResolutor for JavaTcpClient {
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

#[cfg(test)]
mod tests {
    use foton_utils::FrontVec;
    use tokio::{task::yield_now, time::sleep};

    use super::*;

    #[test]
    fn translation_handoff_preserves_every_packet_after_upgrade_in_order() {
        let packets = vec![vec![1], vec![2], vec![3]];
        let (upgrade, pending) = translation_handoff("play", packets.into_iter());
        assert_eq!(upgrade, "play");
        assert_eq!(pending, vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn java_connection_limiter_rejects_after_the_documented_global_limit() {
        let limiter = JavaTcpClient::connection_limiter();
        let permits: Vec<_> = (0..MAX_LIVE_JAVA_CONNECTIONS)
            .map(|_| {
                JavaTcpClient::try_acquire_connection_slot(&limiter)
                    .expect("every slot up to the documented limit must be available")
            })
            .collect();

        assert!(JavaTcpClient::try_acquire_connection_slot(&limiter).is_none());
        drop(permits);
        assert!(JavaTcpClient::try_acquire_connection_slot(&limiter).is_some());
    }

    #[test]
    fn java_connection_slot_lives_until_both_network_tasks_finish() {
        let limiter = Arc::new(Semaphore::new(1));
        let admitted = JavaTcpClient::try_acquire_connection_slot(&limiter)
            .expect("the test limiter starts with one slot");
        let lifetime = ConnectionLifetimePermit::new(admitted);
        let incoming_task = lifetime.clone();
        let outgoing_task = lifetime.clone();
        drop(lifetime);

        assert!(JavaTcpClient::try_acquire_connection_slot(&limiter).is_none());
        drop(incoming_task);
        assert!(
            JavaTcpClient::try_acquire_connection_slot(&limiter).is_none(),
            "one completed half must not admit a replacement while the other half is alive"
        );
        drop(outgoing_task);
        assert!(JavaTcpClient::try_acquire_connection_slot(&limiter).is_some());
    }

    #[tokio::test]
    async fn shutdown_interrupts_immediate_configuration_write_waiting_for_writer() {
        let writer: JavaNetworkWriter = Arc::new(AsyncMutex::new(None));
        let writer_guard = writer.lock().await;
        let cancel_token = CancellationToken::new();
        let packet = EncodedPacket {
            encoded_data: Arc::new(FrontVec::new(0)),
        };
        let (translated, _translated_recv) = mpsc::channel(1);

        let write = JavaTcpClient::write_network_packet_until_cancelled(
            &cancel_token,
            &writer,
            None,
            &translated,
            None,
            &packet,
        );
        tokio::pin!(write);
        yield_now().await;
        cancel_token.cancel();

        assert!(
            !timeout(Duration::from_millis(100), &mut write)
                .await
                .expect("shutdown should interrupt a blocked configuration write")
                .expect("cancellation is not a network error")
        );
        drop(writer_guard);
    }

    #[tokio::test]
    async fn repeated_pre_play_progress_cannot_extend_the_absolute_deadline() {
        let cancel_token = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_millis(80);
        let mut completed_steps = 0;

        while JavaTcpClient::run_pre_play_step(&cancel_token, deadline, async {
            sleep(Duration::from_millis(20)).await;
        })
        .await
        .is_some()
        {
            completed_steps += 1;
        }

        assert!(
            completed_steps < 5,
            "successful intermediate work must not reset the absolute deadline"
        );
        assert!(
            Instant::now() < deadline + Duration::from_millis(100),
            "the absolute deadline should end the sequence promptly"
        );
    }
}
