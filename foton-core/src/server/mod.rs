//! This module contains the `Server` struct, which is the main entry point for the server.
mod broadcasting;
/// Tick-polled server jobs.
pub mod jobs;
mod packet_processor;
mod pregen;
/// The registry cache for the server.
pub mod registry_cache;
mod run_loop;
mod service_keys;
/// The tick rate manager for the server.
pub mod tick_rate_manager;
mod world_tick_workers;
/// Domain-aware loaded world map.
pub mod worlds;

use crate::bootstrap::init_globals;
use crate::boss_event::custom::DomainCustomBossEvents;
use crate::chunk::{
    chunk_request::{ChunkRequest, ChunkRequestHandle, ChunkRequestState, ChunkTicketKind},
    status::ChunkStatus,
};
use crate::command::brigadier::{StringReader, SuggestionError, Suggestions};
use crate::command::execution::{
    CommandExecutionContext, CommandResultCallback, CommandSource, ExecutionCommandSource,
    ExecutionStop,
};
use crate::command::functions::{FunctionManager, FunctionReloadReport};
use crate::command::rcon::RconOutput;
use crate::command::sender::{CommandExecutionOwner, CommandSender};
use crate::command::storage::DomainCommandStorage;
use crate::command::{
    COMMAND_REQUESTS_PER_TICK, COMMAND_RESUMPTIONS_PER_TICK, CommandCompletion, CommandDispatcher,
    CommandQueueFull, CommandRegistry, CommandRequest, CommandRequestQueue,
    PendingCommandExecutionQueue, client_permission_event, command_suggestions_packet,
    command_tree_packet, create_registered_dispatcher,
};
use crate::config::{
    ResolvedWorldConfig, RuntimeConfig, StorageSelection, WorldsConfig, validate_login_security,
};
use crate::entity::{
    Entity, EntityBase, PendingWorldChangeToken, RemovalReason, SharedEntity, change_entity_world,
};

use crate::chunk_saver::{ChunkStorage, PersistentEntity, registry::WorldStorageRegistry};
use crate::event::EventBus;
use crate::level_data::{LevelDataManager, RespawnData, WorldGenerationSettings};
use crate::map::DomainMapData;
use crate::permission::{
    COMMAND_BLOCK_GROUP, OP_GROUP, PermissionGroupManager, PermissionGroupManagerError,
    PermissionGroupUpdateError, PermissionGroupsConfig, PermissionMetadataExpression,
    PermissionRuleExpression, PermissionSet, PermissionSubjectIndex, PermissionSubjectState,
};
use crate::player::chunk_sender::{ChunkSender, EncodedChunk};
use crate::player::connection::NetworkConnection;
use crate::player::connection::ScheduledPlayPacket;
use crate::player::player_data::{
    PersistentEnderPearl, PersistentPlayerData, PersistentRootVehicle,
};
use crate::player::player_data_storage::{GlobalPlayerData, PlayerDataStorage};
use crate::player::player_inventory::MenuRemovalStatus;
use crate::player::{
    DomainResidenceToken, GameProfile, KnownPlayer, KnownPlayerNameLookup, KnownPlayers, Player,
    ProfileLookupError, ResetReason, is_valid_player_name, lookup_online_profile, offline_uuid,
};
use crate::portal::{
    PortalKind, TeleportPostTransition, TeleportTransition, WorldChangeRequest, end_gateway,
    end_portal, nether_portal,
};
use crate::scoreboard::DomainScoreboards;
use crate::server::jobs::{FnServerJob, ServerJobContext, ServerJobQueue};
use crate::server::packet_processor::PacketProcessor;
use crate::server::registry_cache::RegistryCache;
use crate::server::service_keys::ServiceKeyStore;
use crate::server::worlds::WorldMap;
use crate::world::player_spawn_finder::{PlayerSpawnSearch, PlayerSpawnSearchPoll};
use crate::world::{PlayerMap, World, WorldConfig};
use crate::worldgen::WorldGeneratorRegistry;
use crate::worldgen::registry::GeneratorOutput;
use crossbeam::queue::SegQueue;
use foton_crypto::{key_store::KeyStore, signature::ProfileKeyValidator};
use foton_protocol::packet_traits::{ClientPacket, EncodedPacket};
use foton_protocol::packets::game::{
    CCommandSuggestions, CEntityEvent, CLogin, CPlayerInfoUpdate, CRemovePlayerInfo,
    CSetDefaultSpawnPosition, CSystemChat, CTabList, CTickingState, CTickingStep,
    CommonPlayerSpawnInfo, RelativeMovement,
};
use foton_protocol::utils::ConnectionProtocol;
use foton_registry::vanilla_game_rules::{
    ALLOW_ENTERING_NETHER_USING_PORTALS, IMMEDIATE_RESPAWN, LIMITED_CRAFTING, REDUCED_DEBUG_INFO,
};
use foton_registry::{
    RegistryEntry, dimension_type::DimensionTypeRef, vanilla_dimension_types, vanilla_entities,
};
use foton_utils::{
    BlockPos, ChunkPos, Identifier,
    locks::{AsyncMutex, SyncMutex, SyncRwLock},
    text::DisplayResolutor,
    translations,
    types::GameType,
};
use glam::DVec3;
use rayon::{ThreadPool, ThreadPoolBuilder};
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering, Ordering as AtomicOrdering};
use std::{
    collections::BTreeSet,
    io, mem,
    num::NonZero,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use text_components::{Modifier, TextComponent, format::Color};
use tick_rate_manager::{SprintReport, TickRateManager};
use tokio::{
    runtime::Runtime,
    sync::{Notify, oneshot},
    task::{JoinSet, spawn_blocking},
    time::sleep,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Interval in ticks between tab list updates (20 ticks = 1 second).
const TAB_LIST_UPDATE_INTERVAL: u64 = 20;
/// Interval in ticks between player info broadcasts (600 ticks = 30 seconds).
/// Matches vanilla `PlayerList.SEND_PLAYER_INFO_INTERVAL`.
const SEND_PLAYER_INFO_INTERVAL: u64 = 600;
/// Wall-clock interval between saves of command-owned persistent server data.
/// Matches vanilla's intended five-minute autosave cadence.
const COMMAND_DATA_AUTOSAVE_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Clone, Copy)]
struct TabListTickStats {
    tps: f32,
    recent_mspt: f32,
    average_mspt: f32,
    p95_mspt: f32,
}

impl TabListTickStats {
    fn capture(tick_manager: &TickRateManager) -> Self {
        Self {
            tps: tick_manager.get_tps(),
            recent_mspt: tick_manager.get_smoothed_mspt(),
            average_mspt: tick_manager.get_average_mspt(),
            p95_mspt: tick_manager.get_p95(),
        }
    }
}

/// Results from saving every command-owned persistent data set.
pub struct CommandDataSaveResults {
    /// Number of dirty domain scoreboards written, or the save error.
    pub scoreboards: io::Result<usize>,
    /// Number of dirty domain command-storage values written, or the save error.
    pub storage: io::Result<usize>,
    /// Number of dirty domain map stores written, or the save error.
    pub maps: io::Result<usize>,
    /// Number of dirty domain boss-bar sets written, or the save error.
    pub boss_bars: io::Result<usize>,
}

mod known_players;

use foton_utils::types::Difficulty::Normal;
use known_players::KnownPlayerCacheState;
use tokio::sync::oneshot::error::TryRecvError::{Closed, Empty};
use toml::map::Map;

/// Tick rate for the chunk sending loop.
const CHUNK_SENDING_TPS: u64 = 20;

/// Work duration at which background chunk work is considered slow.
const SLOW_CHUNK_TICK_THRESHOLD: Duration = Duration::from_millis(50);

fn configured_chunk_generation_threads(configured_threads: Option<usize>) -> Option<usize> {
    cap_positive_thread_count(configured_threads, available_worker_threads())
}

fn configured_chunk_encoding_threads(configured_threads: Option<usize>) -> Option<usize> {
    cap_positive_thread_count(configured_threads, available_worker_threads())
}

fn configured_packet_workers(configured_workers: Option<usize>) -> usize {
    packet_workers_for_available(configured_workers, available_worker_threads())
}

fn available_worker_threads() -> usize {
    thread::available_parallelism().map_or(4, NonZero::get)
}

fn cap_positive_thread_count(
    configured_threads: Option<usize>,
    available_threads: usize,
) -> Option<usize> {
    let configured_threads = configured_threads.filter(|&threads| threads > 0)?;
    Some(configured_threads.min(available_threads.max(1)))
}

fn packet_workers_for_available(
    configured_workers: Option<usize>,
    available_threads: usize,
) -> usize {
    let available_threads = available_threads.max(1);
    if let Some(configured_workers) = configured_workers.filter(|&workers| workers > 0) {
        return configured_workers.min(available_threads);
    }

    ((available_threads / 2).max(2)).min(available_threads)
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
struct PreparedSpawn {
    position: DVec3,
    rotation: (f32, f32),
}

fn apply_default_spawn(player: &Arc<Player>, world: &Arc<World>, spawn: PreparedSpawn) {
    player.base().set_position_local(spawn.position);
    player.set_rotation(spawn.rotation);
    player.restore_game_modes(world.default_gamemode, None);
    player
        .abilities
        .lock()
        .update_for_game_mode(world.default_gamemode);
}

fn is_allowed_to_enter_portal(source_world: &World, target_world: &World) -> bool {
    is_allowed_to_enter_portal_target(
        is_nether_dimension_type(target_world),
        source_world.get_game_rule(&ALLOW_ENTERING_NETHER_USING_PORTALS),
    )
}

const fn is_allowed_to_enter_portal_target(
    target_is_nether: bool,
    allow_entering_nether_using_portals: bool,
) -> bool {
    if !target_is_nether {
        return true;
    }

    allow_entering_nether_using_portals
}

fn can_teleport_between_worlds(
    entity: &dyn Entity,
    source_world: &World,
    target_world: &World,
    projectile_owner_seen_credits: impl Fn(&uuid::Uuid) -> Option<bool>,
) -> bool {
    if is_end_return_transition(source_world.dimension_type, target_world.dimension_type) {
        return can_entity_return_from_end_to_overworld(entity, projectile_owner_seen_credits);
    }

    true
}

fn is_end_return_transition(
    source_dimension_type: DimensionTypeRef,
    target_dimension_type: DimensionTypeRef,
) -> bool {
    source_dimension_type == &vanilla_dimension_types::THE_END
        && target_dimension_type == &vanilla_dimension_types::OVERWORLD
}

fn is_nether_dimension_type(world: &World) -> bool {
    world.dimension_type == &vanilla_dimension_types::THE_NETHER
}

fn is_end_dimension_type(world: &World) -> bool {
    world.dimension_type == &vanilla_dimension_types::THE_END
}

fn can_entity_return_from_end_to_overworld(
    entity: &dyn Entity,
    projectile_owner_seen_credits: impl Fn(&uuid::Uuid) -> Option<bool>,
) -> bool {
    if entity.entity_type() == &vanilla_entities::ENDER_PEARL
        && entity
            .projectile_owner_uuid()
            .and_then(|uuid| projectile_owner_seen_credits(&uuid))
            == Some(false)
    {
        return false;
    }

    direct_passengers_allow_end_return(entity)
}

fn direct_passengers_allow_end_return(entity: &dyn Entity) -> bool {
    for passenger in entity.passengers() {
        if passenger
            .as_player()
            .is_some_and(|player| !player.has_seen_credits())
        {
            return false;
        }
    }

    true
}

fn local_respawn_data_for_world(world: &World) -> RespawnData {
    let level_data = world.level_data.read();
    let data = level_data.data();
    RespawnData::of(world.key.clone(), data.spawn_pos(), data.spawn.angle, 0.0)
}

fn generation_settings_for_world(
    world_entry: &ResolvedWorldConfig,
    generator_output: &GeneratorOutput,
) -> WorldGenerationSettings {
    WorldGenerationSettings::from_generator_config(
        world_entry.generator_config.generator().clone(),
        &generator_output.config,
        generator_output.dimension_type.key.clone(),
        generator_output.dimension_type.min_y,
        generator_output.dimension_type.height,
    )
}

fn world_config_registries() -> Result<(WorldGeneratorRegistry, WorldStorageRegistry), String> {
    let generator_registry = WorldGeneratorRegistry::new_with_builtins()
        .map_err(|e| format!("failed to initialize world generator registry: {e}"))?;
    let storage_registry = WorldStorageRegistry::new_with_builtins()
        .map_err(|e| format!("failed to initialize world storage registry: {e}"))?;
    Ok((generator_registry, storage_registry))
}

struct DomainPlayerState {
    world: Arc<World>,
    data: DomainPlayerData,
    spawn_chunk_request: ChunkRequestHandle,
}

struct UnpreparedDomainPlayerState {
    world: Arc<World>,
    explicit_target: bool,
    data: UnpreparedDomainPlayerData,
}

enum UnpreparedDomainPlayerData {
    SavedRestored { data: Box<PersistentPlayerData> },
    SavedWithoutLocation { data: Box<PersistentPlayerData> },
    FirstVisit,
}

enum DomainPlayerData {
    SavedRestored {
        data: Box<PersistentPlayerData>,
    },
    SavedWithoutLocation {
        data: Box<PersistentPlayerData>,
        spawn: PreparedSpawn,
    },
    FirstVisit {
        spawn: PreparedSpawn,
    },
}

struct DomainSwitchRequest {
    player: Arc<Player>,
    target_domain: String,
    target_world: Option<Arc<World>>,
    pending_token: PendingWorldChangeToken,
}

/// Failure while atomically editing one player's persisted permission state.
#[derive(Debug, thiserror::Error)]
pub enum PlayerPermissionUpdateError<E> {
    /// The caller rejected the proposed edit.
    #[error("{0}")]
    Edit(E),
    /// The edit assigns a group that is not configured.
    #[error("unknown permission group '{0}'")]
    UnknownGroup(String),
    /// The permission snapshot could not be persisted.
    #[error("failed to update player permissions: {0}")]
    Storage(io::Error),
}

impl<E> From<io::Error> for PlayerPermissionUpdateError<E> {
    fn from(value: io::Error) -> Self {
        Self::Storage(value)
    }
}

mod permissions;

#[cfg(test)]
use permissions::validate_player_permission_group_update;

mod player_admission;
mod player_lifecycle;

use player_admission::{PlayerAdmissionState, PlayerDisconnectQueue, PlayerJoinQueue};

mod world_changes;

use jobs::domain_switch::DomainSwitchJob;
use jobs::teleport::{
    EndGatewayTeleportJob, EndPortalTeleportJob, EnderPearlRestoreJob, NetherPortalTeleportJob,
    RootVehicleRestoreJob, WorldSpawnTeleportJob, clear_pending_world_change,
    portal_entity_still_valid,
};

/// The main server struct.
pub struct Server {
    /// Runtime configuration (view distance, compression, etc.).
    pub config: Arc<RuntimeConfig>,
    /// Runtime used by world loading and chunk tasks.
    chunk_runtime: Arc<Runtime>,
    /// Runtime permission groups and their persistence boundary.
    pub permission_groups: PermissionGroupManager,
    /// The cancellation token for graceful shutdown.
    pub cancel_token: CancellationToken,
    /// The key store for the server.
    pub key_store: KeyStore,
    /// The registry cache for the server.
    pub registry_cache: RegistryCache,
    /// A list of all the worlds on the server.
    pub worlds: WorldMap,
    /// Root directory used by dynamically constructed world storage.
    world_save_path: PathBuf,
    /// Players currently connected to the server, independent of world membership.
    online_players: PlayerMap,
    // Read by the plugin host, which lives outside this crate: a plugin asking
    // who is online is the single most common thing a plugin does.
    /// UUIDs reserved by a join or disconnect/save lifecycle transition.
    player_admissions: SyncMutex<FxHashMap<Uuid, PlayerAdmissionState>>,
    /// The tick rate manager for the server.
    pub tick_rate_manager: SyncRwLock<TickRateManager>,
    /// Command scoreboards isolated by Foton domain.
    pub scoreboards: DomainScoreboards,
    /// Command NBT storage isolated by Foton domain.
    pub(crate) command_storage: DomainCommandStorage,
    /// Filled maps isolated by Foton domain.
    ///
    /// Vanilla keeps one map store per server; Foton keeps one per domain, so
    /// a map carried between a domain's worlds stays readable while two
    /// domains never share an id.
    pub map_data: DomainMapData,
    /// Named boss bars isolated by Foton domain, beside the scoreboards and
    /// the command storage `execute store` addresses the same way.
    pub boss_bars: DomainCustomBossEvents,
    /// Saves and dispatches commands to appropriate handlers.
    command_dispatcher: SyncRwLock<CommandDispatcher>,
    /// Datapack-loaded command functions, shared by every domain like vanilla's.
    pub(crate) functions: FunctionManager,
    /// Foton-owned permission keys exposed for command autocomplete.
    command_permission_keys: Vec<String>,
    /// Command work submitted from connection and console tasks.
    command_requests: CommandRequestQueue,
    /// Decoded serverbound play packets handled during the inter-tick phase.
    packet_processor: PacketProcessor,
    /// Registry of world generator factories retained for dynamic world loading.
    world_generator_registry: WorldGeneratorRegistry,
    /// Registry of world storage backends retained for dynamic world loading.
    world_storage_registry: WorldStorageRegistry,
    /// Dedicated pool for CPU-heavy chunk generation.
    generation_pool: Arc<ThreadPool>,
    /// Dedicated worker pool for CPU-heavy chunk persistence and packet encoding.
    chunk_encoding_pool: Arc<ThreadPool>,
    /// Jobs resumed from a known point in the server game tick.
    pub jobs: ServerJobQueue,
    /// Player data storage for saving/loading player state.
    pub player_data_storage: PlayerDataStorage,
    /// In-memory snapshot of server-wide player timestamps for synchronous plugin lookups.
    global_player_data: SyncRwLock<FxHashMap<Uuid, GlobalPlayerData>>,
    /// Persisted permission state indexed by player UUID.
    player_permission_states: SyncRwLock<PermissionSubjectIndex>,
    /// Serializes persistence and cache publication for player permission edits.
    player_permission_updates: AsyncMutex<()>,
    /// Player identities and coalesced persistence state.
    known_players: SyncMutex<KnownPlayerCacheState>,
    /// Wakes shutdown when the single known-player save worker becomes idle.
    known_player_save_idle: Notify,
    /// HTTP client used by online-mode name-to-profile lookups.
    profile_lookup_client: reqwest::Client,
    /// Cached Mojang service keys used to validate player-key certificates.
    service_keys: Arc<ServiceKeyStore>,
    /// Player joins prepared by async I/O and finalized at the game tick safe point.
    pending_player_joins: PlayerJoinQueue,
    /// Disconnected players waiting to be detached at the next game tick safe point.
    pending_player_disconnects: PlayerDisconnectQueue,
    /// Queued world changes to process after the tick.
    pub pending_world_changes: SyncMutex<Vec<(SharedEntity, WorldChangeRequest)>>,
    /// World removals requested by plugins, applied at the tick safe-point.
    pub(crate) pending_world_removals: SyncMutex<Vec<WorldRemovalRequest>>,
    /// Worlds ready to attach at the next tick safe-point.
    pending_world_additions:
        SyncMutex<Vec<(Arc<World>, tokio::sync::oneshot::Sender<Result<(), String>>)>>,
    /// Who is listening for what.
    ///
    /// Unlike the block and item registries this is not frozen after startup:
    /// it holds subscriptions rather than game data, and something being
    /// enabled or disabled while the server runs is ordinary.
    pub events: EventBus,
    /// Queued domain switches to process after world ticks.
    pending_domain_switches: SyncMutex<Vec<DomainSwitchRequest>>,
}

/// A world removal requested for the next game-tick safe point.
static NEXT_WORLD_CREATION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, PartialEq, Eq)]
pub enum WorldCreationState {
    Pending,
    Ready,
    Failed(String),
}

/// Handle for a world creation request. Poll this from a safe-point; never block the game tick.
pub struct WorldCreationRequest {
    id: u64,
    receiver: tokio::sync::oneshot::Receiver<Result<(), String>>,
}

impl WorldCreationRequest {
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Polls the request without waiting for asynchronous world construction.
    pub fn poll(&mut self) -> WorldCreationState {
        match self.receiver.try_recv() {
            Ok(Ok(())) => WorldCreationState::Ready,
            Ok(Err(error)) => WorldCreationState::Failed(error),
            Err(Empty) => WorldCreationState::Pending,
            Err(Closed) => WorldCreationState::Failed("world creation task closed".to_owned()),
        }
    }

    /// Awaits completion for callers already outside the game tick.
    pub async fn wait(self) -> WorldCreationState {
        match self.receiver.await {
            Ok(Ok(())) => WorldCreationState::Ready,
            Ok(Err(error)) => WorldCreationState::Failed(error),
            Err(_) => WorldCreationState::Failed("world creation task closed".to_owned()),
        }
    }
}

pub(crate) struct WorldRemovalRequest {
    pub(crate) key: Identifier,
    pub(crate) save: bool,
    pub(crate) completion: Option<oneshot::Sender<Result<usize, String>>>,
}

struct GameTickTaskGuard {
    server: Arc<Server>,
    cancel_token: CancellationToken,
}

impl GameTickTaskGuard {
    const fn new(server: Arc<Server>, cancel_token: CancellationToken) -> Self {
        Self {
            server,
            cancel_token,
        }
    }
}

impl Drop for GameTickTaskGuard {
    fn drop(&mut self) {
        self.server.packet_processor.stop();
        self.cancel_token.cancel();
    }
}

impl Server {
    /// Schedules creation of a plugin-requested world without blocking the game
    /// tick. Completion is reported only after safe-point attachment.
    pub fn request_world_creation(
        self: &Arc<Self>,
        name: String,
        generator: Identifier,
        seed: i64,
        bonus_chest: bool,
    ) -> Result<WorldCreationRequest, String> {
        if name.is_empty() || !Identifier::validate_path(&name) {
            return Err(format!("invalid world name {name}"));
        }
        let domain = self.worlds.default_domain().to_owned();
        let key = Identifier::new(domain.clone(), name.clone());
        if self.worlds.get(&key).is_some() {
            return Err(format!("world {key} is already loaded"));
        }
        let generator_config = self
            .world_generator_registry
            .validate_config(&generator, &toml::Value::Table(Map::new()))?;
        let world_entry = ResolvedWorldConfig {
            key,
            domain,
            name,
            generator,
            generator_config,
            seed,
            default_gamemode: GameType::Survival,
            difficulty: Normal,
            bonus_chest,
            storage: StorageSelection::default_world_disk(),
            nether_portal_target: None,
            end_portal_target: None,
        };
        let (sender, receiver) = oneshot::channel();
        let server = Arc::clone(self);
        self.chunk_runtime.spawn(async move {
            let result = match server.load_world_from_config_tracked(world_entry).await {
                Ok(request) => match request.wait().await {
                    WorldCreationState::Ready => Ok(()),
                    WorldCreationState::Failed(error) => Err(error),
                    WorldCreationState::Pending => {
                        Err("world creation request ended pending".to_owned())
                    }
                },
                Err(error) => Err(error),
            };
            let _ = sender.send(result);
        });
        let id = NEXT_WORLD_CREATION_ID.fetch_add(1, Ordering::Relaxed);
        Ok(WorldCreationRequest { id, receiver })
    }

    /// Publishes the latest server-wide player metadata for synchronous plugin lookups.
    pub fn publish_global_player_data(&self, uuid: Uuid, data: GlobalPlayerData) {
        self.global_player_data.write().insert(uuid, data);
    }

    /// Returns the cached server-wide player metadata without performing I/O.
    #[must_use]
    pub fn global_player_data(&self, uuid: Uuid) -> Option<GlobalPlayerData> {
        self.global_player_data.read().get(&uuid).cloned()
    }

    /// Returns a cached statistic for the player's last active domain.
    #[must_use]
    pub fn offline_statistic(&self, uuid: Uuid, statistic: &str) -> i32 {
        let Some(data) = self.global_player_data(uuid) else {
            return 0;
        };
        let value = match statistic {
            "JUMP" => "minecraft:jump",
            "TIME_SINCE_REST" => "minecraft:time_since_rest",
            _ => return 0,
        };
        data.statistics
            .iter()
            .find(|entry| entry.stat_type == "minecraft:custom" && entry.value == value)
            .map_or(0, |entry| entry.count)
    }

    /// Returns UUIDs of all currently connected players for plugin recipient sets.
    pub fn online_player_ids(&self) -> Vec<Uuid> {
        let mut ids = Vec::new();
        self.online_players.iter_players(|uuid, _| {
            ids.push(*uuid);
            true
        });
        ids
    }

    /// Queues asynchronous saves for every currently connected player.
    pub fn request_save_players(self: &Arc<Self>) {
        let Some(runtime) = self
            .worlds
            .values()
            .into_iter()
            .next()
            .map(|world| Arc::clone(&world.chunk_map.chunk_runtime))
        else {
            return;
        };
        let mut players = Vec::new();
        self.online_players.iter_players(|_, player| {
            players.push(Arc::clone(player));
            true
        });
        let server = Arc::clone(self);
        runtime.handle().spawn(async move {
            for player in players {
                if let Err(error) = server.player_data_storage.save(&player).await {
                    log::error!("Failed to save player {}: {error}", player.gameprofile.id);
                }
            }
        });
    }
    pub(crate) fn permission_rule_suggestions(&self) -> Vec<String> {
        let mut suggestions = self
            .command_permission_keys
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let config = self.permission_groups.config_snapshot();
        for group in config.groups.values() {
            suggestions.extend(group.allow.iter().cloned());
            suggestions.extend(group.deny.iter().cloned());
        }
        for (_, state) in self.player_permission_states.read().entries() {
            suggestions.extend(state.overrides().entries().iter().map(|entry| {
                PermissionRuleExpression::new(entry.key().clone(), entry.context().clone())
                    .to_string()
            }));
        }
        suggestions.into_iter().collect()
    }

    pub(crate) fn permission_metadata_suggestions(&self) -> Vec<String> {
        let mut suggestions = BTreeSet::new();
        let config = self.permission_groups.config_snapshot();
        for group in config.groups.values() {
            suggestions.extend(group.metadata.iter().map(|rule| rule.key.clone()));
        }
        for (_, state) in self.player_permission_states.read().entries() {
            suggestions.extend(state.metadata_overrides().entries().iter().map(|entry| {
                PermissionMetadataExpression::new(entry.key().clone(), entry.context().clone())
                    .to_string()
            }));
        }
        suggestions.into_iter().collect()
    }

    /// Creates a new server with only Foton's built-in commands.
    /// Builds a world using the same validated generator/storage pipeline as startup.
    ///
    /// The caller must insert the returned world into WorldMap and attach it at a
    /// tick safe-point.
    pub(crate) async fn build_world_from_config(
        &self,
        chunk_runtime: Arc<Runtime>,
        world_entry: &ResolvedWorldConfig,
    ) -> Result<Arc<World>, String> {
        let world_path = self
            .world_save_path
            .join(&world_entry.domain)
            .join("worlds")
            .join(&world_entry.name);
        let storage_output = self
            .world_storage_registry
            .create(&world_entry.storage, &self.world_save_path, &world_path)
            .map_err(|error| {
                format!("failed to create storage for {}: {error}", world_entry.key)
            })?;
        let world_seed = LevelDataManager::load_seed_or_default(
            storage_output.level_data_path.as_deref(),
            world_entry.seed,
        )
        .await
        .map_err(|error| {
            format!(
                "failed to load level data seed for {}: {error}",
                world_entry.key
            )
        })?;
        let generator_output = self
            .world_generator_registry
            .create(
                storage_output.level_data_path.as_deref(),
                &world_entry.generator_config,
                world_seed,
                Arc::clone(&self.generation_pool),
            )
            .map_err(|error| {
                format!(
                    "failed to create generator for {}: {error}",
                    world_entry.key
                )
            })?;
        let generation_settings = generation_settings_for_world(world_entry, &generator_output);
        let world = World::new_with_config_and_encoding_pool(
            chunk_runtime,
            world_entry.key.clone(),
            generator_output.dimension_type,
            world_seed,
            WorldConfig {
                storage: storage_output.storage,
                level_data_path: storage_output
                    .level_data_path
                    .map(|path| path.to_string_lossy().into_owned()),
                generator: Arc::new(generator_output.generator),
                generation_settings,
                view_distance: self.config.view_distance,
                simulation_distance: self.config.simulation_distance,
                max_chained_neighbor_updates: self.config.max_chained_neighbor_updates,
                compression: self.config.compression,
                is_flat: generator_output.is_flat,
                sea_level: generator_output.sea_level,
                default_gamemode: world_entry.default_gamemode,
                difficulty: world_entry.difficulty,
                bonus_chest: world_entry.bonus_chest,
            },
            Arc::clone(&self.generation_pool),
            Arc::clone(&self.chunk_encoding_pool),
        )
        .await
        .map_err(|error| format!("failed to create world {}: {error}", world_entry.key))?;
        world
            .initialize_spawn_if_needed_with_bonus_chest(world_entry.bonus_chest)
            .await
            .map_err(|error| {
                format!(
                    "failed to initialize spawn for {}: {error}",
                    world_entry.key
                )
            })?;
        Ok(world)
    }

    pub(crate) async fn load_world_from_config_tracked(
        self: &Arc<Self>,
        world_entry: ResolvedWorldConfig,
    ) -> Result<WorldCreationRequest, String> {
        if self.worlds.get(&world_entry.key).is_some() {
            return Err(format!("world {} is already loaded", world_entry.key));
        }
        let world = self
            .build_world_from_config(Arc::clone(&self.chunk_runtime), &world_entry)
            .await?;
        world.attach_server(self);
        let id = NEXT_WORLD_CREATION_ID.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut pending = self.pending_world_additions.lock();
        if pending
            .iter()
            .any(|(pending_world, _)| pending_world.key == world_entry.key)
        {
            return Err(format!(
                "world {} is already pending attachment",
                world_entry.key
            ));
        }
        pending.push((world, sender));
        Ok(WorldCreationRequest { id, receiver })
    }

    pub async fn new(
        chunk_runtime: Arc<Runtime>,
        cancel_token: CancellationToken,
        config: RuntimeConfig,
        worlds_config: WorldsConfig,
        permission_groups: PermissionGroupManager,
    ) -> Result<Self, String> {
        Self::new_with_commands(
            chunk_runtime,
            cancel_token,
            config,
            worlds_config,
            permission_groups,
            CommandRegistry::new(),
        )
        .await
    }

    /// Creates a new server and atomically merges startup command extensions after built-ins.
    #[expect(
        clippy::too_many_lines,
        reason = "server initialization is a single cohesive flow"
    )]
    pub async fn new_with_commands(
        chunk_runtime: Arc<Runtime>,
        cancel_token: CancellationToken,
        config: RuntimeConfig,
        worlds_config: WorldsConfig,
        permission_groups: PermissionGroupManager,
        command_registry: CommandRegistry,
    ) -> Result<Self, String> {
        validate_login_security(config.online_mode, config.encryption).map_err(str::to_owned)?;
        let config = Arc::new(config);
        init_globals()?;
        log::info!(
            "Foton is not affiliated with Mojang or Microsoft. Use is subject to the Minecraft EULA: https://aka.ms/MinecraftEULA"
        );

        // Authlib starts this fetch alongside server initialization and waits on first use.
        // Foton completes the same initial attempt before opening its listener.
        let service_keys = Arc::new(
            ServiceKeyStore::new(config.services_server.as_deref())
                .map_err(|error| format!("failed to configure Minecraft services keys: {error}"))?,
        );
        let service_keys_ready = service_keys.start(cancel_token.clone());

        let registry_cache = RegistryCache::new(config.compression);

        let (generator_registry, storage_registry) = world_config_registries()?;
        let resolved_worlds = worlds_config
            .validate_and_resolve(&generator_registry, &storage_registry)
            .map_err(|e| format!("failed to validate worlds.toml: {e}"))?;

        let generation_pool: Arc<ThreadPool> = Arc::new({
            let mut builder = ThreadPoolBuilder::new().thread_name(|i| format!("rayon-gen-{i}"));
            if let Some(chunk_generation_threads) =
                configured_chunk_generation_threads(config.chunk_generation_threads)
            {
                builder = builder.num_threads(chunk_generation_threads);
            }
            // Debug builds have deep call chains in density functions that overflow the default 2 MB stack
            if cfg!(debug_assertions) {
                builder = builder.stack_size(8 * 1024 * 1024);
            }
            builder
                .build()
                .map_err(|e| format!("failed to create generation thread pool: {e}"))?
        });
        let chunk_encoding_pool = Arc::new({
            let mut builder =
                ThreadPoolBuilder::new().thread_name(|i| format!("rayon-chunk-encode-{i}"));
            if let Some(chunk_encoding_threads) =
                configured_chunk_encoding_threads(config.chunk_encoding_threads)
            {
                builder = builder.num_threads(chunk_encoding_threads);
            }
            builder
                .build()
                .map_err(|e| format!("failed to create chunk encoding thread pool: {e}"))?
        });

        let player_data_storage = PlayerDataStorage::new(
            resolved_worlds.save_path.clone(),
            resolved_worlds.player_storage.clone(),
        )
        .await
        .map_err(|e| format!("failed to create player data storage: {e}"))?;
        let player_permission_states = player_data_storage
            .load_permission_subjects()
            .await
            .map_err(|error| format!("failed to load player permissions: {error}"))?;
        let known_players = player_data_storage
            .load_known_players()
            .await
            .map_err(|error| format!("failed to load known players: {error}"))?;
        let mut global_player_data = FxHashMap::default();
        for known in known_players.entries() {
            if let Some(data) = player_data_storage
                .load_global(known.uuid())
                .await
                .map_err(|error| format!("failed to load global data: {error}"))?
            {
                let mut data = data;
                if !data.last_active_domain.is_empty() {
                    if let Some(domain_data) = player_data_storage
                        .load_domain(&data.last_active_domain, known.uuid())
                        .await
                        .map_err(|error| format!("failed to load player statistics: {error}"))?
                    {
                        data.statistics = domain_data.statistics;
                    }
                }
                global_player_data.insert(known.uuid(), data);
            }
        }
        let worlds = WorldMap::new(
            resolved_worlds.default_domain.clone(),
            &resolved_worlds.domains,
            &resolved_worlds.worlds,
        );

        for world_entry in &resolved_worlds.worlds {
            let default_world_path = resolved_worlds
                .save_path
                .join(&world_entry.domain)
                .join("worlds")
                .join(&world_entry.name);
            let storage_output = storage_registry
                .create(
                    &world_entry.storage,
                    &resolved_worlds.save_path,
                    Path::new(&default_world_path),
                )
                .map_err(|e| format!("failed to create storage for {}: {e}", world_entry.key))?;
            let world_seed = LevelDataManager::load_seed_or_default(
                storage_output.level_data_path.as_deref(),
                world_entry.seed,
            )
            .await
            .map_err(|e| {
                format!(
                    "failed to load level data seed for {}: {e}",
                    world_entry.key
                )
            })?;
            let generator_output = generator_registry
                .create(
                    storage_output.level_data_path.as_deref(),
                    &world_entry.generator_config,
                    world_seed,
                    generation_pool.clone(),
                )
                .map_err(|e| format!("failed to create generator for {}: {e}", world_entry.key))?;
            let generation_settings = generation_settings_for_world(world_entry, &generator_output);
            let world = World::new_with_config_and_encoding_pool(
                chunk_runtime.clone(),
                world_entry.key.clone(),
                generator_output.dimension_type,
                world_seed,
                WorldConfig {
                    storage: storage_output.storage,
                    level_data_path: storage_output
                        .level_data_path
                        .map(|path| path.to_string_lossy().into_owned()),
                    generator: Arc::new(generator_output.generator),
                    generation_settings,
                    view_distance: config.view_distance,
                    simulation_distance: config.simulation_distance,
                    max_chained_neighbor_updates: config.max_chained_neighbor_updates,
                    compression: config.compression,
                    is_flat: generator_output.is_flat,
                    sea_level: generator_output.sea_level,
                    default_gamemode: world_entry.default_gamemode,
                    difficulty: world_entry.difficulty,
                    bonus_chest: world_entry.bonus_chest,
                },
                generation_pool.clone(),
                Arc::clone(&chunk_encoding_pool),
            )
            .await
            .map_err(|e| format!("failed to create world {}: {e}", world_entry.key))?;
            world
                .initialize_spawn_if_needed_with_bonus_chest(world_entry.bonus_chest)
                .await
                .map_err(|e| format!("failed to initialize spawn for {}: {e}", world_entry.key))?;
            worlds
                .insert(world_entry.key.clone(), world)
                .map_err(|error| format!("failed to publish world {}: {error}", world_entry.key))?;
        }

        let scoreboards = DomainScoreboards::load(&worlds)
            .await
            .map_err(|error| format!("failed to load domain scoreboards: {error}"))?;
        let command_storage = DomainCommandStorage::load(&worlds)
            .await
            .map_err(|error| format!("failed to load domain command storage: {error}"))?;
        let map_data = DomainMapData::load(&worlds)
            .map_err(|error| format!("failed to load domain map data: {error}"))?;
        let boss_bars = DomainCustomBossEvents::load(&worlds)
            .await
            .map_err(|error| format!("failed to load domain boss bars: {error}"))?;
        let registered_commands = create_registered_dispatcher(command_registry)
            .map_err(|error| format!("failed to register commands: {error}"))?;
        let command_permission_keys = registered_commands
            .permissions
            .into_iter()
            .map(|permission| permission.as_str().to_owned())
            .collect();

        if service_keys_ready.await.is_err() {
            log::error!("Minecraft services key fetch task stopped before its initial attempt");
        }

        Ok(Server {
            config,
            chunk_runtime,
            permission_groups,
            cancel_token,
            key_store: KeyStore::create(),
            worlds,
            world_save_path: resolved_worlds.save_path.clone(),
            online_players: PlayerMap::new(),
            player_admissions: SyncMutex::new(FxHashMap::default()),
            registry_cache,
            tick_rate_manager: SyncRwLock::new(TickRateManager::new()),
            scoreboards,
            command_storage,
            map_data,
            boss_bars,
            command_dispatcher: SyncRwLock::new(registered_commands.dispatcher),
            functions: FunctionManager::new(resolved_worlds.save_path.join("datapacks")),
            command_permission_keys,
            command_requests: CommandRequestQueue::new(),
            packet_processor: PacketProcessor::new(),
            world_generator_registry: generator_registry,
            world_storage_registry: storage_registry,
            generation_pool,
            chunk_encoding_pool,
            jobs: ServerJobQueue::new(),
            player_data_storage,
            global_player_data: SyncRwLock::new(global_player_data),
            player_permission_states: SyncRwLock::new(player_permission_states),
            player_permission_updates: AsyncMutex::new(()),
            known_players: SyncMutex::new(KnownPlayerCacheState::new(known_players)),
            known_player_save_idle: Notify::new(),
            profile_lookup_client: reqwest::Client::new(),
            service_keys,
            pending_player_joins: PlayerJoinQueue::new(),
            pending_player_disconnects: PlayerDisconnectQueue::new(),
            pending_world_changes: SyncMutex::new(vec![]),
            pending_world_removals: SyncMutex::new(vec![]),
            pending_world_additions: SyncMutex::new(vec![]),
            events: EventBus::new(),
            pending_domain_switches: SyncMutex::new(vec![]),
        })
    }

    /// Returns the current player-certificate validator, if service keys are available.
    pub fn profile_key_signature_validator(&self) -> Option<Arc<ProfileKeyValidator>> {
        self.service_keys.profile_key_validator()
    }

    /// Returns whether secure chat can currently be enforced.
    #[must_use]
    pub fn enforces_secure_chat(&self) -> bool {
        self.config.enforce_secure_chat
            && self.config.online_mode
            && self.profile_key_signature_validator().is_some()
    }

    /// Saves all dirty domain command storage through domain default worlds.
    pub async fn save_command_storage(&self) -> io::Result<usize> {
        self.command_storage.save(&self.worlds).await
    }

    /// Saves all command-owned persistent data while allowing each data set to fail independently.
    pub async fn save_command_data(&self) -> CommandDataSaveResults {
        CommandDataSaveResults {
            scoreboards: self.scoreboards.save(&self.worlds).await,
            storage: self.save_command_storage().await,
            maps: self.map_data.save(&self.worlds).await,
            boss_bars: self.boss_bars.save(&self.worlds).await,
        }
    }

    /// Links every loaded world back to this server.
    ///
    /// A world is built before the server that owns it, so the link cannot be
    /// passed to `World::new`. Nothing that runs before this call may rely on
    /// [`crate::world::World::server`].
    pub fn attach_worlds(self: &Arc<Self>) {
        for snapshot in self.worlds.snapshots() {
            snapshot.world().attach_server(self);
        }
    }

    /// Reloads every datapack function and reports what the load produced.
    ///
    /// Vanilla parity: the function half of `ReloadableServerResources`, which
    /// the server runs once at startup and again on a resource reload.
    pub(crate) fn reload_functions(self: &Arc<Self>) -> FunctionReloadReport {
        let compilation_source = self.function_source();
        let dispatcher = self.command_dispatcher.read();
        self.functions.reload(&dispatcher, &compilation_source)
    }

    /// Snapshot of datapacks accepted by the active function/resource scan.
    pub fn datapack_records(&self, enabled_only: bool) -> Vec<String> {
        self.functions.datapack_records(enabled_only)
    }

    /// Runs `visit` against the live command graph.
    ///
    /// Instantiating a function macro reparses its lines, so it needs the same
    /// dispatcher the load used. No command holds this lock while it runs, so
    /// a command executor may take it.
    pub(crate) fn with_command_dispatcher<R>(
        &self,
        visit: impl FnOnce(&CommandDispatcher) -> R,
    ) -> R {
        visit(&self.command_dispatcher.read())
    }

    /// The source `.mcfunction` lines are compiled and run against.
    ///
    /// Vanilla parity: `Commands.createCompilationContext` for the load and
    /// `ServerFunctionManager.getGameLoopSender` for the tick, both of which are
    /// a server-level source at gamemaster permission with suppressed output.
    pub(crate) fn function_source(self: &Arc<Self>) -> CommandSource {
        CommandSource::new(CommandSender::Console, Arc::clone(self)).with_suppressed_output()
    }

    /// Runs one command to completion inside the current tick.
    ///
    /// Vanilla parity: the `commands.performPrefixedCommand` of
    /// `BaseCommandBlock.performCommand`, which runs synchronously because the
    /// command block reads its own success count in the same tick -- that count
    /// is what a comparator reports and what gates a conditional chain.
    ///
    /// [`Self::submit_command`] is the queued path used by chat and the
    /// console; it cannot serve a command block, because the queue is drained
    /// before the world tick that would read the result.
    ///
    /// A command that suspends -- today only a structure search waiting on
    /// chunk generation -- is cancelled rather than carried into the next tick,
    /// and counts as no successes. Vanilla blocks the server thread there
    /// instead; a game tick may not wait.
    pub(crate) fn run_command_now(self: &Arc<Self>, source: CommandSource, command: &str) -> i32 {
        let successes = Arc::new(AtomicI32::new(0));
        let counter = Arc::clone(&successes);
        let source = source.with_callback(CommandResultCallback::new(move |success, _result| {
            if success {
                counter.fetch_add(1, AtomicOrdering::Relaxed);
            }
        }));

        let command = command.strip_prefix('/').unwrap_or(command);
        let chain = {
            let dispatcher = self.command_dispatcher.read();
            let parse = dispatcher.parse(command, source.clone());
            dispatcher.context_chain(parse)
        };
        let chain = match chain {
            Ok(chain) => chain,
            Err(error) => {
                source.handle_error(&error, false);
                return 0;
            }
        };

        let mut execution = CommandExecutionContext::for_source(&source);
        let callback = source.callback();
        execution.queue_initial_command(chain, source, callback);
        if execution.run() == ExecutionStop::Suspended {
            log::warn!(
                "command block command `{command}` suspended on chunk work and was cancelled"
            );
            execution.cancel();
        }

        successes.load(AtomicOrdering::Relaxed)
    }

    /// Queues a command for execution at the start of the next game tick.
    pub fn submit_command(
        &self,
        sender: CommandSender,
        command: String,
    ) -> Result<(), CommandQueueFull> {
        self.command_requests.submit(CommandRequest::Execute {
            owner: CommandExecutionOwner::capture(sender, self),
            command,
        })
    }

    /// Queues one Rcon command and hands back the reply its client waits on.
    ///
    /// Vanilla parity: `DedicatedServer.runCommand`, which clears the shared
    /// Rcon buffer, blocks the Rcon thread on the server thread with
    /// `executeBlocking`, and reads the buffer back. A Foton tick may not be
    /// blocked on, so the wait moves to the caller: the reply arrives when the
    /// last handle to the command's output sink is dropped, which happens
    /// whether the command completed, failed to parse, hit the command limit,
    /// overflowed the execution queue, or was cancelled at shutdown.
    ///
    /// # Errors
    /// Returns [`CommandQueueFull`] when the command request queue is full,
    /// in which case no reply will ever be sent.
    pub fn submit_rcon_command(
        &self,
        connection: u64,
        command: String,
    ) -> Result<oneshot::Receiver<String>, CommandQueueFull> {
        let (output, reply) = RconOutput::new(connection);
        self.submit_command(CommandSender::Rcon(Arc::new(output)), command)?;
        Ok(reply)
    }

    pub(crate) fn submit_command_suggestions(
        &self,
        player: Arc<Player>,
        transaction_id: i32,
        input: String,
    ) -> Result<(), CommandQueueFull> {
        self.command_requests.submit(CommandRequest::Suggestions {
            owner: CommandExecutionOwner::capture(CommandSender::Player(player), self),
            transaction_id,
            input,
        })
    }

    /// Schedules a decoded play packet for the inter-tick packet phase.
    pub(crate) fn schedule_play_packet(
        &self,
        player: Arc<Player>,
        packet: ScheduledPlayPacket,
        payload_bytes: usize,
    ) {
        self.packet_processor
            .schedule(player, packet, payload_bytes);
    }

    /// Returns Brigadier completions visible to a command sender.
    pub fn command_completions(
        self: &Arc<Self>,
        sender: CommandSender,
        input: &str,
    ) -> Vec<CommandCompletion> {
        if !CommandExecutionOwner::capture(sender.clone(), self).is_current(self) {
            return Vec::new();
        }
        match self.build_command_suggestions(sender, input) {
            Ok(suggestions) => {
                let range = suggestions.range();
                suggestions
                    .list()
                    .iter()
                    .map(|suggestion| {
                        CommandCompletion::new(
                            range.start(),
                            range.len(),
                            suggestion.text().to_owned(),
                        )
                    })
                    .collect()
            }
            Err(error) => {
                tracing::warn!(%error, "failed to build command suggestions");
                Vec::new()
            }
        }
    }
}
