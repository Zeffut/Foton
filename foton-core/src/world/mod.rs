//! This module contains the `World` struct, which represents a world.

use std::{
    io, mem,
    path::Path,
    sync::{
        Arc, LazyLock, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::chunk::chunk_ticket_manager::{PersistentChunkTickets, TimedChunkTickets};
use crate::chunk::full_chunk::{FullChunkBlockSetResult, FullChunkRef};
use crate::chunk::gameplay_chunk_lookup_cache::GameplayChunkLookupCacheScope;
use crate::chunk::light::{
    LightLayer, LightSectionEmptinessChange, MAX_LIGHT_LEVEL, has_different_light_properties,
};
use crate::chunk::status::ChunkStatus;
use crate::dimension::end::EnderDragonFight;
use crate::dimension::end::fight::PersistentEnderDragonFight;
use crate::poi::OccupationStatus;
use crate::portal::WorldChangeRequest;
use crate::raid::{PersistentRaids, Raids};
use crate::server::Server;
use crate::world::game_event::{
    DynamicListenerAction, GameEventContext, GameEventDispatcher, GameEventListenerCount,
    GameEventListenerStorage, SharedGameEventListener,
};
use crate::{chunk::chunk_map::ChunkMapGameTickTimings, world::weather::Weather};
use foton_utils::saved_data::{SavedDataManager, names as saved_data_names};

use foton_protocol::packets::game::{
    CBlockDestruction, CChangeDifficulty, CGameEvent, CInitializeBorder, CLevelEvent,
    CLevelParticles, CPlayerChat, CSetBorderCenter, CSetBorderLerpSize, CSetBorderSize,
    CSetBorderWarningDelay, CSetBorderWarningDistance, CSetEntityData, CSetEntityLink,
    CSetEquipment, CSound, CSystemChat, CUpdateAttributes, GameEventType, SoundSource,
};
use foton_protocol::utils::ConnectionProtocol;
use foton_protocol::{
    packet_traits::{ClientPacket, CompressionInfo, EncodedPacket},
    packets::game::CSetTime,
};
use glam::DVec3;
use sha2::{Digest, Sha256};

use foton_registry::biome::{BiomeRef, TemperatureModifier};
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{Axis, BlockStateProperties, Direction};
use foton_registry::blocks::shapes::{
    BooleanOp, OffsetVoxelShape, SupportType, VoxelShape, is_offset_face_full, is_shape_full_block,
    join_is_not_empty,
};
use foton_registry::fluid::{FluidRef, FluidState};
use foton_registry::game_events::GameEventRef;
use foton_registry::game_rules::{ErasedGameRuleRef, GameRule, GameRuleValue, GameRuleValueType};
use foton_registry::item_stack::ItemStack;
use foton_registry::level_events;
use foton_registry::loot_table::{BlockEntityRef, LootContext};
use foton_registry::particle_type::ParticleData;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_game_rules::{
    BLOCK_DROPS, GLOBAL_SOUND_EVENTS, PLAYERS_NETHER_PORTAL_DEFAULT_DELAY, RANDOM_TICK_SPEED,
};
use foton_registry::{REGISTRY, RegistryEntry, RegistryExt, dimension_type::DimensionTypeRef};
use foton_registry::{block_entity_type::BlockEntityTypeRef, vanilla_dimension_types};
use foton_registry::{
    blocks::BlockRef, vanilla_game_rules::ADVANCE_TIME, vanilla_game_rules::ADVANCE_WEATHER,
};
use foton_registry::{vanilla_blocks, vanilla_entities, vanilla_game_events, vanilla_poi_types};
use foton_utils::block_util::FoundRectangle;
use foton_utils::{
    Downcast as _,
    locks::{SyncMutex, SyncRwLock},
    random::{Random as _, RandomSource, legacy_random::LegacyRandom},
};
use foton_worldgen::{biomes::obfuscate_biome_seed, noise::PerlinSimplexNoise};
use rustc_hash::{FxHashMap, FxHashSet};
use simdnbt::owned::NbtCompound;

use foton_utils::{
    BlockLocalAabb, BlockPos, BlockStateId, ChunkPos, Identifier, PackedBlockPos, SectionPos,
    WorldAabb,
    types::{Difficulty, GameType, UpdateFlags},
};
use tokio::{runtime::Runtime, time::Instant};

use crate::{
    ChunkMap,
    behavior::{BLOCK_BEHAVIORS, BlockCollisionContext, BlockLootContext, FLUID_BEHAVIORS},
    block_entity::{BlockEntity, SharedBlockEntity, entities::EndGatewayBlockEntity},
    chunk::{heightmap::HeightmapType, player_chunk_view::PlayerChunkView},
    chunk_saver::{ChunkStorage, RamOnlyStorage, RegionManager},
    entity::{
        AddEntityError, Entity, EntityChangeSenders, EntityChunkCallback, EntityLifecycleChanges,
        EntityMovementSyncPacket, EntityOwnership, EntityTracker, EntityVisibility,
        InactiveEntityCallback, MobEffectSyncPacket, RemovalReason, SharedEntity,
        WorldEntityManager,
        entities::{ExperienceOrbEntity, ItemEntity, LightningBoltEntity},
        entity_loot_ref, next_entity_id,
    },
    fluid::{FluidStateExt as _, fluid_state_to_block},
    level_data::{LevelDataManager, RespawnData, WorldGenerationSettings},
    player::{LastSeen, Player, connection::NetworkConnection},
    poi::PointOfInterestStorage,
};

pub mod base_spawner;
mod biome_search;
mod block_entity_ticker;
mod block_event;
/// Matching multi-block shapes against the world.
pub mod block_pattern;
mod block_region;
mod block_updates;
mod border;
mod broadcasts;
pub(crate) mod clock;
pub mod difficulty;
mod entity_management;
mod environment;
mod events;
pub mod explosion;
/// Vanilla game-event contexts, listeners, and dispatch storage.
pub mod game_event;
mod level_effects;
mod level_reader;
mod loot_view;
mod mob_effects;
mod natural_spawn;
mod player_index;
pub(crate) mod player_spawn_finder;
mod portals;
mod properties;
mod raycast;
mod redstone;
mod signal_getter;
mod sleep;
mod sleep_status;
mod spawn;
pub mod spawn_placement;
pub mod spawn_util;
pub mod tick_scheduler;
mod village;
mod weather;
mod world_entities;
mod worldgen_level;

#[cfg(test)]
mod tests;

pub use crate::config::WorldStorageConfig;
use crate::worldgen::generators::vanilla::fuzzed_biome_at_block;
use crate::worldgen::{ChunkGenerator, ChunkGeneratorType};
use block_event::BlockEventQueue;
pub(crate) use block_region::{BlockRegionBounds, MAX_BLOCK_REGION_WORKSET_SLOTS};
use block_updates::CollectingNeighborUpdater;
pub use border::WorldBorderError;
use border::{WorldBorder, WorldBorderSnapshot};
use entity_management::NavigatingMobTracker;
#[cfg(test)]
use entity_management::nearest_player_distance_in_range;
pub use level_reader::{LevelAccessor, LevelReader, ScheduledTickAccess};
pub use player_index::{PlayerAreaMap, PlayerMap};
pub use raycast::{ClipBlockShape, ClipFluid, ClipHitResult, RaytraceAction};
pub use signal_getter::{SignalGetter, SignalQueryContext};
pub(crate) use signal_getter::{
    get_best_neighbor_signal, get_control_input_signal, get_signal, is_redstone_conductor,
};
pub use tick_scheduler::ScheduledTick;
pub use weather::Precipitation;
pub(crate) use worldgen_level::WorldGenLevel;

use crate::entity::RemovalReason::Discarded;
use crate::event::{ChunkLoadEvent, Event};
use foton_registry::entity_type::MobCategory;
#[cfg(test)]
use level_effects::sound_is_within_range;
#[cfg(test)]
use portals::{
    closest_portal_candidate, nether_portal_creation_scan_origin, nether_portal_frame_offset_pos,
};
use std::path::PathBuf;
use tokio::fs;
use tokio::fs::create_dir_all;

const fn initialize_border_packet(snapshot: WorldBorderSnapshot) -> CInitializeBorder {
    CInitializeBorder {
        new_center_x: snapshot.center_x,
        new_center_z: snapshot.center_z,
        old_size: snapshot.old_size,
        new_size: snapshot.new_size,
        lerp_time: snapshot.lerp_time,
        new_absolute_max_size: snapshot.absolute_max_size,
        warning_blocks: snapshot.warning_blocks,
        warning_time: snapshot.warning_time,
    }
}

/// Loads the dragon fight of a world whose dimension type runs one.
///
/// Vanilla parity: the `if (this.dimensionType().hasEnderDragonFight())` of
/// `ServerLevel`'s constructor, which reads the saved fight and immediately
/// hands it the level, the seed and `BlockPos.ZERO`.
async fn load_dragon_fight(
    saved_data: &SavedDataManager,
    dimension_type: DimensionTypeRef,
    seed: i64,
) -> io::Result<Option<EnderDragonFight>> {
    if !dimension_type.has_ender_dragon_fight {
        return Ok(None);
    }

    let persistent: PersistentEnderDragonFight = saved_data
        .load_or_default(saved_data_names::ENDER_DRAGON_FIGHT)
        .await?;
    Ok(Some(EnderDragonFight::from_persistent(
        persistent,
        seed,
        BlockPos::ZERO,
    )))
}

/// Timing information for a world game tick.
#[derive(Debug)]
pub struct WorldGameTickTimings {
    /// Total time for this world's tick.
    pub elapsed: Duration,
    /// Chunk map game tick timings.
    pub chunk_map: ChunkMapGameTickTimings,
    /// Time spent ticking entities.
    pub entity_tick: Duration,
}

/// Result of replacing a block only when its current state still matches a prior read.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionalBlockSetResult {
    /// The expected state was claimed and replaced.
    Changed,
    /// The expected state already equals the requested state, so no callbacks ran.
    Unchanged,
    /// The current state did not match the caller's expected state.
    Stale(BlockStateId),
    /// The position is invalid, the chunk is unavailable, or the update limit was exhausted.
    Unavailable,
}

/// Configuration for creating a new world.
#[derive(Clone)]
pub struct WorldConfig {
    /// Storage configuration for chunk persistence.
    pub storage: WorldStorageConfig,
    /// Directory for level data. `None` means level data is ephemeral.
    pub level_data_path: Option<String>,
    /// World generator.
    pub generator: Arc<ChunkGeneratorType>,
    /// Generator metadata persisted for startup compatibility checks.
    pub generation_settings: WorldGenerationSettings,
    /// Server view distance (maximum chunk radius).
    pub view_distance: u8,
    /// Server simulation distance.
    pub simulation_distance: u8,
    /// Maximum queued neighbor-update tasks in one chained run; negative means unlimited.
    pub max_chained_neighbor_updates: i32,
    /// Compression settings for encoding broadcast packets.
    pub compression: Option<CompressionInfo>,
    /// Whether the world should be marked as flat in login/respawn packets.
    pub is_flat: bool,
    /// Sea level sent in login/respawn packets.
    pub sea_level: i32,
    /// Default game mode for first-visit player data.
    pub default_gamemode: GameType,
    /// Difficulty used when creating new level data.
    pub difficulty: Difficulty,
    /// Whether the world has a bonus chest.
    pub bonus_chest: bool,
}

/// A spawn limit that may or may not be set.
///
/// Lets [`World::set_spawn_limit`] take either a plain `i32` or an
/// `Option<i32>`, where `None` clears the override and restores the default.
pub trait SpawnLimitValue {
    /// The limit, or `None` to clear it.
    fn into_limit(self) -> Option<i32>;
}
impl SpawnLimitValue for i32 {
    fn into_limit(self) -> Option<i32> {
        Some(self)
    }
}
impl SpawnLimitValue for Option<i32> {
    fn into_limit(self) -> Option<i32> {
        self
    }
}

/// One loaded world: its chunks, its entities, its players and its clock.
///
/// A server holds several, keyed by [`foton_utils::Identifier`], and ticks
/// each on its own worker.
pub struct World {
    /// The chunk map of the world.
    pub chunk_map: Arc<ChunkMap>,
    /// All players in the world with dual indexing by UUID and entity ID.
    pub players: PlayerMap,
    /// Spatial index for player proximity queries.
    pub player_area_map: PlayerAreaMap,
    /// Loaded world identifier (`domain:world`).
    pub key: Identifier,
    /// Vanilla dimension type for this loaded world.
    ///
    /// Vanilla often calls loaded worlds "dimensions". In Foton, `World` is the
    /// loaded world instance and `dimension_type` is the vanilla registry entry
    /// controlling height, skylight, ceiling, water evaporation, etc.
    pub dimension_type: DimensionTypeRef,
    /// Level data manager for persistent world state.
    pub level_data: SyncRwLock<LevelDataManager>,
    /// Per-world saved data storage.
    pub(crate) saved_data: SavedDataManager,
    /// Runtime world border state.
    world_border: SyncMutex<WorldBorder>,
    /// Vanilla sleeping player counts for night-skip checks.
    sleep_status: SyncMutex<sleep_status::SleepStatus>,
    /// Server view distance (maximum chunk radius).
    pub view_distance: u8,
    /// Server simulation distance.
    pub simulation_distance: u8,
    /// Compression settings for encoding broadcast packets.
    pub compression: Option<CompressionInfo>,
    /// Whether the world should be marked as flat in login/respawn packets.
    pub is_flat: bool,
    /// Sea level sent in login/respawn packets.
    pub sea_level: i32,
    /// Default game mode for first-visit player data.
    pub default_gamemode: GameType,
    /// Whether the world has a bonus chest.
    pub bonus_chest: bool,
    /// Whether the tick rate is running normally (not frozen/paused).
    /// When false, movement validation checks are skipped.
    tick_runs_normally: AtomicBool,
    /// Whether player-versus-player damage is enabled in this world.
    pvp: AtomicBool,
    /// Whether natural monsters are allowed in this world.
    allow_monsters: AtomicBool,
    /// Whether natural animals are allowed in this world.
    allow_animals: AtomicBool,
    /// Per-world natural-spawn limits overridden by plugins.
    spawn_limits: SyncRwLock<FxHashMap<MobCategory, i32>>,
    /// Per-world natural-spawn cadence overrides in ticks.
    spawn_ticks: SyncRwLock<FxHashMap<MobCategory, i32>>,
    keep_spawn_in_memory: AtomicBool,
    auto_save: AtomicBool,
    /// Whether vanilla's scheduled/chunk/block-event tick phase is active.
    handling_tick: AtomicBool,
    /// Ordered, duplicate-suppressing server block events awaiting execution.
    block_events: SyncMutex<BlockEventQueue>,
    /// Vanilla collecting neighbor updater shared by all live block mutations.
    neighbor_updater: CollectingNeighborUpdater,
    /// Central runtime entity ownership and lookup.
    entity_manager: WorldEntityManager,
    /// Entities being evaluated by a pre-insertion spawn event.
    pending_spawn_entities: SyncMutex<FxHashMap<uuid::Uuid, SharedEntity>>,
    /// World-global ordered block-entity ticker phase.
    block_entity_tickers: block_entity_ticker::WorldBlockEntityTickers,
    /// Physical entries retained by this world's chunk-owned game-event registries.
    game_event_listener_count: Arc<GameEventListenerCount>,
    /// Entity tracker for managing which players can see which entities.
    entity_tracker: EntityTracker,
    /// Runtime IDs for pathfinder mobs currently visible to the active world.
    navigating_mobs: NavigatingMobTracker,
    /// Weather Data needed for animating starting and stopping of rain clientside
    pub weather: SyncMutex<Weather>,
    /// Per-level recent toggle history used by vanilla redstone-torch burnout.
    redstone_torch_toggles: SyncMutex<redstone::RedstoneTorchToggleTracker>,
    /// World registration and sparse head index for chunk-owned scheduled ticks.
    scheduled_ticks: tick_scheduler::WorldTickScheduler,
    /// Published block batch used by `willTickThisTick` queries during callbacks.
    scheduled_block_ticks_this_tick:
        SyncMutex<Option<Arc<tick_scheduler::ScheduledTickRunBatch<BlockRef>>>>,
    /// Published fluid batch used by `willTickThisTick` queries during callbacks.
    scheduled_fluid_ticks_this_tick:
        SyncMutex<Option<Arc<tick_scheduler::ScheduledTickRunBatch<FluidRef>>>>,
    /// Point of interest storage for efficient spatial queries of special blocks.
    pub poi_storage: SyncMutex<PointOfInterestStorage>,
    /// Village raids running in this loaded world, saved as `data/raids.toml`.
    ///
    /// Vanilla parity: the `ServerLevel.raids` saved data. Per loaded world
    /// rather than per domain, like the chunk tickets above it: a raid belongs
    /// to the dimension whose village it besieges.
    raids: Raids,
    /// The dragon fight this loaded world runs, saved as
    /// `data/ender_dragon_fight.toml`.
    ///
    /// Vanilla parity: `ServerLevel.dragonFight`, which exists only on a
    /// dimension whose type has `has_ender_dragon_fight`.
    dragon_fight: Option<EnderDragonFight>,
    /// World-change requests queued by world-local ticks for server safe-point processing.
    pending_world_changes: SyncMutex<Vec<(SharedEntity, WorldChangeRequest)>>,
    /// The level's own random source.
    ///
    /// Vanilla parity: `Level.random`, which vanilla creates unseeded. Feature code
    /// that vanilla draws from `LevelAccessor.getRandom()` reaches it through
    /// [`WorldGenLevel::with_level_random`] when placement runs in a live world.
    level_random: SyncMutex<RandomSource>,
    /// The server this world belongs to, attached once the server exists.
    ///
    /// Vanilla reaches the server through `Level.getServer()`. Foton builds its
    /// worlds before the server that owns them, so the link is filled in by
    /// [`crate::server::Server::attach_worlds`] and is absent in tests that
    /// build a world on its own.
    server: OnceLock<Weak<Server>>,
}

impl World {
    /// Fires a plugin event when this world is attached to a server.
    pub(crate) fn fire_event<E: Event>(&self, event: &mut E) {
        if let Some(server) = self.server.get().and_then(Weak::upgrade) {
            server.events.fire(event);
        }
    }
    /// Returns the persistent world directory, or `None` for RAM-only worlds.
    #[must_use]
    pub fn world_folder(&self) -> Option<PathBuf> {
        self.level_data.read().world_dir().map(Path::to_path_buf)
    }
    /// Returns chunk coordinates whose holders have reached vanilla Full status.
    #[must_use]
    pub fn loaded_chunk_positions(&self) -> Vec<ChunkPos> {
        let mut positions = Vec::new();
        self.chunk_map.chunks.iter_sync(|pos, holder| {
            if !holder.is_status_disallowed(ChunkStatus::Full)
                && holder.try_chunk(ChunkStatus::Full).is_some()
            {
                positions.push(*pos);
            }
            true
        });
        positions
    }

    /// Returns block-entity positions and states for a loaded full chunk.
    #[must_use]
    pub fn block_entity_positions_in_chunk(&self, x: i32, z: i32) -> Vec<(BlockPos, BlockStateId)> {
        let pos = ChunkPos::new(x, z);
        let mut holder = None;
        self.chunk_map.chunks.iter_sync(|chunk_pos, value| {
            if *chunk_pos == pos {
                holder = Some(Arc::clone(value));
                false
            } else {
                true
            }
        });
        let Some(holder) = holder else {
            return Vec::new();
        };
        let Some(chunk) = holder.try_chunk(ChunkStatus::Full) else {
            return Vec::new();
        };
        chunk
            .get_block_entities()
            .into_iter()
            .map(|entity| (entity.get_block_pos(), entity.get_block_state()))
            .collect()
    }

    /// Returns whether the requested chunk has an active Full-status holder.
    #[must_use]
    /// Removes a live entity as an explicit plugin discard.
    pub fn remove_entity(&self, entity_id: i32) -> bool {
        self.entity_manager()
            .remove_live_entity(entity_id, Discarded)
            .is_some()
    }

    /// Whether the chunk at these chunk coordinates is loaded.
    ///
    /// Note the arguments are chunk coordinates, not block coordinates.
    pub fn is_chunk_loaded(&self, x: i32, z: i32) -> bool {
        self.loaded_chunk_positions()
            .iter()
            .any(|pos| pos.0.x == x && pos.0.y == z)
    }

    /// Checks persisted generation state without stalling a game tick.
    ///
    /// Bukkit exposes this synchronously. Outside a tick we may query the
    /// storage runtime; during a tick the query is intentionally conservative
    /// to avoid waiting on asynchronous region I/O on the tick thread.
    pub fn is_chunk_generated(&self, x: i32, z: i32) -> bool {
        if self.is_chunk_loaded(x, z) || self.handling_tick.load(Ordering::Relaxed) {
            return self.is_chunk_loaded(x, z);
        }
        self.chunk_map
            .chunk_runtime
            .block_on(self.chunk_map.storage.chunk_exists(ChunkPos::new(x, z)))
            .unwrap_or(false)
    }
    /// Creates a new world with custom configuration.
    ///
    /// This allows specifying storage backend (disk or RAM-only) and other options.
    /// Uses `Arc::new_cyclic` to create a cyclic reference between
    /// the World and its `ChunkMap`'s `WorldGenContext`.
    ///
    /// # Arguments
    /// * `chunk_runtime` - The Tokio runtime for chunk operations
    /// * `dimension_type` - Vanilla dimension type (overworld, nether, end)
    /// * `seed` - The world seed
    /// * `config` - World configuration including storage options
    pub async fn new_with_config(
        chunk_runtime: Arc<Runtime>,
        key: Identifier,
        dimension_type: DimensionTypeRef,
        seed: i64,
        config: WorldConfig,
        generation_pool: Arc<rayon::ThreadPool>,
    ) -> io::Result<Arc<Self>> {
        let chunk_encoding_pool = Arc::clone(&generation_pool);
        Self::new_with_config_and_encoding_pool(
            chunk_runtime,
            key,
            dimension_type,
            seed,
            config,
            generation_pool,
            chunk_encoding_pool,
        )
        .await
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one straight-line assembly of every world subsystem, in order"
    )]
    pub(crate) async fn new_with_config_and_encoding_pool(
        chunk_runtime: Arc<Runtime>,
        key: Identifier,
        dimension_type: DimensionTypeRef,
        seed: i64,
        config: WorldConfig,
        generation_pool: Arc<rayon::ThreadPool>,
        chunk_encoding_pool: Arc<rayon::ThreadPool>,
    ) -> io::Result<Arc<Self>> {
        let view_distance = config.view_distance;
        let simulation_distance = config.simulation_distance;
        let max_chained_neighbor_updates = config.max_chained_neighbor_updates;
        let compression = config.compression;
        let is_flat = config.is_flat;
        let sea_level = config.sea_level;
        let default_gamemode = config.default_gamemode;
        let bonus_chest = config.bonus_chest;
        // Create storage backend based on config
        let storage: Arc<ChunkStorage> = match &config.storage {
            WorldStorageConfig::Disk { path } => {
                Arc::new(ChunkStorage::Disk(RegionManager::new(path.clone())))
            }
            WorldStorageConfig::RamOnly => {
                Arc::new(ChunkStorage::RamOnly(RamOnlyStorage::empty_world()))
            }
        };

        // Create or skip level data based on config

        let path = config.level_data_path.as_deref().map(Path::new);
        let saved_data = SavedDataManager::new(path);
        let mut level_data =
            LevelDataManager::new(path, seed, config.difficulty, config.generation_settings)
                .await?;
        if level_data.is_dirty() {
            level_data.save().await?;
        }
        let persistent_chunk_tickets: PersistentChunkTickets = saved_data
            .load_or_default(saved_data_names::CHUNK_TICKETS)
            .await?;
        let timed_chunk_tickets = TimedChunkTickets::from_persistent(persistent_chunk_tickets);
        let persistent_raids: PersistentRaids =
            saved_data.load_or_default(saved_data_names::RAIDS).await?;
        let raids = Raids::from_persistent(persistent_raids);
        let dragon_fight = load_dragon_fight(&saved_data, dimension_type, seed).await?;
        let world_border = WorldBorder::new(level_data.data().world_border)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let keep_spawn_in_memory = level_data.data().keep_spawn_in_memory;
        // let generator = Arc::new(ChunkGeneratorType::Flat(FlatChunkGenerator::new(
        //     REGISTRY
        //         .blocks
        //         .get_default_state_id(vanilla_blocks::BEDROCK), // Bedrock
        //     REGISTRY.blocks.get_default_state_id(vanilla_blocks::DIRT), // Dirt
        //     REGISTRY
        //         .blocks
        //         .get_default_state_id(vanilla_blocks::GRASS_BLOCK), // Grass Block
        // )));

        let mut weather = Weather::default();
        if level_data.is_raining() {
            weather.rain_level = 1.0;
            if level_data.is_thundering() {
                weather.thunder_level = 1.0;
            }
        }

        Ok(Arc::new_cyclic(|weak_self: &Weak<World>| {
            let chunk_map = Arc::new(ChunkMap::new_with_storage_and_timed_tickets(
                chunk_runtime,
                weak_self.clone(),
                dimension_type,
                sea_level,
                storage,
                config.generator,
                generation_pool,
                chunk_encoding_pool,
                timed_chunk_tickets,
            ));
            chunk_map.start_generation_refill_loop();

            Self {
                chunk_map,
                players: PlayerMap::new(),
                player_area_map: PlayerAreaMap::new(),
                key,
                dimension_type,
                level_data: SyncRwLock::new(level_data),
                saved_data,
                world_border: SyncMutex::new(world_border),
                sleep_status: SyncMutex::new(sleep_status::SleepStatus::default()),
                view_distance,
                simulation_distance,
                compression,
                is_flat,
                sea_level,
                default_gamemode,
                bonus_chest,
                tick_runs_normally: AtomicBool::new(true),
                pvp: AtomicBool::new(true),
                allow_monsters: AtomicBool::new(true),
                allow_animals: AtomicBool::new(true),
                spawn_limits: SyncRwLock::new(FxHashMap::default()),
                spawn_ticks: SyncRwLock::new(FxHashMap::default()),
                keep_spawn_in_memory: AtomicBool::new(keep_spawn_in_memory),
                auto_save: AtomicBool::new(true),
                handling_tick: AtomicBool::new(false),
                block_events: SyncMutex::new(BlockEventQueue::default()),
                neighbor_updater: CollectingNeighborUpdater::new(max_chained_neighbor_updates),
                entity_manager: WorldEntityManager::new(),
                pending_spawn_entities: SyncMutex::new(FxHashMap::default()),
                block_entity_tickers: block_entity_ticker::WorldBlockEntityTickers::new(),
                game_event_listener_count: GameEventListenerCount::shared(),
                entity_tracker: EntityTracker::new(),
                navigating_mobs: NavigatingMobTracker::new(),
                weather: SyncMutex::new(weather),
                redstone_torch_toggles: SyncMutex::new(
                    redstone::RedstoneTorchToggleTracker::default(),
                ),
                scheduled_ticks: tick_scheduler::WorldTickScheduler::new(),
                scheduled_block_ticks_this_tick: SyncMutex::new(None),
                scheduled_fluid_ticks_this_tick: SyncMutex::new(None),
                poi_storage: SyncMutex::new(PointOfInterestStorage::new()),
                raids,
                dragon_fight,
                pending_world_changes: SyncMutex::new(Vec::new()),
                level_random: SyncMutex::new(RandomSource::Legacy(LegacyRandom::from_seed(
                    rand::random(),
                ))),
                server: OnceLock::new(),
            }
        }))
    }

    /// Links this world to the server that owns it.
    ///
    /// Called once, from [`crate::server::Server::attach_worlds`]. A second
    /// call is ignored rather than replacing the link.
    pub(crate) fn attach_server(&self, server: &Arc<Server>) {
        let _ = self.server.set(Arc::downgrade(server));
    }

    /// Returns the server that owns this world.
    ///
    /// Vanilla parity: `Level.getServer`. `None` before the server is built and
    /// in tests that construct a world on its own, so callers on a game tick
    /// must treat a missing server as "do nothing" rather than unwrapping.
    #[must_use]
    pub fn server(&self) -> Option<Arc<Server>> {
        self.server.get().and_then(Weak::upgrade)
    }

    /// Cleans up the world by saving all chunks.
    pub async fn cleanup(&self, total_saved: &mut usize) {
        self.sync_world_border_to_level_data();
        let level_data_save = self.level_data.write().prepare_save();
        match level_data_save {
            Ok(Some((path, content))) => {
                let result = async {
                    if let Some(parent) = path.parent() {
                        create_dir_all(parent).await?;
                    }
                    fs::write(&path, content).await
                }
                .await;
                match result {
                    Ok(()) => log::info!("World {} level data saved successfully", self.key),
                    Err(error) => log::error!("Failed to save world level data: {error}"),
                }
            }
            Ok(None) => {}
            Err(error) => log::error!("Failed to prepare world level data: {error}"),
        }

        let chunk_tickets = self.chunk_map.persistent_chunk_tickets();
        match self
            .saved_data
            .save(saved_data_names::CHUNK_TICKETS, &chunk_tickets)
            .await
        {
            Ok(()) => log::info!("World {} saved chunk ticket data successfully", self.key),
            Err(e) => log::error!("Failed to save world chunk ticket data: {e}"),
        }

        let raids = self.raids.to_persistent();
        match self.saved_data.save(saved_data_names::RAIDS, &raids).await {
            Ok(()) => log::info!("World {} saved raid data successfully", self.key),
            Err(e) => log::error!("Failed to save world raid data: {e}"),
        }

        if let Some(fight) = self.dragon_fight.as_ref() {
            let fight = fight.to_persistent();
            match self
                .saved_data
                .save(saved_data_names::ENDER_DRAGON_FIGHT, &fight)
                .await
            {
                Ok(()) => log::info!("World {} saved dragon fight data successfully", self.key),
                Err(e) => log::error!("Failed to save world dragon fight data: {e}"),
            }
        }

        match self.save_all_chunks().await {
            Ok(count) => *total_saved += count,
            Err(e) => log::error!("Failed to save world chunks: {e}"),
        }
    }

    /// Returns the domain this loaded world belongs to.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.key.namespace.as_ref()
    }

    /// Returns whether this world has a bonus chest.
    #[must_use]
    pub const fn has_bonus_chest(&self) -> bool {
        self.bonus_chest
    }

    /// Returns the dragon fight this world runs, if it runs one.
    ///
    /// Vanilla parity: `ServerLevel.getDragonFight`. `None` outside the End,
    /// which is what makes a dragon summoned anywhere else fightless -- no boss
    /// bar, no crystals, no exit portal, exactly as in vanilla.
    #[must_use]
    pub const fn dragon_fight(&self) -> Option<&EnderDragonFight> {
        self.dragon_fight.as_ref()
    }

    /// Returns whether this world uses vanilla's Nether dimension type.
    ///
    /// Vanilla tests `level.dimension() == Level.NETHER` against a fixed level
    /// key. Foton's world keys are `domain:world`, so the dimension type is the
    /// only thing that still identifies a Nether.
    #[must_use]
    pub fn is_nether(&self) -> bool {
        self.dimension_type == &vanilla_dimension_types::THE_NETHER
    }

    /// Game tick: weather, time, chunk game tick (broadcasts + random/scheduled ticks),
    /// and player logic (without chunk sending).
    ///
    /// * `tick_count` - The current tick number
    /// * `runs_normally` - Whether game elements (random ticks, entities) should run.
    ///   When false (frozen), only essential operations like chunk loading run.
    #[tracing::instrument(level = "trace", skip(self), name = "world_game_tick")]
    #[expect(
        clippy::too_many_lines,
        reason = "world tick orchestration keeps vanilla subsystem order explicit"
    )]
    pub fn tick_game(
        self: &Arc<Self>,
        tick_count: u64,
        runs_normally: bool,
    ) -> WorldGameTickTimings {
        let world_start = Instant::now();
        let lookup_cache_scope = GameplayChunkLookupCacheScope::enter(&self.chunk_map);
        self.handling_tick.store(true, Ordering::Relaxed);
        self.set_tick_runs_normally(runs_normally);
        if runs_normally {
            self.tick_world_border();
            self.tick_weather();
        }
        self.tick_sleeping_players();
        if runs_normally {
            self.tick_time();
            self.tick_natural_spawn(tick_count);
            // Vanilla parity: the `profiler.popPush("raid")` step of
            // `ServerLevel.tick`, which runs after the world's own clocks and
            // before block events and entities.
            self.raids.tick(self);
        }

        let random_tick_speed = self.get_game_rule(&RANDOM_TICK_SPEED) as u32;

        let loaded_before = self.loaded_chunk_positions();
        let mut chunk_map_timings =
            self.chunk_map
                .tick_game(self, tick_count, random_tick_speed, runs_normally);
        let loaded_before = loaded_before
            .into_iter()
            .map(|pos| (pos.0.x, pos.0.y))
            .collect::<rustc_hash::FxHashSet<_>>();
        for pos in self.loaded_chunk_positions() {
            if loaded_before.contains(&(pos.0.x, pos.0.y)) {
                continue;
            }
            self.fire_event(&mut ChunkLoadEvent::new(self.key.to_string(), pos, false));
        }

        if runs_normally {
            let _span = tracing::trace_span!("block_events").entered();
            self.run_block_events();
        }

        // Vanilla clears this before ticking entities and block entities.
        self.handling_tick.store(false, Ordering::Relaxed);

        // Vanilla parity: the `profiler.push("dragonFight")` step of
        // `ServerLevel.tick`, which opens the entity phase -- the fight decides
        // whether a dragon exists before the dragon gets its tick.
        if runs_normally && let Some(fight) = self.dragon_fight.as_ref() {
            let _span = tracing::trace_span!("dragon_fight").entered();
            fight.tick(self);
        }

        let entity_tick = {
            let _span = tracing::trace_span!("entity_tick").entered();
            let start = Instant::now();
            let dirty_chunks = self
                .entity_manager
                .tick_entities(tick_count as i32, runs_normally);
            for chunk in dirty_chunks {
                self.mark_chunk_dirty(chunk);
            }
            start.elapsed()
        };

        {
            let _span = tracing::trace_span!("block_entities").entered();
            let start = Instant::now();
            self.block_entity_tickers.tick(self, runs_normally);
            chunk_map_timings.tick_block_entities = start.elapsed();
        }

        {
            let _span = tracing::trace_span!("entity_tracker_send_changes").entered();
            self.entity_tracker.send_changes(
                |chunk| self.get_packet_tracking_players(chunk),
                |player_id| self.players.get_by_entity_id(player_id),
                EntityChangeSenders {
                    movement: |entity_id, packet| {
                        self.broadcast_movement_sync_to_entity_trackers(entity_id, packet, None);
                    },
                    self_movement: |player_id, packet| {
                        let Some(encoded) = self.encode_movement_sync_packet(packet) else {
                            return;
                        };
                        let Some(player) = self.players.get_by_entity_id(player_id) else {
                            return;
                        };
                        player.connection.send_encoded(encoded);
                    },
                    entity_data: |entity_id, dirty_entity_data| {
                        let packet = CSetEntityData::new(entity_id, dirty_entity_data);
                        let Ok(encoded) = EncodedPacket::from_bare(
                            packet,
                            self.compression,
                            ConnectionProtocol::Play,
                        ) else {
                            return;
                        };
                        self.broadcast_to_entity_trackers_encoded(entity_id, encoded.clone(), None);
                        if let Some(player) = self.players.get_by_entity_id(entity_id) {
                            player.connection.send_encoded(encoded);
                        }
                    },
                    attributes: |entity_id, dirty_attributes| {
                        let packet = CUpdateAttributes::new(entity_id, dirty_attributes);
                        let Ok(encoded) = EncodedPacket::from_bare(
                            packet,
                            self.compression,
                            ConnectionProtocol::Play,
                        ) else {
                            return;
                        };
                        self.broadcast_to_entity_trackers_encoded(entity_id, encoded.clone(), None);
                        if let Some(player) = self.players.get_by_entity_id(entity_id) {
                            player.connection.send_encoded(encoded);
                        }
                    },
                    mob_effects: |player_id, packet| {
                        let Some(player) = self.players.get_by_entity_id(player_id) else {
                            return;
                        };
                        match packet {
                            MobEffectSyncPacket::Update(packet) => player.send_packet(packet),
                            MobEffectSyncPacket::Remove(packet) => player.send_packet(packet),
                        }
                    },
                    equipment: |entity_id, packet: CSetEquipment| {
                        let Ok(encoded) = EncodedPacket::from_bare(
                            packet,
                            self.compression,
                            ConnectionProtocol::Play,
                        ) else {
                            return;
                        };
                        self.broadcast_to_entity_trackers_encoded(entity_id, encoded, None);
                    },
                    passengers: |player_id, packet| {
                        if let Some(player) = self.players.get_by_entity_id(player_id) {
                            player.send_packet(packet);
                        }
                    },
                    entity_link: |entity_id, packet: CSetEntityLink| {
                        let Ok(encoded) = EncodedPacket::from_bare(
                            packet,
                            self.compression,
                            ConnectionProtocol::Play,
                        ) else {
                            return;
                        };
                        self.broadcast_to_entity_trackers_encoded(entity_id, encoded, None);
                    },
                },
            );
        }

        chunk_map_timings.lookup_cache = lookup_cache_scope.finish();
        WorldGameTickTimings {
            elapsed: world_start.elapsed(),
            chunk_map: chunk_map_timings,
            entity_tick,
        }
    }

    /// Returns whether this world participates in automatic saves.
    #[must_use]
    pub fn keep_spawn_in_memory(&self) -> bool {
        self.keep_spawn_in_memory.load(Ordering::Acquire)
    }
    /// Sets whether the spawn chunks stay loaded with no player nearby.
    ///
    /// Placing or removing the spawn ticket takes effect immediately, so
    /// turning this off frees the chunks without waiting for a save.
    pub fn set_keep_spawn_in_memory(&self, value: bool) {
        let old = self.keep_spawn_in_memory.swap(value, Ordering::AcqRel);
        if old == value {
            return;
        }
        let spawn = {
            let mut level_data = self.level_data.write();
            level_data.data_mut().keep_spawn_in_memory = value;
            level_data.data().spawn_pos()
        };
        if value {
            self.chunk_map.place_spawn_ticket(spawn);
        } else {
            self.chunk_map.remove_spawn_ticket(spawn);
        }
    }

    /// This world's override of how many mobs of `category` may exist, if one
    /// was set.
    ///
    /// `None` means the category's own default applies.
    pub fn spawn_limit(&self, category: MobCategory) -> Option<i32> {
        self.spawn_limits.read().get(&category).copied()
    }

    /// Overrides how many mobs of `category` this world may hold, or clears
    /// the override with `None`. Negative limits are clamped to zero.
    pub fn set_spawn_limit<L: SpawnLimitValue>(&self, category: MobCategory, limit: L) {
        let mut limits = self.spawn_limits.write();
        if let Some(limit) = limit.into_limit() {
            limits.insert(category, limit.max(0));
        } else {
            limits.remove(&category);
        }
    }

    /// This world's override of how often `category` is considered for
    /// spawning, in ticks.
    pub fn spawn_ticks(&self, category: MobCategory) -> Option<i32> {
        self.spawn_ticks.read().get(&category).copied()
    }

    /// Sets how often `category` is considered for spawning, in ticks.
    /// Negative values are clamped to zero, meaning every tick.
    pub fn set_spawn_ticks(&self, category: MobCategory, ticks: i32) {
        self.spawn_ticks.write().insert(category, ticks.max(0));
    }

    /// Whether this world is saved automatically. See [`Self::set_auto_save`].
    pub fn is_auto_save(&self) -> bool {
        self.auto_save.load(Ordering::Acquire)
    }

    /// Enables or disables automatic saves for this world.
    pub fn set_auto_save(&self, value: bool) {
        self.auto_save.store(value, Ordering::Release);
    }

    /// Queues a non-blocking save of level data and dirty chunks.
    pub fn request_save(self: &Arc<Self>) {
        let prepared = self.level_data.write().prepare_save();
        let world = Arc::clone(self);
        self.chunk_map.chunk_runtime.handle().spawn(async move {
            match prepared {
                Ok(Some((path, content))) => {
                    if let Some(parent) = path.parent()
                        && let Err(error) = create_dir_all(parent).await
                    {
                        tracing::error!(%error, "World level-data directory save failed");
                        return;
                    }
                    if let Err(error) = fs::write(&path, content).await {
                        tracing::error!(%error, "World level-data save failed");
                    }
                }
                Ok(None) => {}
                Err(error) => tracing::error!(%error, "World level-data serialization failed"),
            }
            if let Err(error) = world.save_all_chunks().await {
                tracing::error!(%error, "World chunk save failed");
            }
        });
    }

    /// Saves all dirty chunks in this world to disk.
    ///
    /// This should be called during graceful shutdown.
    /// Returns the number of chunks saved.
    pub async fn save_all_chunks(&self) -> io::Result<usize> {
        self.chunk_map.save_all_chunks().await
    }
}

impl LevelReader for World {
    fn get_block_state(&self, pos: BlockPos) -> BlockStateId {
        Self::get_block_state(self, pos)
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<SharedBlockEntity> {
        Self::get_block_entity(self, pos)
    }

    fn is_face_sturdy_for(
        &self,
        state: BlockStateId,
        pos: BlockPos,
        direction: Direction,
        support_type: SupportType,
    ) -> bool {
        BLOCK_BEHAVIORS
            .get_behavior(state.get_block())
            .is_face_sturdy(state, self, pos, direction, support_type)
    }

    fn raw_brightness(&self, pos: BlockPos, sky_darkening: u8) -> u8 {
        let sky_light = if self.dimension_type.has_skylight {
            self.light_value_at(LightLayer::Sky, pos)
                .saturating_sub(sky_darkening)
        } else {
            0
        };

        if sky_light == MAX_LIGHT_LEVEL {
            return MAX_LIGHT_LEVEL;
        }

        sky_light.max(self.light_value_at(LightLayer::Block, pos))
    }

    fn can_see_sky(&self, pos: BlockPos) -> bool {
        Self::can_see_sky(self, pos)
    }

    fn ambient_light(&self) -> f32 {
        self.dimension_type.ambient_light
    }

    fn min_y(&self) -> i32 {
        self.get_min_y()
    }

    fn height(&self) -> i32 {
        self.get_height()
    }
}

impl LevelReader for Arc<World> {
    fn get_block_state(&self, pos: BlockPos) -> BlockStateId {
        self.as_ref().get_block_state(pos)
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<SharedBlockEntity> {
        self.as_ref().get_block_entity(pos)
    }

    fn is_face_sturdy_for(
        &self,
        state: BlockStateId,
        pos: BlockPos,
        direction: Direction,
        support_type: SupportType,
    ) -> bool {
        self.as_ref()
            .is_face_sturdy_for(state, pos, direction, support_type)
    }

    fn raw_brightness(&self, pos: BlockPos, sky_darkening: u8) -> u8 {
        self.as_ref().raw_brightness(pos, sky_darkening)
    }

    fn can_see_sky(&self, pos: BlockPos) -> bool {
        self.as_ref().can_see_sky(pos)
    }

    fn ambient_light(&self) -> f32 {
        self.as_ref().ambient_light()
    }

    fn min_y(&self) -> i32 {
        self.as_ref().get_min_y()
    }

    fn height(&self) -> i32 {
        self.as_ref().get_height()
    }
}

impl ScheduledTickAccess for Arc<World> {
    fn fluid_tick_delay(&self, fluid: FluidRef) -> i32 {
        FLUID_BEHAVIORS.get_behavior(fluid).tick_delay(self)
    }

    fn schedule_block_tick_default(&self, pos: BlockPos, block: BlockRef, delay: i32) -> bool {
        self.as_ref().schedule_block_tick_default(pos, block, delay);
        true
    }

    fn has_scheduled_block_tick(&self, pos: BlockPos, block: BlockRef) -> bool {
        self.as_ref().has_scheduled_block_tick(pos, block)
    }

    fn will_tick_block_this_tick(&self, pos: BlockPos, block: BlockRef) -> bool {
        self.as_ref().will_tick_block_this_tick(pos, block)
    }

    fn schedule_fluid_tick_default(&self, pos: BlockPos, fluid: FluidRef, delay: i32) -> bool {
        self.as_ref().schedule_fluid_tick_default(pos, fluid, delay);
        true
    }

    fn will_tick_fluid_this_tick(&self, pos: BlockPos, fluid: FluidRef) -> bool {
        self.as_ref().will_tick_fluid_this_tick(pos, fluid)
    }
}

impl LevelAccessor for Arc<World> {
    fn set_block_state(&self, pos: BlockPos, state: BlockStateId, flags: UpdateFlags) -> bool {
        self.set_block(pos, state, flags)
    }

    fn play_block_sound(
        &self,
        sound: SoundEventRef,
        pos: BlockPos,
        volume: f32,
        pitch: f32,
        exclude: Option<i32>,
    ) {
        self.as_ref()
            .play_block_sound(sound, pos, volume, pitch, exclude);
    }

    fn game_event(&self, event: GameEventRef, pos: BlockPos, context: &GameEventContext<'_>) {
        World::game_event(self, event, pos, context);
    }

    fn level_event(&self, event_type: i32, pos: BlockPos, data: i32, exclude: Option<i32>) {
        World::level_event(self, event_type, pos, data, exclude);
    }

    fn heightmap_at(&self, heightmap_type: HeightmapType, x: i32, z: i32) -> i32 {
        // A finished chunk keeps only the final heightmaps. Vanilla primes a missing
        // one on demand from the blocks that are there, which for the worldgen pair
        // is the same answer their final counterparts already hold, so ask those.
        // `WorldGenRegion` resolves an already-full dependency the same way.
        let heightmap_type = match heightmap_type {
            HeightmapType::WorldSurfaceWg => HeightmapType::WorldSurface,
            HeightmapType::OceanFloorWg => HeightmapType::OceanFloor,
            other => other,
        };
        World::height_at(self, heightmap_type, x, z).unwrap_or_else(|| self.get_min_y())
    }
}
