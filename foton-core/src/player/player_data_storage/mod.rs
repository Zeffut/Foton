//! Player data storage for global and domain-scoped player state.

mod known_players;
mod permissions;

use std::{
    hash::{Hash, Hasher},
    io::{Cursor, Read as _},
    path::{Component, Path, PathBuf},
    str::from_utf8,
};

use rustc_hash::FxHasher;
use simdnbt::{ToNbtTag, borrow::read_compound as read_borrowed_compound, owned::NbtTag};
use tokio::{
    fs,
    io::{self, AsyncReadExt, AsyncWriteExt},
};
use uuid::Uuid;
use wincode::{SchemaRead, SchemaWrite};

#[cfg(test)]
use self::permissions::set_permission_subject;
use self::{
    known_players::{KnownPlayersFile, decode_known_players_file, encode_known_players_file},
    permissions::{PlayerPermissionsFile, serialize_player_permissions_file},
};
use super::PlayerRespawnConfig;
use super::player_data::{
    PLAYER_DATA_VERSION, PersistentAbilities, PersistentAdvancement, PersistentCriterion,
    PersistentEnderPearl, PersistentPlayerData, PersistentRootVehicle, PersistentSlot,
    PersistentStatistic,
};
use crate::chunk_saver::PersistentEntity;
use crate::config::StorageSelection;
use crate::level_data::RespawnData;
use crate::permission::PermissionSubjectIndex;
#[cfg(test)]
use crate::permission::PermissionSubjectState;
use crate::player::KnownPlayers;
use crate::player::Player;
use foton_registry::item_stack::ItemStack;
use foton_utils::locks::AsyncMutex;
use foton_utils::{BlockPos, Identifier};

const PLAYER_MAGIC: [u8; 4] = *b"STLP";
const GLOBAL_MAGIC: [u8; 4] = *b"STLG";
const PLAYER_STORAGE_VERSION: u16 = 11;
const GLOBAL_STORAGE_VERSION: u16 = 3;
const GLOBAL_PLAYER_DATA_VERSION: i32 = 2;
const MAX_PLAYER_DATA_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DECOMPRESSED_PLAYER_DATA_BYTES: usize = 64 * 1024 * 1024;
const PLAYER_DATA_DECOMPRESSION_BUFFER_BYTES: usize = 8 * 1024;
const FILE_LOCK_STRIPES: usize = 256;

#[derive(Debug)]
enum AtomicWriteOutcome {
    Durable,
    /// The new primary is already visible, but a post-rename durability or
    /// backup-rotation operation failed.
    CommittedWithError(io::Error),
}

impl AtomicWriteOutcome {
    fn into_result(self) -> io::Result<()> {
        match self {
            Self::Durable => Ok(()),
            Self::CommittedWithError(error) => Err(error),
        }
    }
}

/// Server-wide player data.
#[derive(Debug, Clone)]
pub struct GlobalPlayerData {
    /// Last active domain for reconnects.
    pub last_active_domain: String,
    /// Unix epoch milliseconds of the first observed login.
    pub first_played: i64,
    /// Unix epoch milliseconds of the most recent observed login/logout.
    pub last_played: i64,
    /// Statistics from the player's last active domain, cached for synchronous
    /// `OfflinePlayer` lookups.
    pub statistics: Vec<PersistentStatistic>,
    /// Whether this player is allowed by the server whitelist.
    pub whitelisted: bool,
}

/// Result of a state write after its new primary file becomes visible.
#[derive(Debug)]
pub enum PersistenceUpdateOutcome {
    /// The primary and its directory metadata were durably synchronized.
    Durable,
    /// The primary is visible and must be published to memory, but a later
    /// durability or backup-rotation operation failed.
    CommittedWithError(io::Error),
}

/// Manages player data persistence.
pub struct PlayerDataStorage {
    backend: PlayerDataStorageBackend,
}

enum PlayerDataStorageBackend {
    File(FilePlayerDataStorage),
}

struct FilePlayerDataStorage {
    save_root: PathBuf,
    file_locks: Box<[AsyncMutex<()>]>,
}

#[derive(SchemaWrite, SchemaRead)]
struct PlayerDataFile {
    data_version: i32,
    pos: [f64; 3],
    motion: [f64; 3],
    rotation: [f32; 2],
    on_ground: bool,
    fall_flying: bool,
    remaining_fire_ticks: i32,
    ticks_frozen: i32,
    is_in_powder_snow: bool,
    was_in_powder_snow: bool,
    has_visual_fire: bool,
    health: f32,
    game_mode: i32,
    prev_game_mode: Option<i32>,
    abilities: AbilitiesFile,
    inventory: Vec<SlotFile>,
    /// Ender chest slots, saved beside the inventory.
    ender_items: Vec<SlotFile>,
    selected_slot: i32,
    world: String,
    food_level: i32,
    food_saturation_level: f32,
    food_exhaustion_level: f32,
    food_tick_timer: i32,
    experience_level: i32,
    experience_progress: f32,
    experience_total: i32,
    enchantment_seed: i32,
    score: i32,
    seen_credits: bool,
    warden_spawn_tracker: [i32; 3],
    root_vehicle: Option<RootVehicleFile>,
    respawn_config: Option<RespawnConfigFile>,
    ender_pearls: Vec<EnderPearlFile>,
    /// Advancement progress, one entry per advancement with anything to save.
    advancements: Vec<AdvancementFile>,
    /// Statistics, keyed by name rather than by registry id.
    statistics: Vec<StatisticFile>,
    /// Written `LivingEntity.addAdditionalSaveData` compound.
    living_nbt: Vec<u8>,
}

#[derive(SchemaWrite, SchemaRead)]
struct AdvancementFile {
    key: String,
    criteria: Vec<CriterionFile>,
}

#[derive(SchemaWrite, SchemaRead)]
struct CriterionFile {
    name: String,
    obtained_epoch_millis: i64,
}

#[derive(SchemaWrite, SchemaRead)]
struct StatisticFile {
    stat_type: String,
    value: String,
    count: i32,
}

#[derive(SchemaWrite, SchemaRead)]
struct RootVehicleFile {
    attach: [u8; 16],
    entity: PersistentEntity,
}

#[derive(SchemaWrite, SchemaRead)]
struct RespawnConfigFile {
    dimension: String,
    pos: [i32; 3],
    yaw: f32,
    pitch: f32,
    forced: bool,
}

#[derive(SchemaWrite, SchemaRead)]
struct EnderPearlFile {
    world: String,
    entity: PersistentEntity,
}

#[derive(SchemaWrite, SchemaRead)]
struct AbilitiesFile {
    invulnerable: bool,
    flying: bool,
    may_fly: bool,
    instabuild: bool,
    may_build: bool,
    flying_speed: f32,
    walking_speed: f32,
}

#[derive(SchemaWrite, SchemaRead)]
struct SlotFile {
    slot: i8,
    item_nbt: Vec<u8>,
}

#[derive(SchemaWrite, SchemaRead)]
struct GlobalPlayerDataFile {
    data_version: i32,
    last_active_domain: String,
    first_played: i64,
    last_played: i64,
    whitelisted: bool,
}

#[derive(SchemaWrite, SchemaRead)]
struct LegacyGlobalPlayerDataFile {
    data_version: i32,
    last_active_domain: String,
}

#[derive(SchemaWrite, SchemaRead)]
struct LegacyGlobalPlayerDataFileV2 {
    data_version: i32,
    last_active_domain: String,
    first_played: i64,
    last_played: i64,
}

impl PlayerDataStorage {
    /// Creates player data storage from config.
    pub async fn new(save_root: PathBuf, selection: StorageSelection) -> io::Result<Self> {
        if selection.kind != Identifier::from_foton("file") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown player storage {}", selection.kind),
            ));
        }
        let backend = PlayerDataStorageBackend::File(FilePlayerDataStorage::new(save_root).await?);
        Ok(Self { backend })
    }

    /// Saves a player's current domain data and global last-active-domain.
    pub async fn save(&self, player: &Player) -> io::Result<()> {
        let domain = player.get_world().domain().to_owned();
        self.save_domain(&domain, player).await?;
        self.save_global(
            player.gameprofile.id,
            &GlobalPlayerData {
                last_active_domain: domain,
                first_played: 0,
                last_played: unix_epoch_millis(),
                statistics: Vec::new(),
                // `save_global` deliberately ignores this snapshot. Whitelist
                // membership has its own serialized update path.
                whitelisted: false,
            },
        )
        .await
    }

    /// Saves a player's data for a specific domain.
    pub async fn save_domain(&self, domain: &str, player: &Player) -> io::Result<()> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => storage.save_domain(domain, player).await,
        }
    }

    /// Saves an already captured player data snapshot for a specific domain.
    pub async fn save_domain_data(
        &self,
        domain: &str,
        uuid: Uuid,
        data: &PersistentPlayerData,
    ) -> io::Result<()> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => {
                storage.save_domain_data(domain, uuid, data).await
            }
        }
    }

    /// Loads a player's data for a specific domain.
    pub async fn load_domain(
        &self,
        domain: &str,
        uuid: Uuid,
    ) -> io::Result<Option<PersistentPlayerData>> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => storage.load_domain(domain, uuid).await,
        }
    }

    /// Loads server-wide player data.
    pub async fn load_global(&self, uuid: Uuid) -> io::Result<Option<GlobalPlayerData>> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => storage.load_global(uuid).await,
        }
    }

    /// Loads all persisted player permission snapshots.
    pub async fn load_permission_subjects(&self) -> io::Result<PermissionSubjectIndex> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => storage.load_permission_subjects().await,
        }
    }

    /// Loads the rebuildable player identity cache, falling back to empty on failure.
    pub async fn load_known_players(&self) -> io::Result<KnownPlayers> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => storage.load_known_players().await,
        }
    }

    /// Persists the identity cache when the caller's snapshot is still current.
    pub async fn save_known_players_if_current(
        &self,
        players: &KnownPlayers,
        is_current: impl FnOnce() -> bool + Send,
    ) -> io::Result<bool> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => {
                storage
                    .save_known_players_if_current(players, is_current)
                    .await
            }
        }
    }

    /// Saves server-wide player data.
    pub async fn save_global(&self, uuid: Uuid, data: &GlobalPlayerData) -> io::Result<()> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => storage.save_global(uuid, data).await,
        }
    }

    /// Changes only whitelist membership, serialized with gameplay metadata updates.
    pub async fn set_player_whitelisted(
        &self,
        uuid: Uuid,
        whitelisted: bool,
        fallback: &GlobalPlayerData,
        is_current: impl FnOnce() -> bool + Send,
        publish: impl FnOnce(GlobalPlayerData) + Send,
    ) -> io::Result<Option<PersistenceUpdateOutcome>> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => {
                storage
                    .set_player_whitelisted(uuid, whitelisted, fallback, is_current, publish)
                    .await
            }
        }
    }

    /// Persists the server's complete UUID-keyed permission snapshot.
    pub async fn save_permission_subjects(
        &self,
        subjects: &PermissionSubjectIndex,
    ) -> io::Result<PersistenceUpdateOutcome> {
        match &self.backend {
            PlayerDataStorageBackend::File(storage) => {
                storage.save_permission_subjects(subjects).await
            }
        }
    }
}

impl FilePlayerDataStorage {
    async fn new(save_root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(save_root.join("global").join("players")).await?;
        Ok(Self {
            save_root,
            file_locks: (0..FILE_LOCK_STRIPES)
                .map(|_| AsyncMutex::new(()))
                .collect(),
        })
    }

    async fn save_domain(&self, domain: &str, player: &Player) -> io::Result<()> {
        let uuid = player.gameprofile.id;
        let data = PersistentPlayerData::from_player(player);
        self.save_domain_data(domain, uuid, &data).await
    }

    async fn save_domain_data(
        &self,
        domain: &str,
        uuid: Uuid,
        data: &PersistentPlayerData,
    ) -> io::Result<()> {
        let file = PlayerDataFile::from_persistent(data)?;
        let bytes = encode_player_file(&file)?;
        let players_dir = self.domain_players_dir(domain)?;
        self.write_atomic(&players_dir, uuid, bytes).await?;
        log::debug!("Saved player data for {uuid} in domain {domain}");
        Ok(())
    }

    async fn load_domain(
        &self,
        domain: &str,
        uuid: Uuid,
    ) -> io::Result<Option<PersistentPlayerData>> {
        let path = Self::player_file(&self.domain_players_dir(domain)?, uuid);
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        if !Self::recover_missing_atomic_path_locked(&path).await? {
            return Ok(None);
        }
        let (data, recovered_bytes) = Self::read_with_valid_backup(&path, |bytes| {
            decode_player_file(bytes)?.into_persistent()
        })
        .await?;
        if let Some(bytes) = recovered_bytes {
            Self::restore_valid_backup_locked(&path, &bytes).await?;
        }
        log::debug!("Loaded player data for {uuid} in domain {domain}");
        Ok(Some(data))
    }

    async fn load_global(&self, uuid: Uuid) -> io::Result<Option<GlobalPlayerData>> {
        let path = Self::player_file(&self.global_players_dir(), uuid);
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        let Some(file) = Self::read_global_file_locked(&path).await? else {
            return Ok(None);
        };
        Ok(Some(Self::global_data_from_file(file, Vec::new())))
    }

    async fn read_global_file_locked(path: &Path) -> io::Result<Option<GlobalPlayerDataFile>> {
        let had_primary = fs::try_exists(path).await?;
        if !Self::recover_missing_atomic_path_locked(path).await? {
            return Ok(None);
        }
        let (mut file, recovered_bytes) =
            Self::read_with_valid_backup(path, decode_global_file).await?;
        if !had_primary || recovered_bytes.is_some() {
            file.whitelisted = false;
            let bytes = encode_global_file(&file)?;
            Self::restore_valid_backup_locked(path, &bytes).await?;
            tracing::warn!(
                path = %path.display(),
                "Recovered global player data with whitelist membership cleared"
            );
        }
        Ok(Some(file))
    }

    fn global_data_from_file(
        file: GlobalPlayerDataFile,
        statistics: Vec<PersistentStatistic>,
    ) -> GlobalPlayerData {
        GlobalPlayerData {
            last_active_domain: file.last_active_domain,
            first_played: file.first_played,
            last_played: file.last_played,
            statistics,
            whitelisted: file.whitelisted,
        }
    }

    async fn load_permission_subjects(&self) -> io::Result<PermissionSubjectIndex> {
        self.load_player_permissions_file()
            .await?
            .into_subject_index()
    }

    async fn load_known_players(&self) -> io::Result<KnownPlayers> {
        let path = self.known_players_file();
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        match Self::read_known_players_file_locked(&path).await {
            Ok(players) => Ok(players),
            Err(error) => {
                log::warn!(
                    "Failed to load known player cache from {}: {error}. Starting with an empty cache",
                    path.display()
                );
                Ok(KnownPlayers::new())
            }
        }
    }

    async fn read_known_players_file_locked(path: &Path) -> io::Result<KnownPlayers> {
        if !Self::recover_missing_atomic_path_locked(path).await? {
            return Ok(KnownPlayers::new());
        }
        let bytes = Self::read_bounded_file(path).await?;
        decode_known_players_file(&bytes)?.into_known_players()
    }

    async fn save_known_players_if_current(
        &self,
        players: &KnownPlayers,
        is_current: impl FnOnce() -> bool + Send,
    ) -> io::Result<bool> {
        let path = self.known_players_file();
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        if !is_current() {
            return Ok(false);
        }
        let bytes = encode_known_players_file(&KnownPlayersFile::from_known_players(players))?;
        Self::write_atomic_path_locked(&path, bytes)
            .await?
            .into_result()?;
        Ok(true)
    }

    pub(crate) async fn save_global(&self, uuid: Uuid, data: &GlobalPlayerData) -> io::Result<()> {
        validate_domain_name(&data.last_active_domain, true, io::ErrorKind::InvalidInput)?;
        let path = Self::player_file(&self.global_players_dir(), uuid);
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        let existing = Self::read_global_file_locked(&path).await?;
        let file = GlobalPlayerDataFile {
            data_version: GLOBAL_PLAYER_DATA_VERSION,
            last_active_domain: data.last_active_domain.clone(),
            first_played: if data.first_played == 0 {
                let existing_first_played = existing.as_ref().map_or(0, |file| file.first_played);
                if existing_first_played == 0 {
                    unix_epoch_millis()
                } else {
                    existing_first_played
                }
            } else {
                data.first_played
            },
            last_played: if data.last_played == 0 {
                existing.as_ref().map_or(0, |file| file.last_played)
            } else {
                data.last_played
            },
            whitelisted: existing.is_some_and(|file| file.whitelisted),
        };
        let bytes = encode_global_file(&file)?;
        Self::write_atomic_path_locked(&path, bytes)
            .await?
            .into_result()
    }

    async fn set_player_whitelisted(
        &self,
        uuid: Uuid,
        whitelisted: bool,
        fallback: &GlobalPlayerData,
        is_current: impl FnOnce() -> bool + Send,
        publish: impl FnOnce(GlobalPlayerData) + Send,
    ) -> io::Result<Option<PersistenceUpdateOutcome>> {
        validate_domain_name(
            &fallback.last_active_domain,
            true,
            io::ErrorKind::InvalidInput,
        )?;
        let path = Self::player_file(&self.global_players_dir(), uuid);
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        if !is_current() {
            return Ok(None);
        }
        let existing = Self::read_global_file_locked(&path).await?;
        let file = GlobalPlayerDataFile {
            data_version: GLOBAL_PLAYER_DATA_VERSION,
            last_active_domain: existing.as_ref().map_or_else(
                || fallback.last_active_domain.clone(),
                |file| file.last_active_domain.clone(),
            ),
            first_played: existing
                .as_ref()
                .map_or(fallback.first_played, |file| file.first_played),
            last_played: existing
                .as_ref()
                .map_or(fallback.last_played, |file| file.last_played),
            whitelisted,
        };
        let bytes = encode_global_file(&file)?;
        let outcome = Self::write_atomic_path_locked(&path, bytes).await?;
        publish(Self::global_data_from_file(
            file,
            fallback.statistics.clone(),
        ));
        Ok(Some(match outcome {
            AtomicWriteOutcome::Durable => PersistenceUpdateOutcome::Durable,
            AtomicWriteOutcome::CommittedWithError(error) => {
                PersistenceUpdateOutcome::CommittedWithError(error)
            }
        }))
    }

    async fn save_permission_subjects(
        &self,
        subjects: &PermissionSubjectIndex,
    ) -> io::Result<PersistenceUpdateOutcome> {
        let path = self.player_permissions_file();
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        let file = PlayerPermissionsFile::from_subject_index(subjects);
        self.write_player_permissions_file_locked(&path, &file)
            .await
    }

    async fn load_player_permissions_file(&self) -> io::Result<PlayerPermissionsFile> {
        let path = self.player_permissions_file();
        let lock = self.file_lock(&path);
        let _guard = lock.lock().await;
        self.read_player_permissions_file_locked(&path).await
    }

    async fn read_player_permissions_file_locked(
        &self,
        path: &Path,
    ) -> io::Result<PlayerPermissionsFile> {
        if !fs::try_exists(path).await? {
            let temp_path = Self::atomic_temp_path(path);
            if fs::try_exists(&temp_path).await?
                && let Err(error) = fs::remove_file(&temp_path).await
            {
                tracing::warn!(
                    %error,
                    path = %temp_path.display(),
                    "Failed to remove an uncommitted permission-file temporary"
                );
            }
            return Ok(PlayerPermissionsFile::default());
        }
        let bytes = Self::read_bounded_file(path).await?;
        Self::decode_player_permissions_file(path, &bytes)
    }

    fn decode_player_permissions_file(
        path: &Path,
        bytes: &[u8],
    ) -> io::Result<PlayerPermissionsFile> {
        let contents = from_utf8(bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "player permissions file {} is not valid UTF-8: {error}",
                    path.display()
                ),
            )
        })?;
        let file = toml::from_str::<PlayerPermissionsFile>(contents).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid player permissions TOML in {}: {error}",
                    path.display()
                ),
            )
        })?;
        file.validate()?;
        Ok(file)
    }

    async fn write_player_permissions_file_locked(
        &self,
        path: &Path,
        file: &PlayerPermissionsFile,
    ) -> io::Result<PersistenceUpdateOutcome> {
        let contents = serialize_player_permissions_file(file).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to serialize player permissions TOML: {error}"),
            )
        })?;
        Ok(
            match Self::write_atomic_path_locked(path, contents.into_bytes()).await? {
                AtomicWriteOutcome::Durable => PersistenceUpdateOutcome::Durable,
                AtomicWriteOutcome::CommittedWithError(error) => {
                    PersistenceUpdateOutcome::CommittedWithError(error)
                }
            },
        )
    }

    fn global_dir(&self) -> PathBuf {
        self.save_root.join("global")
    }

    fn global_players_dir(&self) -> PathBuf {
        self.global_dir().join("players")
    }

    fn player_permissions_file(&self) -> PathBuf {
        self.global_dir().join("player_permissions.toml")
    }

    fn known_players_file(&self) -> PathBuf {
        self.global_dir().join("known_players.dat")
    }

    fn domain_players_dir(&self, domain: &str) -> io::Result<PathBuf> {
        validate_domain_name(domain, false, io::ErrorKind::InvalidInput)?;
        Ok(self.save_root.join(domain).join("players"))
    }

    fn player_file(players_dir: &Path, uuid: Uuid) -> PathBuf {
        players_dir.join(format!("{uuid}.dat"))
    }

    fn file_lock(&self, path: &Path) -> &AsyncMutex<()> {
        let mut hasher = FxHasher::default();
        path.hash(&mut hasher);
        let index = hasher.finish() as usize % self.file_locks.len();
        &self.file_locks[index]
    }

    async fn write_atomic(&self, players_dir: &Path, uuid: Uuid, bytes: Vec<u8>) -> io::Result<()> {
        let final_path = Self::player_file(players_dir, uuid);
        let lock = self.file_lock(&final_path);
        let _guard = lock.lock().await;
        Self::write_atomic_path_locked(&final_path, bytes)
            .await?
            .into_result()
    }

    async fn write_atomic_path_locked(
        final_path: &Path,
        bytes: Vec<u8>,
    ) -> io::Result<AtomicWriteOutcome> {
        if bytes.len() as u64 > MAX_PLAYER_DATA_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "refusing to write {} bytes to player data file {}; the limit is {} bytes",
                    bytes.len(),
                    final_path.display(),
                    MAX_PLAYER_DATA_FILE_BYTES
                ),
            ));
        }
        let Some(parent) = final_path.parent() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "atomic write path has no parent",
            ));
        };
        fs::create_dir_all(parent).await?;
        let temp_path = Self::atomic_temp_path(final_path);
        let backup_path = Self::atomic_backup_path(final_path);
        let backup_temp_path = Self::atomic_temp_path(&backup_path);

        Self::write_synced_file(&temp_path, &bytes).await?;
        let had_primary = fs::try_exists(final_path).await?;
        if had_primary {
            Self::copy_synced_file(final_path, &backup_temp_path).await?;
        }
        fs::rename(&temp_path, final_path).await?;
        // Commit the new primary before rotating its predecessor into the
        // backup slot. At every interruption point either the old backup or
        // the new primary therefore remains intact.
        if let Err(error) = Self::sync_parent(parent).await {
            return Ok(AtomicWriteOutcome::CommittedWithError(error));
        }
        if had_primary {
            // Tokio delegates to `std::fs::rename`; on Windows that operation
            // replaces an existing destination. The native three-generation
            // regression test below keeps this backup rotation guarantee live.
            if let Err(error) = fs::rename(&backup_temp_path, &backup_path).await {
                return Ok(AtomicWriteOutcome::CommittedWithError(error));
            }
        }
        if let Err(error) = Self::sync_parent(parent).await {
            return Ok(AtomicWriteOutcome::CommittedWithError(error));
        }
        Ok(AtomicWriteOutcome::Durable)
    }

    async fn read_with_valid_backup<T>(
        path: &Path,
        decode: impl Fn(&[u8]) -> io::Result<T>,
    ) -> io::Result<(T, Option<Vec<u8>>)> {
        let primary = Self::read_bounded_file(path)
            .await
            .and_then(|bytes| decode(&bytes));
        let primary_error = match primary {
            Ok(value) => return Ok((value, None)),
            Err(error) => error,
        };
        if primary_error.kind() != io::ErrorKind::InvalidData {
            return Err(primary_error);
        }

        let backup_path = Self::atomic_backup_path(path);
        let Ok(backup_bytes) = Self::read_bounded_file(&backup_path).await else {
            return Err(primary_error);
        };
        match decode(&backup_bytes) {
            Ok(value) => {
                tracing::warn!(
                    path = %path.display(),
                    backup = %backup_path.display(),
                    %primary_error,
                    "Loaded the last committed backup because the current data file is invalid"
                );
                Ok((value, Some(backup_bytes)))
            }
            Err(_) => Err(primary_error),
        }
    }

    async fn restore_valid_backup_locked(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let Some(parent) = path.parent() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "atomic recovery path has no parent",
            ));
        };
        let temp_path = Self::atomic_temp_path(path);
        Self::write_synced_file(&temp_path, bytes).await?;
        fs::rename(&temp_path, path).await?;
        Self::sync_parent(parent).await
    }

    async fn read_bounded_file(path: &Path) -> io::Result<Vec<u8>> {
        let file = fs::File::open(path).await?;
        let declared_len = file.metadata().await?.len();
        if declared_len > MAX_PLAYER_DATA_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "player data file {} declares {declared_len} bytes, which exceeds the {} byte limit",
                    path.display(),
                    MAX_PLAYER_DATA_FILE_BYTES
                ),
            ));
        }

        let capacity = usize::try_from(declared_len).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "player data file {} is too large to address",
                    path.display()
                ),
            )
        })?;
        let mut bytes = Vec::with_capacity(capacity);
        file.take(MAX_PLAYER_DATA_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() as u64 > MAX_PLAYER_DATA_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "player data file {} grew beyond the {} byte limit while being read",
                    path.display(),
                    MAX_PLAYER_DATA_FILE_BYTES
                ),
            ));
        }
        Ok(bytes)
    }

    fn atomic_temp_path(path: &Path) -> PathBuf {
        let extension = path.extension().and_then(|value| value.to_str());
        path.with_extension(match extension {
            Some(extension) => format!("{extension}.tmp"),
            None => "tmp".to_owned(),
        })
    }

    fn atomic_backup_path(path: &Path) -> PathBuf {
        let extension = path.extension().and_then(|value| value.to_str());
        path.with_extension(match extension {
            Some(extension) => format!("{extension}_old"),
            None => "old".to_owned(),
        })
    }

    async fn recover_missing_atomic_path_locked(final_path: &Path) -> io::Result<bool> {
        if fs::try_exists(final_path).await? {
            return Ok(true);
        }

        let backup_path = Self::atomic_backup_path(final_path);
        if fs::try_exists(&backup_path).await? {
            fs::rename(&backup_path, final_path).await?;
            let Some(parent) = final_path.parent() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "atomic recovery path has no parent",
                ));
            };
            Self::sync_parent(parent).await?;
            let temp_path = Self::atomic_temp_path(final_path);
            if fs::try_exists(&temp_path).await?
                && let Err(error) = fs::remove_file(&temp_path).await
            {
                tracing::warn!(
                    %error,
                    path = %temp_path.display(),
                    "Failed to remove an uncommitted atomic-write temporary file"
                );
            }
            tracing::warn!(
                path = %final_path.display(),
                backup = %backup_path.display(),
                "Recovered a missing data file from its last committed backup"
            );
            return Ok(true);
        }

        let temp_path = Self::atomic_temp_path(final_path);
        if fs::try_exists(&temp_path).await? {
            if let Err(error) = fs::remove_file(&temp_path).await {
                tracing::warn!(
                    %error,
                    path = %temp_path.display(),
                    "Failed to remove an uncommitted atomic-write temporary file"
                );
            }
            tracing::warn!(
                path = %final_path.display(),
                temporary = %temp_path.display(),
                "Discarded an interrupted data-file publication with no committed generation"
            );
        }

        Ok(false)
    }

    async fn write_synced_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = fs::File::create(path).await?;
        file.write_all(bytes).await?;
        file.sync_all().await
    }

    async fn copy_synced_file(source: &Path, destination: &Path) -> io::Result<()> {
        let mut source = fs::File::open(source).await?;
        let mut destination = fs::File::create(destination).await?;
        io::copy(&mut source, &mut destination).await?;
        destination.sync_all().await
    }

    async fn sync_parent(parent: &Path) -> io::Result<()> {
        // Runtime check so the `.await`s stay present on all platforms for clippy;
        // on Windows the branch never runs (directory fsync is unix-only).
        if cfg!(unix) {
            fs::File::open(parent).await?.sync_all().await?;
        }
        Ok(())
    }
}

impl PlayerDataFile {
    fn from_persistent(data: &PersistentPlayerData) -> io::Result<Self> {
        let mut ender_items = Vec::with_capacity(data.ender_items.len());
        for slot in &data.ender_items {
            ender_items.push(SlotFile {
                slot: slot.slot,
                item_nbt: item_to_nbt_bytes(&slot.item)?,
            });
        }

        let mut inventory = Vec::with_capacity(data.inventory.len());
        for slot in &data.inventory {
            inventory.push(SlotFile {
                slot: slot.slot,
                item_nbt: item_to_nbt_bytes(&slot.item)?,
            });
        }

        let file = Self {
            data_version: data.data_version,
            pos: data.pos,
            motion: data.motion,
            rotation: data.rotation,
            on_ground: data.on_ground,
            fall_flying: data.fall_flying,
            remaining_fire_ticks: data.remaining_fire_ticks,
            ticks_frozen: data.ticks_frozen,
            is_in_powder_snow: data.is_in_powder_snow,
            was_in_powder_snow: data.was_in_powder_snow,
            has_visual_fire: data.has_visual_fire,
            health: data.health,
            game_mode: data.game_mode,
            prev_game_mode: data.prev_game_mode,
            abilities: AbilitiesFile {
                invulnerable: data.abilities.invulnerable,
                flying: data.abilities.flying,
                may_fly: data.abilities.may_fly,
                instabuild: data.abilities.instabuild,
                may_build: data.abilities.may_build,
                flying_speed: data.abilities.flying_speed,
                walking_speed: data.abilities.walking_speed,
            },
            inventory,
            ender_items,
            selected_slot: data.selected_slot,
            world: data.world.clone(),
            food_level: data.food_level,
            food_saturation_level: data.food_saturation_level,
            food_exhaustion_level: data.food_exhaustion_level,
            food_tick_timer: data.food_tick_timer,
            experience_level: data.experience_level,
            experience_progress: data.experience_progress,
            experience_total: data.experience_total,
            enchantment_seed: data.enchantment_seed,
            score: data.score,
            seen_credits: data.seen_credits,
            warden_spawn_tracker: data.warden_spawn_tracker,
            root_vehicle: data
                .root_vehicle
                .clone()
                .map(|root_vehicle| RootVehicleFile {
                    attach: root_vehicle.attach,
                    entity: root_vehicle.entity,
                }),
            respawn_config: data
                .respawn_config
                .clone()
                .map(RespawnConfigFile::from_runtime),
            ender_pearls: data
                .ender_pearls
                .iter()
                .map(|pearl| EnderPearlFile {
                    world: pearl.world.clone(),
                    entity: pearl.entity.clone(),
                })
                .collect(),
            advancements: data
                .advancements
                .iter()
                .map(|advancement| AdvancementFile {
                    key: advancement.key.clone(),
                    criteria: advancement
                        .criteria
                        .iter()
                        .map(|criterion| CriterionFile {
                            name: criterion.name.clone(),
                            obtained_epoch_millis: criterion.obtained_epoch_millis,
                        })
                        .collect(),
                })
                .collect(),
            statistics: data
                .statistics
                .iter()
                .map(|statistic| StatisticFile {
                    stat_type: statistic.stat_type.clone(),
                    value: statistic.value.clone(),
                    count: statistic.count,
                })
                .collect(),
            living_nbt: data.living_nbt.clone(),
        };
        file.validate_finite_values()?;
        Ok(file)
    }

    fn into_persistent(self) -> io::Result<PersistentPlayerData> {
        if self.data_version != PLAYER_DATA_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported player data payload version {}",
                    self.data_version
                ),
            ));
        }
        self.validate_finite_values()?;

        let mut ender_items = Vec::with_capacity(self.ender_items.len());
        for slot in self.ender_items {
            ender_items.push(PersistentSlot {
                slot: slot.slot,
                item: item_from_nbt_bytes(&slot.item_nbt)?,
            });
        }

        let mut inventory = Vec::with_capacity(self.inventory.len());
        for slot in self.inventory {
            inventory.push(PersistentSlot {
                slot: slot.slot,
                item: item_from_nbt_bytes(&slot.item_nbt)?,
            });
        }

        let advancements = into_persistent_advancements(self.advancements);
        let statistics = into_persistent_statistics(self.statistics);

        Ok(PersistentPlayerData {
            pos: self.pos,
            motion: self.motion,
            rotation: self.rotation,
            on_ground: self.on_ground,
            fall_flying: self.fall_flying,
            remaining_fire_ticks: self.remaining_fire_ticks,
            ticks_frozen: self.ticks_frozen,
            is_in_powder_snow: self.is_in_powder_snow,
            was_in_powder_snow: self.was_in_powder_snow,
            has_visual_fire: self.has_visual_fire,
            health: self.health,
            game_mode: self.game_mode,
            prev_game_mode: self.prev_game_mode,
            abilities: PersistentAbilities {
                invulnerable: self.abilities.invulnerable,
                flying: self.abilities.flying,
                may_fly: self.abilities.may_fly,
                instabuild: self.abilities.instabuild,
                may_build: self.abilities.may_build,
                flying_speed: self.abilities.flying_speed,
                walking_speed: self.abilities.walking_speed,
            },
            inventory,
            ender_items,
            selected_slot: self.selected_slot,
            world: self.world,
            food_level: self.food_level,
            food_saturation_level: self.food_saturation_level,
            food_exhaustion_level: self.food_exhaustion_level,
            food_tick_timer: self.food_tick_timer,
            data_version: self.data_version,
            experience_level: self.experience_level,
            experience_progress: self.experience_progress,
            experience_total: self.experience_total,
            enchantment_seed: self.enchantment_seed,
            score: self.score,
            seen_credits: self.seen_credits,
            warden_spawn_tracker: self.warden_spawn_tracker,
            root_vehicle: self.root_vehicle.map(|root_vehicle| PersistentRootVehicle {
                attach: root_vehicle.attach,
                entity: root_vehicle.entity,
            }),
            respawn_config: self
                .respawn_config
                .map(RespawnConfigFile::into_runtime)
                .transpose()?,
            ender_pearls: self
                .ender_pearls
                .into_iter()
                .map(|pearl| PersistentEnderPearl {
                    world: pearl.world,
                    entity: pearl.entity,
                })
                .collect(),
            advancements,
            statistics,
            living_nbt: self.living_nbt,
        })
    }

    fn validate_finite_values(&self) -> io::Result<()> {
        validate_finite_f64("position", &self.pos)?;
        validate_finite_f64("motion", &self.motion)?;
        validate_finite_f32("rotation", &self.rotation)?;
        validate_finite_f32("health", &[self.health])?;
        validate_finite_f32(
            "ability speeds",
            &[self.abilities.flying_speed, self.abilities.walking_speed],
        )?;
        validate_finite_f32(
            "food state",
            &[self.food_saturation_level, self.food_exhaustion_level],
        )?;
        validate_finite_f32("experience progress", &[self.experience_progress])?;
        if let Some(respawn) = &self.respawn_config {
            validate_finite_f32("respawn rotation", &[respawn.yaw, respawn.pitch])?;
        }
        if let Some(root_vehicle) = &self.root_vehicle {
            validate_persistent_entity_finite_values(&root_vehicle.entity)?;
        }
        for pearl in &self.ender_pearls {
            validate_persistent_entity_finite_values(&pearl.entity)?;
        }
        Ok(())
    }
}

fn validate_finite_f64(field: &str, values: &[f64]) -> io::Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("player data {field} contains a non-finite value"),
    ))
}

fn validate_finite_f32(field: &str, values: &[f32]) -> io::Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("player data {field} contains a non-finite value"),
    ))
}

fn validate_persistent_entity_finite_values(root: &PersistentEntity) -> io::Result<()> {
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        validate_finite_f64("persisted entity position", &entity.pos)?;
        validate_finite_f64("persisted entity motion", &entity.motion)?;
        validate_finite_f32("persisted entity rotation", &entity.rotation)?;
        validate_finite_f64("persisted entity fall distance", &[entity.fall_distance])?;
        pending.extend(entity.passengers.iter().map(|passenger| &passenger.0));
    }
    Ok(())
}

/// Vanilla parity: nothing -- these are only here to keep `into_persistent`
/// short enough to read.
fn into_persistent_advancements(advancements: Vec<AdvancementFile>) -> Vec<PersistentAdvancement> {
    advancements
        .into_iter()
        .map(|advancement| PersistentAdvancement {
            key: advancement.key,
            criteria: advancement
                .criteria
                .into_iter()
                .map(|criterion| PersistentCriterion {
                    name: criterion.name,
                    obtained_epoch_millis: criterion.obtained_epoch_millis,
                })
                .collect(),
        })
        .collect()
}

fn into_persistent_statistics(statistics: Vec<StatisticFile>) -> Vec<PersistentStatistic> {
    statistics
        .into_iter()
        .map(|statistic| PersistentStatistic {
            stat_type: statistic.stat_type,
            value: statistic.value,
            count: statistic.count,
        })
        .collect()
}

impl RespawnConfigFile {
    fn from_runtime(config: PlayerRespawnConfig) -> Self {
        let pos = config.respawn_data.pos();
        Self {
            dimension: config.respawn_data.dimension().to_string(),
            pos: [pos.x(), pos.y(), pos.z()],
            yaw: config.respawn_data.yaw,
            pitch: config.respawn_data.pitch,
            forced: config.forced,
        }
    }

    fn into_runtime(self) -> io::Result<PlayerRespawnConfig> {
        Ok(PlayerRespawnConfig {
            respawn_data: RespawnData::of(
                self.dimension.parse().map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid respawn dimension: {error}"),
                    )
                })?,
                BlockPos::new(self.pos[0], self.pos[1], self.pos[2]),
                self.yaw,
                self.pitch,
            ),
            forced: self.forced,
        })
    }
}

fn item_to_nbt_bytes(item: &ItemStack) -> io::Result<Vec<u8>> {
    let NbtTag::Compound(compound) = item.clone().to_nbt_tag() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "item stack did not serialize to a compound",
        ));
    };
    let mut bytes = Vec::new();
    compound.write(&mut bytes);
    Ok(bytes)
}

fn item_from_nbt_bytes(bytes: &[u8]) -> io::Result<ItemStack> {
    let nbt = read_borrowed_compound(&mut Cursor::new(bytes)).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse item NBT: {e}"),
        )
    })?;
    let compound = simdnbt::borrow::NbtCompound::from(&nbt);
    ItemStack::from_borrowed_compound(&compound)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid item stack data"))
}

fn encode_player_file(file: &PlayerDataFile) -> io::Result<Vec<u8>> {
    encode_file(
        PLAYER_MAGIC,
        PLAYER_STORAGE_VERSION,
        wincode::serialize(file),
    )
}

fn decode_player_file(bytes: &[u8]) -> io::Result<PlayerDataFile> {
    let payload = decode_file(PLAYER_MAGIC, PLAYER_STORAGE_VERSION, bytes)?;
    wincode::deserialize(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

fn encode_global_file(file: &GlobalPlayerDataFile) -> io::Result<Vec<u8>> {
    encode_file(
        GLOBAL_MAGIC,
        GLOBAL_STORAGE_VERSION,
        wincode::serialize(file),
    )
}

fn decode_global_file(bytes: &[u8]) -> io::Result<GlobalPlayerDataFile> {
    if bytes.len() < 6 || bytes[0..4] != GLOBAL_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid global player data header",
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    let payload = decompress_player_data(&bytes[6..])?;
    if version == 1 {
        let legacy: LegacyGlobalPlayerDataFile = wincode::deserialize(&payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        return validate_global_player_data_file(GlobalPlayerDataFile {
            data_version: GLOBAL_PLAYER_DATA_VERSION,
            last_active_domain: legacy.last_active_domain,
            first_played: 0,
            last_played: 0,
            whitelisted: false,
        });
    }
    if version == 2 {
        let legacy: LegacyGlobalPlayerDataFileV2 = wincode::deserialize(&payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        return validate_global_player_data_file(GlobalPlayerDataFile {
            data_version: GLOBAL_PLAYER_DATA_VERSION,
            last_active_domain: legacy.last_active_domain,
            first_played: legacy.first_played,
            last_played: legacy.last_played,
            whitelisted: false,
        });
    }
    if version != GLOBAL_STORAGE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported global player data storage version {version}"),
        ));
    }
    let file = wincode::deserialize(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    validate_global_player_data_file(file)
}

fn validate_global_player_data_file(
    file: GlobalPlayerDataFile,
) -> io::Result<GlobalPlayerDataFile> {
    validate_domain_name(&file.last_active_domain, true, io::ErrorKind::InvalidData)?;
    Ok(file)
}

fn validate_domain_name(
    domain: &str,
    allow_empty: bool,
    error_kind: io::ErrorKind,
) -> io::Result<()> {
    if allow_empty && domain.is_empty() {
        return Ok(());
    }
    let mut components = Path::new(domain).components();
    let is_single_normal_component = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(component)), None) if component == domain
    );
    if domain.is_empty()
        || domain == "."
        || domain == ".."
        || domain == "global"
        || domain.ends_with(['.', ' '])
        || !is_single_normal_component
        || !Identifier::validate_namespace(domain)
    {
        return Err(io::Error::new(
            error_kind,
            format!("invalid player data domain name {domain:?}"),
        ));
    }
    Ok(())
}

fn unix_epoch_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(i64::MAX as u128) as i64
        })
}

fn encode_file(
    magic: [u8; 4],
    version: u16,
    serialized: wincode::WriteResult<Vec<u8>>,
) -> io::Result<Vec<u8>> {
    let payload =
        serialized.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let compressed = zstd::encode_all(&payload[..], 3)?;
    let mut bytes = Vec::with_capacity(6 + compressed.len());
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&compressed);
    Ok(bytes)
}

fn decode_file(
    expected_magic: [u8; 4],
    expected_version: u16,
    bytes: &[u8],
) -> io::Result<Vec<u8>> {
    if bytes.len() < 6 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "player data file is too short",
        ));
    }
    if bytes[0..4] != expected_magic {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid player data magic",
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != expected_version {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported player data storage version {version}"),
        ));
    }
    decompress_player_data(&bytes[6..])
}

fn decompress_player_data(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let decoder = zstd::stream::read::Decoder::new(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("player data zstd payload is invalid: {error}"),
        )
    })?;
    let mut bytes = Vec::with_capacity(PLAYER_DATA_DECOMPRESSION_BUFFER_BYTES);
    decoder
        .take((MAX_DECOMPRESSED_PLAYER_DATA_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("player data zstd payload is invalid: {error}"),
            )
        })?;
    if bytes.len() > MAX_DECOMPRESSED_PLAYER_DATA_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "player data zstd payload exceeds the {MAX_DECOMPRESSED_PLAYER_DATA_BYTES} byte limit"
            ),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::DEFAULT_MAX_AIR_SUPPLY;
    use crate::permission::PermissionSet;
    use crate::player::KnownPlayer;
    use foton_registry::{init_vanilla_registry, vanilla_items};
    use simdnbt::owned::NbtCompound;
    use std::{
        env,
        io::repeat,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use tokio::{sync::oneshot, time::timeout};

    fn temp_storage_root(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        env::temp_dir().join(format!("fotonmc-player-storage-{name}-{suffix}"))
    }

    /// An ender chest's contents survive the save format.
    ///
    /// This is the half of the block a running server cannot show: the client
    /// cannot click inventory slots and Foton has no `/item` command, so the
    /// only way to prove the contents belong to the player rather than to the
    /// block is to write them out and read them back.
    #[test]
    fn ender_chest_items_survive_a_save_and_load() {
        init_vanilla_registry();

        let mut data = sample_player_file(PLAYER_DATA_VERSION);
        data.ender_items = vec![SlotFile {
            slot: 5,
            item_nbt: item_to_nbt_bytes(&ItemStack::with_count(&vanilla_items::DIAMOND, 7))
                .expect("a diamond encodes"),
        }];

        let restored = data
            .into_persistent()
            .expect("the file reads back at the current version");

        assert_eq!(restored.ender_items.len(), 1, "the slot was lost");
        let slot = &restored.ender_items[0];
        assert_eq!(slot.slot, 5, "the slot index moved");
        assert!(slot.item.is(&vanilla_items::DIAMOND), "the item changed");
        assert_eq!(slot.item.count(), 7, "the stack changed size");
    }

    /// The living half of a player's save survives the file format.
    ///
    /// Absorption, potion effects and attribute modifiers live in this one
    /// field and nowhere else in the file, so a field quietly dropped from the
    /// format costs a player all three and nothing else notices.
    #[test]
    fn the_living_half_survives_the_player_file() {
        let mut living = NbtCompound::new();
        living.insert("AbsorptionAmount", 6.0_f32);
        let mut living_bytes = Vec::new();
        living.write(&mut living_bytes);

        let mut data = sample_player_file(PLAYER_DATA_VERSION);
        data.living_nbt = living_bytes.clone();

        let encoded = encode_player_file(&data).expect("player file should encode");
        let decoded = decode_player_file(&encoded).expect("player file should decode");
        let restored = decoded
            .into_persistent()
            .expect("the file reads back at the current version");

        assert_eq!(restored.living_nbt, living_bytes);
    }

    fn sample_player_file(data_version: i32) -> PlayerDataFile {
        PlayerDataFile {
            data_version,
            pos: [1.0, 2.0, 3.0],
            motion: [0.0, 0.0, 0.0],
            rotation: [90.0, 10.0],
            on_ground: true,
            fall_flying: false,
            remaining_fire_ticks: 0,
            ticks_frozen: 0,
            is_in_powder_snow: false,
            was_in_powder_snow: false,
            has_visual_fire: false,
            health: 20.0,
            game_mode: 2,
            prev_game_mode: Some(0),
            abilities: AbilitiesFile {
                invulnerable: false,
                flying: false,
                may_fly: false,
                instabuild: false,
                may_build: true,
                flying_speed: 0.05,
                walking_speed: 0.1,
            },
            inventory: Vec::new(),
            ender_items: Vec::new(),
            selected_slot: 4,
            world: "lobby:void".to_owned(),
            food_level: 20,
            food_saturation_level: 5.0,
            food_exhaustion_level: 0.0,
            food_tick_timer: 0,
            experience_level: 7,
            experience_progress: 0.5,
            experience_total: 32,
            enchantment_seed: 4242,
            score: 9,
            seen_credits: true,
            warden_spawn_tracker: [0, 0, 0],
            root_vehicle: None,
            respawn_config: None,
            ender_pearls: Vec::new(),
            advancements: vec![AdvancementFile {
                key: "minecraft:story/root".to_owned(),
                criteria: vec![CriterionFile {
                    name: "crafting_table".to_owned(),
                    obtained_epoch_millis: 1_234,
                }],
            }],
            statistics: vec![StatisticFile {
                stat_type: "minecraft:custom".to_owned(),
                value: "minecraft:jump".to_owned(),
                count: 12,
            }],
            living_nbt: Vec::new(),
        }
    }

    fn sample_persistent_entity() -> PersistentEntity {
        PersistentEntity {
            entity_type: Identifier::vanilla_static("minecart"),
            uuid: [7; 16],
            pos: [4.0, 65.0, 6.0],
            motion: [0.0, 0.0, 0.0],
            rotation: [45.0, 0.0],
            fall_distance: 0.0,
            remaining_fire_ticks: 0,
            ticks_frozen: 0,
            is_in_powder_snow: false,
            was_in_powder_snow: false,
            has_visual_fire: false,
            on_ground: true,
            no_gravity: false,
            invulnerable: false,
            air_supply: DEFAULT_MAX_AIR_SUPPLY,
            portal_cooldown: 0,
            custom_name_nbt: Vec::new(),
            custom_name_visible: false,
            silent: false,
            glowing: false,
            tags: Vec::new(),
            custom_data_nbt: Vec::new(),
            nbt_data: Vec::new(),
            passengers: Vec::new(),
        }
    }

    #[tokio::test]
    async fn atomic_path_replacement_retains_the_last_committed_generation() {
        let root = temp_storage_root("atomic-replacement");
        let path = root.join("state.dat");

        FilePlayerDataStorage::write_atomic_path_locked(&path, b"first".to_vec())
            .await
            .expect("first generation should publish");
        FilePlayerDataStorage::write_atomic_path_locked(&path, b"second".to_vec())
            .await
            .expect("second generation should publish");

        assert_eq!(
            fs::read(&path).await.expect("live file should be readable"),
            b"second"
        );
        assert_eq!(
            fs::read(FilePlayerDataStorage::atomic_backup_path(&path))
                .await
                .expect("backup should be readable"),
            b"first"
        );

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn missing_permission_primary_does_not_restore_a_privileged_backup() {
        let root = temp_storage_root("permission-recovery");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let mut subjects = PermissionSubjectIndex::new();
        for (uuid, group) in [
            (Uuid::from_u128(10), "builder"),
            (Uuid::from_u128(20), "moderator"),
        ] {
            subjects.set(
                uuid,
                PermissionSubjectState::new(vec![group.to_owned()], PermissionSet::new()),
            );
        }
        storage
            .save_permission_subjects(&subjects)
            .await
            .expect("permission subjects should persist");

        let path = storage.player_permissions_file();
        let backup = FilePlayerDataStorage::atomic_backup_path(&path);
        let temporary = FilePlayerDataStorage::atomic_temp_path(&path);
        fs::rename(&path, &backup)
            .await
            .expect("legacy publication should reach its interrupted state");
        fs::write(&temporary, b"uncommitted replacement")
            .await
            .expect("uncommitted replacement should be staged");

        let mut recovered = storage
            .load_permission_subjects()
            .await
            .expect("a missing primary should fail closed");
        assert!(recovered.is_empty());
        assert!(
            backup.exists(),
            "the backup should remain for operator recovery"
        );
        assert!(!temporary.exists());

        recovered.set(
            Uuid::from_u128(30),
            PermissionSubjectState::new(vec!["operator".to_owned()], PermissionSet::new()),
        );
        storage
            .save_permission_subjects(&recovered)
            .await
            .expect("an update after recovery should preserve existing subjects");
        let updated = storage
            .load_permission_subjects()
            .await
            .expect("updated permissions should load");
        assert_eq!(updated.len(), 1);

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn corrupt_live_permission_file_fails_closed_despite_a_valid_backup() {
        let root = temp_storage_root("corrupt-live-permissions");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let mut subjects = PermissionSubjectIndex::new();
        subjects.set(
            Uuid::from_u128(42),
            PermissionSubjectState::new(vec!["op".to_owned()], PermissionSet::new()),
        );
        storage
            .save_permission_subjects(&subjects)
            .await
            .expect("permission subject should persist");
        let path = storage.player_permissions_file();
        let backup = FilePlayerDataStorage::atomic_backup_path(&path);
        fs::copy(&path, &backup)
            .await
            .expect("valid backup should be staged");
        fs::write(&path, b"not valid permission TOML")
            .await
            .expect("live permission file should be corrupted for the test");

        let error = storage
            .load_permission_subjects()
            .await
            .expect_err("a stale permission backup must not restore revoked privileges");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read_to_string(&path)
                .await
                .expect("corrupt live file should remain in place"),
            "not valid permission TOML"
        );

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn operational_read_error_does_not_fall_back_to_stale_data() {
        let root = temp_storage_root("operational-read-error");
        let path = root.join("state.dat");
        let backup = FilePlayerDataStorage::atomic_backup_path(&path);
        fs::create_dir_all(&root)
            .await
            .expect("temporary storage should initialize");
        fs::write(&path, b"primary")
            .await
            .expect("primary should be staged");
        fs::write(&backup, b"backup")
            .await
            .expect("backup should be staged");

        let error = FilePlayerDataStorage::read_with_valid_backup(&path, |bytes| {
            if bytes == b"primary" {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "simulated operational failure",
                ))
            } else {
                Ok(bytes.to_vec())
            }
        })
        .await
        .expect_err("an operational failure must not be hidden by stale data");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn interrupted_first_known_player_publication_discards_its_temporary_file() {
        let root = temp_storage_root("known-player-interrupted-first-write");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let uuid = Uuid::from_u128(42);
        let players =
            KnownPlayers::from_entries([KnownPlayer::with_expiration(uuid, "Steve", 1_234_567)]);
        let path = storage.known_players_file();
        let temporary = FilePlayerDataStorage::atomic_temp_path(&path);
        let bytes = encode_known_players_file(&KnownPlayersFile::from_known_players(&players))
            .expect("known players should encode");
        fs::write(&temporary, bytes)
            .await
            .expect("first publication should reach its interrupted state");

        let loaded = storage
            .load_known_players()
            .await
            .expect("uncommitted known-player state should be ignored");
        assert!(loaded.entries().is_empty());
        assert!(!path.exists());
        assert!(!temporary.exists());

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn corrupt_known_player_cache_loads_as_empty() {
        let root = temp_storage_root("corrupt-known-players");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let path = storage.known_players_file();
        fs::write(&path, b"not a known-player cache")
            .await
            .expect("known-player cache should be corrupted for the test");

        let loaded = storage
            .load_known_players()
            .await
            .expect("a corrupt optional cache should not prevent startup");
        assert!(loaded.entries().is_empty());
        assert_eq!(
            fs::read(&path)
                .await
                .expect("the corrupt cache should remain available for diagnosis"),
            b"not a known-player cache"
        );

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn incompatible_known_player_cache_version_loads_as_empty() {
        let root = temp_storage_root("incompatible-known-player-version");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let uuid = Uuid::from_u128(42);
        let players = KnownPlayers::from_entries([KnownPlayer::new(uuid, "Steve")]);
        let mut bytes = encode_known_players_file(&KnownPlayersFile::from_known_players(&players))
            .expect("known-player cache should encode");
        bytes[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
        fs::write(storage.known_players_file(), bytes)
            .await
            .expect("incompatible known-player cache should be seeded");

        let loaded = storage
            .load_known_players()
            .await
            .expect("an incompatible optional cache should not prevent startup");
        assert!(loaded.entries().is_empty());
        assert!(loaded.by_uuid(uuid).is_none());

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn interrupted_first_permission_publication_does_not_apply_uncommitted_access() {
        let root = temp_storage_root("permission-interrupted-first-write");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let path = storage.player_permissions_file();
        let temporary = FilePlayerDataStorage::atomic_temp_path(&path);
        let mut file = PlayerPermissionsFile::default();
        set_permission_subject(
            &mut file,
            Uuid::from_u128(42),
            &PermissionSubjectState::new(vec!["op".to_owned()], PermissionSet::new()),
        );
        let contents = serialize_player_permissions_file(&file)
            .expect("uncommitted permissions should serialize");
        fs::write(&temporary, contents)
            .await
            .expect("uncommitted permissions should be staged");

        let loaded = storage
            .load_permission_subjects()
            .await
            .expect("uncommitted permissions should be ignored");
        assert!(loaded.is_empty());
        assert!(!path.exists());
        assert!(!temporary.exists());

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn known_player_cache_round_trips_and_rejects_stale_writes() {
        let root = temp_storage_root("known-players");
        let storage = match FilePlayerDataStorage::new(root.clone()).await {
            Ok(storage) => storage,
            Err(error) => panic!("test storage should initialize: {error}"),
        };
        let uuid = Uuid::from_u128(42);
        let players =
            KnownPlayers::from_entries([KnownPlayer::with_expiration(uuid, "Steve", 1_234_567)]);

        let stale = storage
            .save_known_players_if_current(&players, || false)
            .await;
        assert!(matches!(stale, Ok(false)));
        assert!(!storage.known_players_file().exists());

        let saved = storage
            .save_known_players_if_current(&players, || true)
            .await;
        assert!(matches!(saved, Ok(true)));
        let loaded = storage.load_known_players().await;
        let Ok(loaded) = loaded else {
            panic!("known players should load");
        };
        assert_eq!(
            loaded.by_uuid(uuid).map(KnownPlayer::last_known_name),
            Some("Steve")
        );
        assert_eq!(
            loaded.by_uuid(uuid).map(KnownPlayer::expires_at_millis),
            Some(1_234_567)
        );

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[test]
    fn known_player_cache_persists_vanillas_mru_limit() {
        let players = KnownPlayers::from_entries((0_u128..=1_000).map(|value| {
            KnownPlayer::with_expiration(
                Uuid::from_u128(value),
                format!("Player{value}"),
                1_234_567,
            )
        }));
        let encoded = encode_known_players_file(&KnownPlayersFile::from_known_players(&players));
        let Ok(encoded) = encoded else {
            panic!("known player cache should encode");
        };
        let decoded =
            decode_known_players_file(&encoded).and_then(KnownPlayersFile::into_known_players);
        let Ok(decoded) = decoded else {
            panic!("known player cache should decode");
        };

        assert_eq!(decoded.entries().len(), 1_000);
        assert!(decoded.by_uuid(Uuid::from_u128(999)).is_some());
        assert!(decoded.by_uuid(Uuid::from_u128(1_000)).is_none());
    }

    #[test]
    fn player_file_roundtrip_preserves_domain_world_data() {
        let file = sample_player_file(PLAYER_DATA_VERSION);

        let encoded = encode_player_file(&file).expect("player file should encode");
        let decoded = decode_player_file(&encoded).expect("player file should decode");

        assert_eq!(
            u16::from_le_bytes([encoded[4], encoded[5]]),
            PLAYER_STORAGE_VERSION
        );
        assert_eq!(decoded.world, "lobby:void");
        assert_eq!(decoded.game_mode, 2);
        assert_eq!(decoded.selected_slot, 4);
        assert_eq!(decoded.experience_level, 7);
        assert_eq!(decoded.experience_progress.to_bits(), 0.5_f32.to_bits());
        assert_eq!(decoded.experience_total, 32);
        assert_eq!(decoded.enchantment_seed, 4242);
        assert_eq!(decoded.score, 9);
        assert!(decoded.seen_credits);

        // Advancements are the reason a relog does not start the tree over.
        assert_eq!(decoded.advancements.len(), 1);
        assert_eq!(decoded.advancements[0].key, "minecraft:story/root");
        assert_eq!(decoded.advancements[0].criteria.len(), 1);
        assert_eq!(decoded.advancements[0].criteria[0].name, "crafting_table");
        assert_eq!(
            decoded.advancements[0].criteria[0].obtained_epoch_millis,
            1_234
        );

        // Statistics are keyed by name, not by registry id: an id only means
        // something against the registry that handed it out.
        assert_eq!(decoded.statistics.len(), 1);
        assert_eq!(decoded.statistics[0].stat_type, "minecraft:custom");
        assert_eq!(decoded.statistics[0].value, "minecraft:jump");
        assert_eq!(decoded.statistics[0].count, 12);
    }

    #[test]
    fn player_file_roundtrip_preserves_absent_previous_game_mode() {
        let mut file = sample_player_file(PLAYER_DATA_VERSION);
        file.prev_game_mode = None;

        let encoded = encode_player_file(&file).expect("player file should encode");
        let decoded = decode_player_file(&encoded).expect("player file should decode");
        let persistent = decoded
            .into_persistent()
            .expect("player file should convert");

        assert_eq!(persistent.prev_game_mode, None);
    }

    #[test]
    fn global_file_roundtrip_preserves_last_active_domain() {
        let file = GlobalPlayerDataFile {
            data_version: GLOBAL_PLAYER_DATA_VERSION,
            last_active_domain: "minecraft".to_owned(),
            first_played: 1_000,
            last_played: 2_000,
            whitelisted: true,
        };

        let encoded = encode_global_file(&file).expect("global file should encode");
        let decoded = decode_global_file(&encoded).expect("global file should decode");

        assert_eq!(
            u16::from_le_bytes([encoded[4], encoded[5]]),
            GLOBAL_STORAGE_VERSION
        );
        assert_eq!(decoded.last_active_domain, "minecraft");
        assert!(decoded.whitelisted);
    }

    #[tokio::test]
    async fn recovered_global_backup_clears_whitelist_and_repairs_the_primary() {
        let root = temp_storage_root("global-backup-whitelist");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let uuid = Uuid::from_u128(42);
        let path = FilePlayerDataStorage::player_file(&storage.global_players_dir(), uuid);
        let backup = FilePlayerDataStorage::atomic_backup_path(&path);
        let privileged = GlobalPlayerDataFile {
            data_version: GLOBAL_PLAYER_DATA_VERSION,
            last_active_domain: "minecraft".to_owned(),
            first_played: 1_000,
            last_played: 2_000,
            whitelisted: true,
        };
        fs::write(
            &backup,
            encode_global_file(&privileged).expect("backup should encode"),
        )
        .await
        .expect("backup should be staged");

        let recovered = storage
            .load_global(uuid)
            .await
            .expect("backup recovery should succeed")
            .expect("backup should provide player data");
        assert!(!recovered.whitelisted);
        let repaired = decode_global_file(
            &fs::read(&path)
                .await
                .expect("repaired primary should be readable"),
        )
        .expect("repaired primary should decode");
        assert!(!repaired.whitelisted);

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn stale_gameplay_save_cannot_overwrite_whitelist_on_disk() {
        let root = temp_storage_root("global-domain-whitelist-isolation");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let uuid = Uuid::from_u128(42);
        let stale = GlobalPlayerData {
            last_active_domain: "old".to_owned(),
            first_played: 1_000,
            last_played: 2_000,
            statistics: Vec::new(),
            whitelisted: false,
        };
        storage
            .save_global(uuid, &stale)
            .await
            .expect("initial gameplay data should persist");
        let (published, published_rx) = mpsc::sync_channel(1);
        storage
            .set_player_whitelisted(
                uuid,
                true,
                &stale,
                || true,
                move |data| {
                    published
                        .send(data)
                        .expect("publication receiver should remain open");
                },
            )
            .await
            .expect("whitelist update should persist");
        let whitelisted = published_rx
            .recv()
            .expect("whitelist update should publish");
        assert!(whitelisted.whitelisted);

        let mut newer_stale_snapshot = stale;
        newer_stale_snapshot.last_active_domain = "new".to_owned();
        newer_stale_snapshot.last_played = 3_000;
        storage
            .save_global(uuid, &newer_stale_snapshot)
            .await
            .expect("gameplay update should persist");
        let loaded = storage
            .load_global(uuid)
            .await
            .expect("global data should load")
            .expect("global data should exist");
        assert_eq!(loaded.last_active_domain, "new");
        assert_eq!(loaded.last_played, 3_000);
        assert!(loaded.whitelisted);

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn committed_whitelist_is_published_when_backup_rotation_fails() {
        let root = temp_storage_root("whitelist-committed-warning");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let uuid = Uuid::from_u128(43);
        let fallback = GlobalPlayerData {
            last_active_domain: "minecraft".to_owned(),
            first_played: 1_000,
            last_played: 2_000,
            statistics: Vec::new(),
            whitelisted: false,
        };
        storage
            .save_global(uuid, &fallback)
            .await
            .expect("initial global data should persist");

        let path = FilePlayerDataStorage::player_file(&storage.global_players_dir(), uuid);
        fs::create_dir(FilePlayerDataStorage::atomic_backup_path(&path))
            .await
            .expect("a directory should obstruct only backup rotation");
        let published = Arc::new(AtomicBool::new(false));
        let published_by_callback = Arc::clone(&published);

        let outcome = storage
            .set_player_whitelisted(
                uuid,
                true,
                &fallback,
                || true,
                move |data| {
                    published_by_callback.store(data.whitelisted, Ordering::SeqCst);
                },
            )
            .await
            .expect("the primary replacement itself should commit");

        assert!(matches!(
            outcome,
            Some(PersistenceUpdateOutcome::CommittedWithError(_))
        ));
        assert!(
            published.load(Ordering::SeqCst),
            "a visible primary must also become visible to synchronous admission checks"
        );
        let persisted = storage
            .load_global(uuid)
            .await
            .expect("committed global data should load")
            .expect("committed global data should exist");
        assert!(persisted.whitelisted);

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn whitelist_disk_and_cache_publication_are_ordered_together() {
        let root = temp_storage_root("whitelist-publication-order");
        let storage = Arc::new(
            FilePlayerDataStorage::new(root.clone())
                .await
                .expect("test storage should initialize"),
        );
        let uuid = Uuid::from_u128(42);
        let fallback = GlobalPlayerData {
            last_active_domain: "minecraft".to_owned(),
            first_played: 1_000,
            last_played: 2_000,
            statistics: Vec::new(),
            whitelisted: false,
        };
        let cached = Arc::new(AtomicBool::new(false));
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = mpsc::sync_channel(0);

        let first_storage = Arc::clone(&storage);
        let first_cached = Arc::clone(&cached);
        let first_fallback = fallback.clone();
        let first = tokio::spawn(async move {
            first_storage
                .set_player_whitelisted(
                    uuid,
                    true,
                    &first_fallback,
                    || true,
                    move |_| {
                        let _ = entered_tx.send(());
                        release_rx
                            .recv()
                            .expect("the first publication should be released");
                        first_cached.store(true, Ordering::SeqCst);
                    },
                )
                .await
        });
        entered_rx
            .await
            .expect("the first update should reach cache publication");

        let second_storage = Arc::clone(&storage);
        let second_cached = Arc::clone(&cached);
        let (second_published_tx, second_published_rx) = oneshot::channel();
        let second = tokio::spawn(async move {
            second_storage
                .set_player_whitelisted(
                    uuid,
                    false,
                    &fallback,
                    || true,
                    move |_| {
                        second_cached.store(false, Ordering::SeqCst);
                        let _ = second_published_tx.send(());
                    },
                )
                .await
        });

        assert!(
            timeout(Duration::from_millis(100), second_published_rx)
                .await
                .is_err(),
            "a later disk write must not pass an earlier cache publication"
        );
        release_tx
            .send(())
            .expect("the first publication should still be waiting");
        first
            .await
            .expect("first update task should finish")
            .expect("first update should succeed");
        second
            .await
            .expect("second update task should finish")
            .expect("second update should succeed");

        let persisted = storage
            .load_global(uuid)
            .await
            .expect("global data should load")
            .expect("global data should exist");
        assert!(!persisted.whitelisted);
        assert!(!cached.load(Ordering::SeqCst));

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn permission_subject_snapshot_removes_noncanonical_uuid_key() {
        let root = temp_storage_root("permission-uuid-key");
        let storage = match FilePlayerDataStorage::new(root.clone()).await {
            Ok(storage) => storage,
            Err(error) => panic!("test storage should initialize: {error}"),
        };
        let target_uuid = Uuid::from_u128(42);
        let control_uuid = Uuid::from_u128(84);
        let mut seed = PermissionSubjectIndex::new();
        seed.set(
            target_uuid,
            PermissionSubjectState::new(vec!["op".to_owned()], PermissionSet::new()),
        );
        seed.set(
            control_uuid,
            PermissionSubjectState::new(vec!["builder".to_owned()], PermissionSet::new()),
        );
        let file = PlayerPermissionsFile::from_subject_index(&seed);
        let canonical = target_uuid.to_string();
        let noncanonical = target_uuid.simple().to_string();
        let contents = serialize_player_permissions_file(&file)
            .expect("permission subjects should serialize")
            .replace(&canonical, &noncanonical);
        fs::write(storage.player_permissions_file(), contents)
            .await
            .expect("noncanonical permission UUID should be seeded");

        let mut subjects = storage
            .load_permission_subjects()
            .await
            .expect("valid UUID spellings should load");
        assert_eq!(subjects.len(), 2);
        let removed = subjects
            .remove(target_uuid)
            .expect("target should be indexed by UUID");
        assert_eq!(removed.groups(), ["op"]);
        storage
            .save_permission_subjects(&subjects)
            .await
            .expect("updated UUID index should persist");

        let reloaded = storage
            .load_permission_subjects()
            .await
            .expect("updated permission subjects should load");
        assert!(reloaded.get(target_uuid).is_none());
        assert_eq!(
            reloaded
                .get(control_uuid)
                .map(PermissionSubjectState::groups),
            Some(["builder".to_owned()].as_slice())
        );
        let persisted = fs::read_to_string(storage.player_permissions_file())
            .await
            .expect("updated permissions should be readable");
        assert!(!persisted.contains(&canonical));
        assert!(!persisted.contains(&noncanonical));

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[test]
    fn player_file_roundtrip_preserves_root_vehicle() {
        let mut file = sample_player_file(PLAYER_DATA_VERSION);
        file.root_vehicle = Some(RootVehicleFile {
            attach: [3; 16],
            entity: sample_persistent_entity(),
        });

        let encoded = encode_player_file(&file).expect("player file should encode");
        let decoded = decode_player_file(&encoded).expect("player file should decode");
        let persistent = decoded
            .into_persistent()
            .expect("player file should convert");

        let Some(root_vehicle) = persistent.root_vehicle else {
            panic!("root vehicle should survive roundtrip");
        };
        assert_eq!(root_vehicle.attach, [3; 16]);
        assert_eq!(root_vehicle.entity.uuid, [7; 16]);
        assert_eq!(
            root_vehicle.entity.entity_type,
            Identifier::vanilla_static("minecart")
        );
        assert_eq!(
            root_vehicle.entity.pos.map(f64::to_bits),
            [4.0_f64.to_bits(), 65.0_f64.to_bits(), 6.0_f64.to_bits()]
        );
    }

    #[test]
    fn player_file_roundtrip_preserves_respawn_config() {
        let mut file = sample_player_file(PLAYER_DATA_VERSION);
        file.respawn_config = Some(RespawnConfigFile {
            dimension: "minecraft:overworld".to_owned(),
            pos: [10, 64, -3],
            yaw: 181.0,
            pitch: -120.0,
            forced: false,
        });
        file.ender_pearls = vec![
            EnderPearlFile {
                world: "minecraft:overworld".to_owned(),
                entity: sample_persistent_entity(),
            },
            EnderPearlFile {
                world: "minecraft:the_nether".to_owned(),
                entity: sample_persistent_entity(),
            },
        ];

        let encoded = encode_player_file(&file).expect("player file should encode");
        let decoded = decode_player_file(&encoded).expect("player file should decode");
        let persistent = decoded
            .into_persistent()
            .expect("player file should convert");

        let Some(respawn_config) = persistent.respawn_config else {
            panic!("respawn config should survive roundtrip");
        };
        assert_eq!(
            respawn_config.respawn_data.dimension(),
            &Identifier::vanilla_static("overworld")
        );
        assert_eq!(respawn_config.respawn_data.pos(), BlockPos::new(10, 64, -3));
        assert_eq!(
            respawn_config.respawn_data.yaw.to_bits(),
            (-179.0_f32).to_bits()
        );
        assert_eq!(
            respawn_config.respawn_data.pitch.to_bits(),
            (-90.0_f32).to_bits()
        );
        assert!(!respawn_config.forced);

        assert_eq!(persistent.ender_pearls.len(), 2);
        assert_eq!(persistent.ender_pearls[0].world, "minecraft:overworld");
        assert_eq!(persistent.ender_pearls[1].world, "minecraft:the_nether");
        assert_eq!(persistent.ender_pearls[0].entity.uuid, [7; 16]);
        assert_eq!(
            persistent.ender_pearls[0].entity.pos.map(f64::to_bits),
            [4.0_f64.to_bits(), 65.0_f64.to_bits(), 6.0_f64.to_bits()]
        );
    }

    #[test]
    fn compressed_player_payload_cannot_expand_past_the_storage_ceiling() {
        const EXPECTED_MAX_PLAYER_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
        let compressed = zstd::encode_all(repeat(0).take(EXPECTED_MAX_PLAYER_PAYLOAD_BYTES + 1), 3)
            .expect("the highly compressible payload should encode");
        let mut bytes = Vec::with_capacity(6 + compressed.len());
        bytes.extend_from_slice(&PLAYER_MAGIC);
        bytes.extend_from_slice(&PLAYER_STORAGE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&compressed);

        let Err(error) = decode_file(PLAYER_MAGIC, PLAYER_STORAGE_VERSION, &bytes) else {
            panic!("an oversized expanded payload must be refused");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn sparse_player_file_is_rejected_before_its_declared_size_is_allocated() {
        const EXPECTED_MAX_PLAYER_FILE_BYTES: u64 = 64 * 1024 * 1024;
        let root = temp_storage_root("sparse-oversized-player");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let uuid = Uuid::from_u128(42);
        let path = FilePlayerDataStorage::player_file(
            &storage
                .domain_players_dir("minecraft")
                .expect("minecraft is a valid domain"),
            uuid,
        );
        fs::create_dir_all(path.parent().expect("player path has a parent"))
            .await
            .expect("player directory should exist");
        let file = fs::File::create(&path)
            .await
            .expect("sparse test file should be created");
        file.set_len(EXPECTED_MAX_PLAYER_FILE_BYTES + 1)
            .await
            .expect("sparse test file should expose an oversized length");

        let error = storage
            .load_domain("minecraft", uuid)
            .await
            .expect_err("an oversized file must be refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exceeds"));

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn persisted_domain_name_cannot_escape_the_save_root() {
        let root = temp_storage_root("domain-traversal");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");

        for domain in [
            ".",
            "..",
            "../outside",
            "inside/outside",
            "inside\\outside",
            "alias.",
            "alias ",
            "global",
        ] {
            let error = storage
                .load_domain(domain, Uuid::from_u128(42))
                .await
                .expect_err("path-like and aliased domain names must be refused");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{domain:?}");
        }

        let players = storage
            .domain_players_dir("minecraft")
            .expect("a normal namespace should be accepted");
        assert_eq!(
            players
                .strip_prefix(&root)
                .expect("a domain path must remain below its save root"),
            Path::new("minecraft").join("players")
        );

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[test]
    fn traversing_last_active_domain_is_rejected_when_global_data_is_decoded() {
        let file = GlobalPlayerDataFile {
            data_version: GLOBAL_PLAYER_DATA_VERSION,
            last_active_domain: "../outside".to_owned(),
            first_played: 1_000,
            last_played: 2_000,
            whitelisted: false,
        };
        let encoded = encode_global_file(&file).expect("hostile fixture should encode");

        let Err(error) = decode_global_file(&encoded) else {
            panic!("a path-like last-active domain must be treated as corrupt data");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn non_finite_player_state_is_rejected_before_materialization() {
        let mut non_finite_position = sample_player_file(PLAYER_DATA_VERSION);
        non_finite_position.pos[0] = f64::NAN;
        assert_eq!(
            non_finite_position
                .into_persistent()
                .expect_err("NaN position must be refused")
                .kind(),
            io::ErrorKind::InvalidData
        );

        let mut non_finite_health = sample_player_file(PLAYER_DATA_VERSION);
        non_finite_health.health = f32::INFINITY;
        assert_eq!(
            non_finite_health
                .into_persistent()
                .expect_err("infinite health must be refused")
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[tokio::test]
    async fn valid_backup_is_loaded_when_the_current_player_file_is_corrupt() {
        let root = temp_storage_root("corrupt-current-valid-backup");
        let storage = FilePlayerDataStorage::new(root.clone())
            .await
            .expect("test storage should initialize");
        let uuid = Uuid::from_u128(42);
        let mut first = sample_player_file(PLAYER_DATA_VERSION);
        first.score = 10;
        let mut second = sample_player_file(PLAYER_DATA_VERSION);
        second.score = 20;
        storage
            .write_atomic(
                &storage
                    .domain_players_dir("minecraft")
                    .expect("minecraft is a valid domain"),
                uuid,
                encode_player_file(&first).expect("first generation should encode"),
            )
            .await
            .expect("first generation should publish");
        storage
            .write_atomic(
                &storage
                    .domain_players_dir("minecraft")
                    .expect("minecraft is a valid domain"),
                uuid,
                encode_player_file(&second).expect("second generation should encode"),
            )
            .await
            .expect("second generation should publish");
        let path = FilePlayerDataStorage::player_file(
            &storage
                .domain_players_dir("minecraft")
                .expect("minecraft is a valid domain"),
            uuid,
        );
        fs::write(&path, b"corrupt current generation")
            .await
            .expect("current generation should be corrupted for the test");

        let restored = storage
            .load_domain("minecraft", uuid)
            .await
            .expect("the valid backup should recover")
            .expect("a committed player generation should exist");
        assert_eq!(restored.score, 10);
        assert_eq!(
            fs::read(&path)
                .await
                .expect("the recovered primary should be readable"),
            encode_player_file(&first).expect("first generation should encode"),
            "loading a backup should heal the corrupt primary"
        );

        let backup = FilePlayerDataStorage::atomic_backup_path(&path);
        assert_eq!(
            fs::read(&backup)
                .await
                .expect("the known-good backup should remain readable"),
            encode_player_file(&first).expect("first generation should encode"),
            "recovery must preserve the last known-good backup"
        );

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[tokio::test]
    async fn third_atomic_publication_replaces_an_existing_windows_backup() {
        let root = temp_storage_root("third-atomic-publication");
        let path = root.join("state.dat");

        for generation in [b"first".as_slice(), b"second", b"third"] {
            FilePlayerDataStorage::write_atomic_path_locked(&path, generation.to_vec())
                .await
                .expect("every generation should publish atomically");
        }

        assert_eq!(
            fs::read(&path).await.expect("live file should be readable"),
            b"third"
        );
        assert_eq!(
            fs::read(FilePlayerDataStorage::atomic_backup_path(&path))
                .await
                .expect("backup should be readable"),
            b"second"
        );

        fs::remove_dir_all(root)
            .await
            .expect("temporary storage should be removable");
    }

    #[test]
    fn stale_player_payload_version_is_rejected() {
        let file = sample_player_file(PLAYER_DATA_VERSION - 1);

        let error = file
            .into_persistent()
            .expect_err("stale payload should fail");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
