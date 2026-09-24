//! This module is responsible for sending chunks to the client.
//!
//! Chunk sending runs on its own independent tick loop, separate from the game
//! tick. The three-phase design (prepare → encode → commit) minimizes lock hold
//! time on the per-player `ChunkSender` mutex so that game-tick operations like
//! `mark_chunk_pending_to_send` and `drop_chunk` are never blocked for long.
use rayon::{ThreadPool, prelude::*};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    fmt, mem,
    sync::{Arc, Weak},
};

use foton_protocol::packet_traits::{ClientPacket, CompressionInfo, EncodedPacket};
use foton_protocol::packets::game::{
    CChunkBatchFinished, CChunkBatchStart, CForgetLevelChunk, CLevelChunkWithLight,
};
use foton_protocol::utils::ConnectionProtocol;
use foton_utils::locks::SyncMutex;
use foton_utils::{ChunkPos, PackedChunkPos};
use tokio::sync::mpsc;

use crate::{
    chunk::{
        chunk_holder::{ChunkHolder, TickingReadinessSnapshot},
        status::ChunkStatus,
    },
    player::PlayerConnection,
    player::connection::NetworkConnection,
    world::World,
};

/// Minimum chunks per tick (vanilla: 0.01)
const MIN_CHUNKS_PER_TICK: f32 = 0.01f32;
/// Maximum chunks per tick (vanilla: 64.0)
const MAX_CHUNKS_PER_TICK: f32 = 64.0;
/// Starting chunks per tick (vanilla: 9.0)
const START_CHUNKS_PER_TICK: f32 = 9.0;
/// Maximum unacknowledged batches after first ack (vanilla: 10)
const MAX_UNACKNOWLEDGED_BATCHES: u16 = 10;

/// One chunk selected during the prepare phase.
pub struct PreparedChunk {
    /// Chunk position.
    pub pos: ChunkPos,
    /// Chunk holder to encode.
    pub holder: Arc<ChunkHolder>,
    /// Exact readiness generation observed while selecting the holder.
    readiness: TickingReadinessSnapshot,
}

/// Data collected during the prepare phase, used to encode and then commit.
pub struct PreparedBatch {
    /// Chunk holders to encode.
    pub chunks: Vec<PreparedChunk>,
    /// Whether the world dimension has a vanilla sky-light layer.
    pub has_skylight: bool,
    /// Snapshot of the player's generation counter at prepare time.
    pub epoch_snapshot: u32,
    reusable_encoded_chunks: SyncMutex<Vec<EncodedChunk>>,
}

/// Encoded chunk packet plus the holder and readiness generation it was built from.
#[derive(Clone)]
pub struct EncodedChunk {
    pos: ChunkPos,
    packet: EncodedPacket,
    content_revision: u64,
    holder: Weak<ChunkHolder>,
    readiness: TickingReadinessSnapshot,
}

struct EncodedChunkMetadata {
    pos: ChunkPos,
    content_revision: u64,
    holder: Weak<ChunkHolder>,
    readiness: TickingReadinessSnapshot,
}

impl EncodedChunk {
    fn is_current_for(&self, prepared: &PreparedChunk) -> bool {
        let Some(encoded_holder) = self.holder.upgrade() else {
            return false;
        };

        self.pos == prepared.pos
            && Arc::ptr_eq(&encoded_holder, &prepared.holder)
            && self.readiness == prepared.readiness
            && prepared.readiness.is_block_ticking()
            && prepared.holder.ticking_readiness_snapshot() == prepared.readiness
            && prepared.holder.packet_content_revision() == self.content_revision
    }

    fn into_parts(self) -> (EncodedChunkMetadata, EncodedPacket) {
        (
            EncodedChunkMetadata {
                pos: self.pos,
                content_revision: self.content_revision,
                holder: self.holder,
                readiness: self.readiness,
            },
            self.packet,
        )
    }
}

impl EncodedChunkMetadata {
    fn with_packet(self, packet: EncodedPacket) -> EncodedChunk {
        EncodedChunk {
            pos: self.pos,
            packet,
            content_revision: self.content_revision,
            holder: self.holder,
            readiness: self.readiness,
        }
    }
}

/// This struct is responsible for sending chunks to the client.
pub struct ChunkSender {
    /// A list of chunks that are waiting to be sent to the client.
    pub pending_chunks: FxHashSet<ChunkPos>,
    /// Chunks whose initial chunk packet has been queued for this client.
    sent_chunks: FxHashSet<ChunkPos>,
    /// The number of batches that have been sent to the client but have not been acknowledged yet.
    pub unacknowledged_batches: u16,
    /// The number of chunks that should be sent to the client per tick.
    /// This is dynamically adjusted based on client feedback.
    pub desired_chunks_per_tick: f32,
    /// The number of chunks that can be sent to the client in the current batch.
    pub batch_quota: f32,
    /// The maximum number of unacknowledged batches allowed.
    /// Starts at 1 and increases to `MAX_UNACKNOWLEDGED_BATCHES` after first ack.
    pub max_unacknowledged_batches: u16,
    deferred_encoded_chunks: Vec<EncodedChunk>,
}

impl fmt::Debug for ChunkSender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChunkSender")
            .field("pending_chunks", &self.pending_chunks)
            .field("sent_chunks", &self.sent_chunks)
            .field("unacknowledged_batches", &self.unacknowledged_batches)
            .field("desired_chunks_per_tick", &self.desired_chunks_per_tick)
            .field("batch_quota", &self.batch_quota)
            .field(
                "max_unacknowledged_batches",
                &self.max_unacknowledged_batches,
            )
            .field(
                "deferred_encoded_chunk_count",
                &self.deferred_encoded_chunks.len(),
            )
            .finish()
    }
}

impl ChunkSender {
    /// Marks a chunk as pending to be sent to the client.
    pub fn mark_chunk_pending_to_send(&mut self, pos: ChunkPos) {
        self.sent_chunks.remove(&pos);
        self.pending_chunks.insert(pos);
    }

    /// Drops a chunk from the client's view.
    pub fn drop_chunk(&mut self, connection: &PlayerConnection, pos: ChunkPos) {
        self.pending_chunks.remove(&pos);
        self.deferred_encoded_chunks
            .retain(|encoded| encoded.pos != pos);
        if self.sent_chunks.remove(&pos) && !connection.closed() {
            Self::send_packet(
                connection,
                CForgetLevelChunk {
                    pos: PackedChunkPos::from(pos),
                },
            );
        }
    }

    /// Encodes and sends a packet through the connection.
    fn send_packet<P: ClientPacket>(connection: &PlayerConnection, packet: P) {
        // `from_bare` refuses a payload past `MAX_PACKET_SIZE`, and under
        // `panic = "abort"` an `expect` here ends the server. The light-update
        // path in `chunk_map` already treats the identical error as a dropped
        // packet and a warning; there is no reason this one should be fatal.
        let Ok(encoded) =
            EncodedPacket::from_bare(packet, connection.compression(), ConnectionProtocol::Play)
        else {
            log::warn!("Failed to encode a chunk-sender packet; dropping it");
            return;
        };
        connection.send_encoded(encoded);
    }

    fn encode_packet<P: ClientPacket>(
        connection: &PlayerConnection,
        packet: P,
    ) -> Option<EncodedPacket> {
        match EncodedPacket::from_bare(packet, connection.compression(), ConnectionProtocol::Play) {
            Ok(encoded) => Some(encoded),
            Err(error) => {
                log::warn!("Failed to encode a chunk-batch boundary packet: {error}");
                None
            }
        }
    }

    fn try_send_batch(
        connection: &PlayerConnection,
        packets: Vec<EncodedPacket>,
    ) -> Result<(), mpsc::error::TrySendError<Vec<EncodedPacket>>> {
        if connection.closed() {
            return Err(mpsc::error::TrySendError::Closed(packets));
        }

        match connection {
            PlayerConnection::Java(java) => java.try_send_chunk_batch(packets),
            PlayerConnection::Other(other) => {
                for packet in packets {
                    other.send_encoded(packet);
                }
                if other.closed() {
                    Err(mpsc::error::TrySendError::Closed(Vec::new()))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn batch_byte_capacity(connection: &PlayerConnection) -> usize {
        match connection {
            PlayerConnection::Java(java) => java.chunk_batch_byte_capacity(),
            PlayerConnection::Other(_) => usize::MAX,
        }
    }

    /// Phase 1: Lock briefly to drain pending chunks and snapshot state.
    ///
    /// Returns `None` if there is nothing to send this tick.
    pub fn prepare_batch(
        &mut self,
        world: &Arc<World>,
        player_chunk_pos: ChunkPos,
        chunk_send_epoch: &SyncMutex<u32>,
    ) -> Option<PreparedBatch> {
        if self.unacknowledged_batches >= self.max_unacknowledged_batches {
            return None;
        }

        let max_batch_size = self.desired_chunks_per_tick.max(1.0);
        self.batch_quota = (self.batch_quota + self.desired_chunks_per_tick).min(max_batch_size);

        if self.batch_quota < 1.0 || self.pending_chunks.is_empty() {
            return None;
        }

        let holders = self.collect_candidates(world, player_chunk_pos);
        if holders.is_empty() {
            return None;
        }

        let epoch_snapshot = *chunk_send_epoch.lock();

        Some(PreparedBatch {
            chunks: holders,
            has_skylight: world.dimension_type.has_skylight,
            epoch_snapshot,
            reusable_encoded_chunks: SyncMutex::new(mem::take(&mut self.deferred_encoded_chunks)),
        })
    }

    /// Phase 2: Encode chunks without holding any lock. Called between prepare and commit.
    ///
    /// Uses the dedicated encoding pool to encode chunks in parallel. A per-tick
    /// local cache prevents multiple players sharing the same chunks from
    /// re-encoding them within the same sending tick.
    ///
    /// # Panics
    /// Panics if a chunk packet fails to encode.
    pub fn encode_batch(
        batch: &PreparedBatch,
        cache: &mut FxHashMap<ChunkPos, EncodedChunk>,
        compression: Option<CompressionInfo>,
        encoding_pool: &ThreadPool,
    ) -> Vec<EncodedChunk> {
        let reusable = mem::take(&mut *batch.reusable_encoded_chunks.lock());
        let mut reusable = reusable.into_iter();
        let mut encoded_prefix = Vec::new();
        for prepared in &batch.chunks {
            let Some(encoded) = reusable.next() else {
                break;
            };
            if !encoded.is_current_for(prepared) {
                break;
            }
            encoded_prefix.push(encoded);
        }

        let cached_chunks = &*cache;
        let encoded_chunks = encoding_pool.install(|| {
            batch
                .chunks
                .par_iter()
                .skip(encoded_prefix.len())
                .map(|prepared| {
                    let holder = &prepared.holder;
                    let pos = prepared.pos;

                    if let Some(cached) = cached_chunks.get(&pos)
                        && cached.is_current_for(prepared)
                    {
                        return Some(cached.clone());
                    }

                    if !prepared.readiness.is_block_ticking()
                        || holder.ticking_readiness_snapshot() != prepared.readiness
                    {
                        return None;
                    }
                    let revision_before = holder.packet_content_revision();
                    let chunk = holder.try_full_chunk()?;

                    // A chunk packet carries every section, the heightmaps, the
                    // light arrays and the NBT of every block entity in the
                    // column, so a chunk dense with signs, banners or command
                    // blocks can pass `MAX_PACKET_SIZE` -- and without
                    // compression that ceiling is only two megabytes. This runs
                    // on a rayon worker, where `panic = "abort"` makes one
                    // oversized chunk the end of the server. Skipping the chunk
                    // leaves that one column unsent, which is survivable.
                    let Ok(packet) = EncodedPacket::from_bare(
                        CLevelChunkWithLight {
                            x: pos.0.x,
                            z: pos.0.y,
                            chunk_data: chunk.extract_chunk_data(),
                            light_data: chunk.extract_light_data(batch.has_skylight),
                        },
                        compression,
                        ConnectionProtocol::Play,
                    ) else {
                        log::warn!("Failed to encode chunk packet for {pos:?}; skipping it");
                        return None;
                    };
                    let revision_after = holder.packet_content_revision();
                    if revision_before != revision_after
                        || holder.ticking_readiness_snapshot() != prepared.readiness
                    {
                        return None;
                    }

                    Some(EncodedChunk {
                        pos,
                        packet,
                        content_revision: revision_after,
                        holder: Arc::downgrade(holder),
                        readiness: prepared.readiness,
                    })
                })
                .collect::<Vec<_>>()
        });
        encoded_prefix.extend(encoded_chunks.into_iter().flatten());

        for encoded in &encoded_prefix {
            cache.insert(encoded.pos, encoded.clone());
        }

        encoded_prefix
    }

    /// Phase 3: Lock briefly to verify generation counter and send the batch.
    ///
    /// If the player teleported between prepare and commit (generation counter
    /// changed), the batch is discarded.
    pub fn commit_batch(
        &mut self,
        batch: &PreparedBatch,
        encoded_chunks: Vec<EncodedChunk>,
        connection: &PlayerConnection,
        chunk_send_epoch: &SyncMutex<u32>,
    ) -> Vec<ChunkPos> {
        let epoch = chunk_send_epoch.lock();
        if *epoch != batch.epoch_snapshot {
            return Vec::new();
        }
        drop(epoch);

        let mut valid_chunks = Vec::with_capacity(encoded_chunks.len());
        for encoded in encoded_chunks {
            if !self.pending_chunks.contains(&encoded.pos) {
                continue;
            }
            let Some(prepared) = batch.chunks.iter().find(|chunk| chunk.pos == encoded.pos) else {
                continue;
            };
            if !encoded.is_current_for(prepared) {
                continue;
            }
            valid_chunks.push(encoded);
        }

        if valid_chunks.is_empty() {
            return Vec::new();
        }

        let Some(start) = Self::encode_packet(connection, CChunkBatchStart {}) else {
            connection.close();
            return Vec::new();
        };
        let batch_capacity = Self::batch_byte_capacity(connection);
        let mut encoded_bytes = start.encoded_data.len().max(1);
        let mut packets = Vec::with_capacity(valid_chunks.len() + 2);
        let mut sent_chunks = Vec::with_capacity(valid_chunks.len());
        let mut selected_metadata = Vec::with_capacity(valid_chunks.len());
        let mut finished = None;
        packets.push(start);
        for encoded in valid_chunks {
            let next_batch_size = sent_chunks.len() + 1;
            let Ok(next_batch_size) = i32::try_from(next_batch_size) else {
                break;
            };
            let Some(next_finished) = Self::encode_packet(
                connection,
                CChunkBatchFinished {
                    batch_size: next_batch_size,
                },
            ) else {
                connection.close();
                return Vec::new();
            };
            let Some(next_encoded_bytes) = encoded_bytes
                .checked_add(encoded.packet.encoded_data.len().max(1))
                .and_then(|bytes| bytes.checked_add(next_finished.encoded_data.len().max(1)))
            else {
                break;
            };
            if next_encoded_bytes > batch_capacity {
                break;
            }

            encoded_bytes = next_encoded_bytes - next_finished.encoded_data.len().max(1);
            let (metadata, packet) = encoded.into_parts();
            sent_chunks.push(metadata.pos);
            selected_metadata.push(metadata);
            packets.push(packet);
            finished = Some(next_finished);
        }
        let Some(finished) = finished else {
            log::warn!("A chunk packet exceeded the outbound batch byte budget; deferring it");
            return Vec::new();
        };
        packets.push(finished);

        match Self::try_send_batch(connection, packets) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(returned_packets)) => {
                let mut returned_packets = returned_packets.into_iter();
                let _boundary = returned_packets.next();
                self.deferred_encoded_chunks.extend(
                    selected_metadata
                        .into_iter()
                        .zip(returned_packets)
                        .map(|(metadata, packet)| metadata.with_packet(packet)),
                );
                return Vec::new();
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                connection.close();
                return Vec::new();
            }
        }

        self.unacknowledged_batches += 1;
        self.batch_quota -= sent_chunks.len() as f32;

        for pos in &sent_chunks {
            self.pending_chunks.remove(pos);
            self.sent_chunks.insert(*pos);
        }
        sent_chunks
    }

    fn collect_candidates(
        &mut self,
        world: &Arc<World>,
        player_chunk_pos: ChunkPos,
    ) -> Vec<PreparedChunk> {
        let max_batch_size = self.batch_quota.floor() as usize;
        let mut candidates: Vec<ChunkPos> = self.pending_chunks.iter().copied().collect();

        // Sort by distance to player
        candidates.sort_by_key(|pos| Self::chunk_distance_squared(*pos, player_chunk_pos));

        let mut chunks_to_send = Vec::new();

        for pos in candidates {
            if chunks_to_send.len() >= max_batch_size {
                break;
            }

            if let Some(holder) = world
                .chunk_map
                .chunks
                .read_sync(&pos, |_, chunk| chunk.clone())
                && holder.published_status() == Some(ChunkStatus::Full)
            {
                let readiness = holder.ticking_readiness_snapshot();
                if readiness.is_block_ticking() {
                    chunks_to_send.push(PreparedChunk {
                        pos,
                        holder,
                        readiness,
                    });
                }
            }
        }
        chunks_to_send
    }

    fn chunk_distance_squared(pos: ChunkPos, player_chunk_pos: ChunkPos) -> u64 {
        let dx = u64::from(pos.0.x.abs_diff(player_chunk_pos.0.x));
        let dz = u64::from(pos.0.y.abs_diff(player_chunk_pos.0.y));
        dx.saturating_mul(dx).saturating_add(dz.saturating_mul(dz))
    }

    /// Handles the acknowledgement of a chunk batch from the client.
    ///
    /// The client sends back its desired chunks per tick based on how fast it can
    /// process chunks. We clamp this value and use it to adjust our sending rate.
    pub const fn on_chunk_batch_received_by_client(
        &mut self,
        desired_chunks_per_tick: f32,
    ) -> bool {
        if self.unacknowledged_batches == 0 {
            return false;
        }

        self.unacknowledged_batches = self.unacknowledged_batches.saturating_sub(1);

        // Vanilla clamps this client-controlled pacing value to 0.01..=64.0.
        self.desired_chunks_per_tick = if desired_chunks_per_tick.is_nan() {
            MIN_CHUNKS_PER_TICK
        } else {
            desired_chunks_per_tick.clamp(MIN_CHUNKS_PER_TICK, MAX_CHUNKS_PER_TICK)
        };

        // Reset batch quota when all batches are acknowledged
        if self.unacknowledged_batches == 0 {
            self.batch_quota = 1.0;
        }

        // After receiving the first acknowledgement, allow more unacknowledged batches
        // for better pipelining (vanilla behavior)
        self.max_unacknowledged_batches = MAX_UNACKNOWLEDGED_BATCHES;
        true
    }

    /// Returns whether the client has been queued the initial chunk packet.
    #[must_use]
    pub fn is_chunk_sent(&self, pos: ChunkPos) -> bool {
        self.sent_chunks.contains(&pos)
    }

    /// Returns a snapshot of all sent chunks for this player.
    #[must_use]
    pub fn sent_chunks_snapshot(&self) -> FxHashSet<ChunkPos> {
        self.sent_chunks.clone()
    }

    #[cfg(test)]
    pub(crate) fn mark_chunk_sent_for_test(&mut self, pos: ChunkPos) {
        self.pending_chunks.remove(&pos);
        self.sent_chunks.insert(pos);
    }
}

impl Default for ChunkSender {
    fn default() -> Self {
        Self {
            pending_chunks: FxHashSet::default(),
            sent_chunks: FxHashSet::default(),
            unacknowledged_batches: 0,
            desired_chunks_per_tick: START_CHUNKS_PER_TICK,
            batch_quota: 0.0,
            max_unacknowledged_batches: 1,
            deferred_encoded_chunks: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::init_behaviors;
    use crate::chunk::{
        Chunk,
        chunk_holder::TickingReadiness,
        chunk_ticket_manager::ChunkTicketLevel,
        heightmap::ChunkHeightmaps,
        light::ChunkLightData,
        section::{ChunkSection, Sections},
    };
    use crate::world::tick_scheduler::{BlockTickList, FluidTickList};
    use crate::{
        player::connection::{JavaConnection, outbound_packet_channel},
        test_support::{fresh_test_world, insert_ready_full_chunk},
    };
    use foton_protocol::packet_writer::TCPNetworkEncoder;
    use foton_registry::init_vanilla_registry;
    use foton_utils::{FrontVec, locks::AsyncMutex};
    use foton_worldgen::structure::{StructureReferenceMap, StructureStartMap};
    use std::sync::Weak;
    use tokio::io::BufWriter;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::runtime::Builder;
    use tokio_util::sync::CancellationToken;

    fn encoded_packet(byte_len: usize) -> EncodedPacket {
        let mut encoded_data = FrontVec::new(0);
        encoded_data.extend_from_slice(&vec![0; byte_len]);
        EncodedPacket {
            encoded_data: Arc::new(encoded_data),
        }
    }

    fn prepared_full_chunk(pos: ChunkPos) -> PreparedChunk {
        let chunk = Chunk::from_full_disk(
            Sections::from_owned(vec![ChunkSection::new_empty()].into_boxed_slice()),
            pos,
            0,
            16,
            Weak::new(),
            BlockTickList::new(),
            FluidTickList::new(),
            ChunkHeightmaps::new(0, 16),
            Vec::new(),
            StructureStartMap::default(),
            StructureReferenceMap::default(),
            ChunkLightData::for_valid_world_height(0, 16),
        );
        let holder = Arc::new(ChunkHolder::new(
            pos,
            ChunkTicketLevel::FULL_CHUNK,
            Some(ChunkTicketLevel::FULL_CHUNK),
            0,
            16,
        ));
        holder.insert_chunk(chunk, ChunkStatus::Full);
        holder.transition_ticking_readiness(TickingReadiness::BlockTicking);
        let readiness = holder.ticking_readiness_snapshot();
        PreparedChunk {
            pos,
            holder,
            readiness,
        }
    }

    #[test]
    fn parallel_chunk_encoding_preserves_batch_order_and_cache_entries() {
        init_vanilla_registry();
        init_behaviors();
        let positions = [
            ChunkPos::new(3, -2),
            ChunkPos::new(-1, 4),
            ChunkPos::new(8, 5),
            ChunkPos::new(0, 0),
        ];
        let batch = PreparedBatch {
            chunks: positions.into_iter().map(prepared_full_chunk).collect(),
            has_skylight: true,
            epoch_snapshot: 0,
            reusable_encoded_chunks: SyncMutex::new(Vec::new()),
        };
        let encoding_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("test chunk encoding pool should initialize");
        let mut cache = FxHashMap::default();

        let encoded = ChunkSender::encode_batch(&batch, &mut cache, None, &encoding_pool);

        assert_eq!(
            encoded.iter().map(|chunk| chunk.pos).collect::<Vec<_>>(),
            positions
        );
        assert_eq!(cache.len(), positions.len());
        for chunk in &encoded {
            let cached = cache
                .get(&chunk.pos)
                .expect("every encoded chunk should be cached");
            assert!(Arc::ptr_eq(
                &cached.packet.encoded_data,
                &chunk.packet.encoded_data
            ));
        }

        let encoded_again = ChunkSender::encode_batch(&batch, &mut cache, None, &encoding_pool);
        for (first, second) in encoded.iter().zip(&encoded_again) {
            assert_eq!(first.pos, second.pos);
            assert!(Arc::ptr_eq(
                &first.packet.encoded_data,
                &second.packet.encoded_data
            ));
        }
    }

    #[test]
    fn readiness_demotion_invalidates_prepared_chunk_encoding() {
        init_vanilla_registry();
        init_behaviors();
        let prepared = prepared_full_chunk(ChunkPos::new(4, -7));
        prepared
            .holder
            .transition_ticking_readiness(TickingReadiness::Unready);
        let batch = PreparedBatch {
            chunks: vec![prepared],
            has_skylight: true,
            epoch_snapshot: 0,
            reusable_encoded_chunks: SyncMutex::new(Vec::new()),
        };
        let encoding_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("test chunk encoding pool should initialize");
        let mut cache = FxHashMap::default();

        assert!(ChunkSender::encode_batch(&batch, &mut cache, None, &encoding_pool).is_empty());
        assert!(cache.is_empty());
    }

    #[test]
    fn encoding_cache_requires_holder_identity_and_exact_readiness_generation() {
        init_vanilla_registry();
        init_behaviors();
        let pos = ChunkPos::new(-5, 9);
        let first_batch = PreparedBatch {
            chunks: vec![prepared_full_chunk(pos)],
            has_skylight: true,
            epoch_snapshot: 0,
            reusable_encoded_chunks: SyncMutex::new(Vec::new()),
        };
        let encoding_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("test chunk encoding pool should initialize");
        let mut cache = FxHashMap::default();

        let first = ChunkSender::encode_batch(&first_batch, &mut cache, None, &encoding_pool);
        assert_eq!(first.len(), 1);

        let replacement_batch = PreparedBatch {
            chunks: vec![prepared_full_chunk(pos)],
            has_skylight: true,
            epoch_snapshot: 0,
            reusable_encoded_chunks: SyncMutex::new(Vec::new()),
        };
        let replacement =
            ChunkSender::encode_batch(&replacement_batch, &mut cache, None, &encoding_pool);
        assert_eq!(replacement.len(), 1);
        assert!(!Arc::ptr_eq(
            &first[0].packet.encoded_data,
            &replacement[0].packet.encoded_data
        ));

        let holder = Arc::clone(&replacement_batch.chunks[0].holder);
        holder.transition_ticking_readiness(TickingReadiness::Unready);
        holder.transition_ticking_readiness(TickingReadiness::BlockTicking);
        let rebound_batch = PreparedBatch {
            chunks: vec![PreparedChunk {
                pos,
                readiness: holder.ticking_readiness_snapshot(),
                holder,
            }],
            has_skylight: true,
            epoch_snapshot: 0,
            reusable_encoded_chunks: SyncMutex::new(Vec::new()),
        };
        let rebound = ChunkSender::encode_batch(&rebound_batch, &mut cache, None, &encoding_pool);
        assert_eq!(rebound.len(), 1);
        assert!(!Arc::ptr_eq(
            &replacement[0].packet.encoded_data,
            &rebound[0].packet.encoded_data
        ));
    }

    #[test]
    fn chunk_batch_ack_without_outstanding_batch_does_not_update_pacing() {
        let mut sender = ChunkSender::default();

        assert!(!sender.on_chunk_batch_received_by_client(64.0));
        assert_eq!(sender.unacknowledged_batches, 0);
        assert_eq!(
            sender.desired_chunks_per_tick.to_bits(),
            START_CHUNKS_PER_TICK.to_bits()
        );
        assert_eq!(sender.batch_quota.to_bits(), 0.0_f32.to_bits());
        assert_eq!(sender.max_unacknowledged_batches, 1);
    }

    #[test]
    fn chunk_batch_ack_updates_pacing_for_outstanding_batch() {
        let mut sender = ChunkSender {
            unacknowledged_batches: 1,
            ..ChunkSender::default()
        };

        assert!(sender.on_chunk_batch_received_by_client(f32::NAN));
        assert_eq!(sender.unacknowledged_batches, 0);
        assert_eq!(sender.desired_chunks_per_tick.to_bits(), 0.01_f32.to_bits());
        assert_eq!(sender.batch_quota.to_bits(), 1.0_f32.to_bits());
        assert_eq!(
            sender.max_unacknowledged_batches,
            MAX_UNACKNOWLEDGED_BATCHES
        );
    }

    #[test]
    fn chunk_batch_ack_clamps_non_finite_and_out_of_range_rates() {
        for (requested, expected) in [
            (f32::NAN, 0.01_f32),
            (f32::NEG_INFINITY, 0.01_f32),
            (-1.0_f32, 0.01_f32),
            (32.0_f32, 32.0_f32),
            (f32::INFINITY, 64.0_f32),
            (500.0_f32, 64.0_f32),
        ] {
            let mut sender = ChunkSender {
                unacknowledged_batches: 1,
                ..ChunkSender::default()
            };

            assert!(sender.on_chunk_batch_received_by_client(requested));
            assert_eq!(sender.desired_chunks_per_tick.to_bits(), expected.to_bits());
        }
    }

    #[test]
    fn marking_chunk_pending_clears_sent_state() {
        let mut sender = ChunkSender::default();
        let pos = ChunkPos::new(2, -3);
        sender.sent_chunks.insert(pos);

        sender.mark_chunk_pending_to_send(pos);

        assert!(sender.pending_chunks.contains(&pos));
        assert!(!sender.is_chunk_sent(pos));
    }

    #[test]
    fn saturated_commit_defers_state_and_reuses_encoding_after_drain() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("chunk-sender-saturation");
        let pos = ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, pos);
        let epoch = SyncMutex::new(0);
        let mut chunk_sender = ChunkSender::default();
        chunk_sender.mark_chunk_pending_to_send(pos);
        let batch = chunk_sender
            .prepare_batch(&world, pos, &epoch)
            .expect("the ready chunk should form a batch");
        let quota_before_commit = chunk_sender.batch_quota;
        let encoding_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("test chunk encoding pool should initialize");
        let mut cache = FxHashMap::default();
        let encoded = ChunkSender::encode_batch(&batch, &mut cache, None, &encoding_pool);
        let Some(first_encoded) = encoded.first() else {
            panic!("the ready chunk should encode");
        };
        let original_packet = Arc::downgrade(&first_encoded.packet.encoded_data);

        let (outgoing, mut receiver) = outbound_packet_channel();
        for _ in 0..256 {
            assert!(
                outgoing
                    .try_send_chunk_batch(vec![encoded_packet(1)])
                    .is_ok()
            );
        }
        let runtime = Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("test network runtime should initialize");
        let (_client, server_stream, remote_address) = runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("test listener should bind");
            let address = listener
                .local_addr()
                .expect("test listener should have an address");
            let (client, accepted) = tokio::join!(TcpStream::connect(address), listener.accept());
            let client = client.expect("test client should connect");
            let (server_stream, remote_address) = accepted.expect("test listener should accept");
            (client, server_stream, remote_address)
        });
        let (_read_half, write_half) = server_stream.into_split();
        let connection = PlayerConnection::Java(JavaConnection::new(
            outgoing,
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
        ));

        let committed = chunk_sender.commit_batch(&batch, encoded, &connection, &epoch);
        assert!(committed.is_empty());
        assert!(chunk_sender.pending_chunks.contains(&pos));
        assert!(!chunk_sender.is_chunk_sent(pos));
        assert_eq!(chunk_sender.unacknowledged_batches, 0);
        assert_eq!(
            chunk_sender.batch_quota.to_bits(),
            quota_before_commit.to_bits()
        );
        cache.clear();

        for _ in 0..3 {
            let queued = receiver
                .try_recv()
                .expect("the saturated queue should contain filler packets");
            drop(queued);
        }
        let retry_batch = chunk_sender
            .prepare_batch(&world, pos, &epoch)
            .expect("the deferred chunk should retry after drain");
        let retry_encoded =
            ChunkSender::encode_batch(&retry_batch, &mut cache, None, &encoding_pool);
        let Some(retry_packet) = retry_encoded.first() else {
            panic!("the deferred chunk should retain its encoding");
        };
        let Some(original_packet) = original_packet.upgrade() else {
            panic!("the deferred batch should retain the encoded packet");
        };
        assert!(Arc::ptr_eq(
            &original_packet,
            &retry_packet.packet.encoded_data
        ));

        let committed = chunk_sender.commit_batch(&retry_batch, retry_encoded, &connection, &epoch);
        assert_eq!(committed, vec![pos]);
        assert!(!chunk_sender.pending_chunks.contains(&pos));
        assert!(chunk_sender.is_chunk_sent(pos));
        assert_eq!(chunk_sender.unacknowledged_batches, 1);
        assert_eq!(
            chunk_sender.batch_quota.to_bits(),
            (quota_before_commit - 1.0).to_bits()
        );
    }

    #[test]
    fn chunk_distance_squared_handles_far_chunk_coordinates() {
        let distance = ChunkSender::chunk_distance_squared(
            ChunkPos::new(1_250_000, -1_250_000),
            ChunkPos::new(0, 0),
        );

        assert_eq!(distance, 3_125_000_000_000);
    }

    #[test]
    fn chunk_distance_squared_handles_valid_world_extremes() {
        let max = ChunkPos::MAX_COORDINATE_VALUE;
        let delta = u64::from(max.abs_diff(-max));
        let expected = delta * delta * 2;

        let distance =
            ChunkSender::chunk_distance_squared(ChunkPos::new(max, max), ChunkPos::new(-max, -max));

        assert_eq!(distance, expected);
    }

    #[test]
    fn chunk_distance_squared_saturates_for_invalid_i32_extremes() {
        let distance = ChunkSender::chunk_distance_squared(
            ChunkPos::new(i32::MIN, i32::MIN),
            ChunkPos::new(i32::MAX, i32::MAX),
        );

        assert_eq!(distance, u64::MAX);
    }
}
