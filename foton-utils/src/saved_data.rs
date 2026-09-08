//! Per-world saved data storage.
//!
//! Vanilla stores world-level saved data under each dimension's `data/`
//! directory. Foton uses the same per-world saved-data boundary for both its
//! human-readable TOML data and versioned binary data.

use std::{
    ffi::OsString,
    fmt::Display,
    fs as sync_fs, io,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Serialize, de::DeserializeOwned};
use tokio::{fs, task::spawn_blocking};
use wincode::{SchemaRead, SchemaWrite, config::DefaultConfig};

static TEMPORARY_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Built-in saved data entry names.
pub mod names {
    use super::{SavedDataName, WincodeSavedDataName};

    /// Vanilla `TicketStorage.TYPE`, persisted as `data/chunk_tickets.toml`.
    pub const CHUNK_TICKETS: SavedDataName = SavedDataName::trusted("chunk_tickets");
    /// Cached concentric-ring positions, persisted as `data/structure_rings.bin`.
    pub const STRUCTURE_RINGS: WincodeSavedDataName =
        WincodeSavedDataName::trusted("structure_rings", *b"STLR", 2);
    /// Vanilla `Raids.TYPE`, persisted as `data/raids.toml`.
    ///
    /// Per loaded world rather than per domain, the way `chunk_tickets` is: a
    /// raid belongs to the dimension whose village it besieges.
    pub const RAIDS: SavedDataName = SavedDataName::trusted("raids");
    /// Vanilla `EnderDragonFight.TYPE`, persisted as `data/ender_dragon_fight.toml`.
    ///
    /// Per loaded world, like the two above it: a fight belongs to the End it
    /// runs in, and a server can host more than one.
    pub const ENDER_DRAGON_FIGHT: SavedDataName = SavedDataName::trusted("ender_dragon_fight");
    /// Domain command scoreboard, persisted through the domain default world.
    pub const SCOREBOARD: SavedDataName = SavedDataName::trusted("scoreboard");
    /// Domain command storage, persisted through the domain default world.
    pub const COMMAND_STORAGE: SavedDataName = SavedDataName::trusted("command_storage");
    /// Domain boss bars a command owns, persisted through the domain default
    /// world.
    pub const CUSTOM_BOSS_EVENTS: SavedDataName = SavedDataName::trusted("custom_boss_events");
    /// Every filled map of a domain plus its id counter, persisted through the
    /// domain default world as `data/maps.bin`.
    ///
    /// Vanilla splits this across one `data/maps/<id>.dat` per map and a
    /// `data/maps/last_id.dat` counter. A saved-data name here is a
    /// `&'static str`, so a file per map is not expressible; the layout is not
    /// observable in game, and one binary file avoids writing 16 KiB color
    /// arrays through the TOML encoder.
    pub const MAPS: WincodeSavedDataName = WincodeSavedDataName::trusted("maps", *b"STMD", 1);
}

/// Name of a per-world saved data entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedDataName(&'static str);

impl SavedDataName {
    /// Creates a saved-data name from a static trusted identifier.
    #[must_use]
    pub(crate) const fn trusted(name: &'static str) -> Self {
        Self(name)
    }

    /// Creates a saved-data name after validating that it cannot escape `data/`.
    pub fn try_new(name: &'static str) -> Result<Self, String> {
        if is_valid_saved_data_name(name) {
            Ok(Self(name))
        } else {
            Err(format!("invalid saved data name {name}"))
        }
    }

    fn file_name(self) -> String {
        format!("{}.toml", self.0)
    }
}

/// Name and format header of a wincode-encoded per-world saved data entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WincodeSavedDataName {
    name: &'static str,
    magic: [u8; 4],
    version: u16,
}

impl WincodeSavedDataName {
    /// Creates a binary saved-data name from trusted format metadata.
    #[must_use]
    pub(crate) const fn trusted(name: &'static str, magic: [u8; 4], version: u16) -> Self {
        Self {
            name,
            magic,
            version,
        }
    }

    /// Creates a binary saved-data name after validating that it cannot escape `data/`.
    pub fn try_new(name: &'static str, magic: [u8; 4], version: u16) -> Result<Self, String> {
        if is_valid_saved_data_name(name) {
            Ok(Self {
                name,
                magic,
                version,
            })
        } else {
            Err(format!("invalid saved data name {name}"))
        }
    }

    fn file_name(self) -> String {
        format!("{}.bin", self.name)
    }
}

fn is_valid_saved_data_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && crate::Identifier::validate_path(name)
}

/// Typed saved-data storage for a loaded world.
#[derive(Debug, Clone)]
pub struct SavedDataManager {
    data_dir: Option<PathBuf>,
}

/// Publishes `bytes` at `path` by renaming a fully written temporary over it.
///
/// A plain write truncates the destination first, so anything that interrupts it
/// -- a crash, a full disk, a kill -- leaves a short or empty file where a good
/// one was. These files are rewritten every autosave on a live server, so that
/// window comes round every five minutes, and an empty `scoreboard.toml` parses
/// as a perfectly valid empty scoreboard: the loss would be silent. `level.toml`
/// and the player store already publish this way; this is the same pattern.
async fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomically_with_before_rename(path.to_owned(), bytes.to_vec(), || Ok(())).await
}

async fn write_atomically_with_before_rename<F>(
    path: PathBuf,
    bytes: Vec<u8>,
    before_rename: F,
) -> io::Result<()>
where
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    spawn_blocking(move || {
        write_atomically_sync_with_parent_sync(
            &path,
            &bytes,
            sync_parent_directory_sync,
            before_rename,
        )
    })
    .await
    .map_err(|error| io::Error::other(format!("atomic write task failed: {error}")))?
}

/// Blocking twin of [`write_atomically`], for the shutdown path.
fn write_atomically_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomically_sync_with_parent_sync(path, bytes, sync_parent_directory_sync, || Ok(()))
}

fn temporary_path(path: &Path) -> PathBuf {
    let counter = TEMPORARY_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut file_name = match path.file_name() {
        Some(file_name) => file_name.to_os_string(),
        None => OsString::from("saved-data"),
    };
    file_name.push(format!(".tmp-{}-{counter}", process::id()));
    path.with_file_name(file_name)
}

fn sync_parent_directory_sync(parent: &Path) -> io::Result<()> {
    if cfg!(unix) {
        sync_fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn write_atomically_sync_with_parent_sync<F, G>(
    path: &Path,
    bytes: &[u8],
    sync_parent: F,
    before_rename: G,
) -> io::Result<()>
where
    F: FnOnce(&Path) -> io::Result<()>,
    G: FnOnce() -> io::Result<()>,
{
    use std::io::Write as _;

    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic saved-data path has no parent",
        )
    })?;
    let (temporary, mut file) = create_temporary_file_sync(path)?;
    let publication = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        before_rename()?;
        sync_fs::rename(&temporary, path)
    })();

    if let Err(error) = publication {
        return match sync_fs::remove_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup_error) if cleanup_error.kind() == io::ErrorKind::NotFound => Err(error),
            Err(cleanup_error) => Err(io::Error::new(
                error.kind(),
                format!(
                    "{error}; additionally failed to remove temporary file {}: {cleanup_error}",
                    temporary.display()
                ),
            )),
        };
    }

    sync_parent(parent)
}

fn create_temporary_file_sync(path: &Path) -> io::Result<(PathBuf, sync_fs::File)> {
    loop {
        let temporary = temporary_path(path);
        let mut options = sync_fs::OpenOptions::new();
        options.write(true).create_new(true);
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

impl SavedDataManager {
    /// Creates saved-data storage rooted at `world_dir/data`.
    ///
    /// `None` means the world is ephemeral, matching Foton's RAM-only storage.
    #[must_use]
    pub fn new(world_dir: Option<&Path>) -> Self {
        Self {
            data_dir: world_dir.map(|path| path.join("data")),
        }
    }

    /// Loads a versioned wincode value, or returns `None` when it is absent or
    /// this world has no persistent storage.
    pub fn sync_load_wincode<T>(&self, name: WincodeSavedDataName) -> io::Result<Option<T>>
    where
        for<'de> T: SchemaRead<'de, DefaultConfig, Dst = T>,
    {
        let Some(path) = self.wincode_path_for(name) else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }

        let bytes = sync_fs::read(&path)?;
        let Some((magic, remainder)) = bytes.split_first_chunk::<4>() else {
            return Err(invalid_binary_data(&path, "missing magic header"));
        };
        if magic != &name.magic {
            return Err(invalid_binary_data(&path, "unexpected magic header"));
        }
        let Some((version, payload)) = remainder.split_first_chunk::<2>() else {
            return Err(invalid_binary_data(&path, "missing format version"));
        };
        if u16::from_le_bytes(*version) != name.version {
            return Err(invalid_binary_data(
                &path,
                format!(
                    "unsupported format version {}",
                    u16::from_le_bytes(*version)
                ),
            ));
        }

        wincode::deserialize_exact(payload)
            .map(Some)
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid binary saved data {}: {error}", path.display()),
                )
            })
    }

    /// Loads saved data, or returns `T::default()` when the data file is absent
    /// or this world has no persistent storage.
    pub async fn load_or_default<T>(&self, name: SavedDataName) -> io::Result<T>
    where
        T: DeserializeOwned + Default,
    {
        let Some(path) = self.path_for(name) else {
            return Ok(T::default());
        };
        if !path.exists() {
            return Ok(T::default());
        }

        let content = fs::read_to_string(&path).await?;
        toml::from_str(&content).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid saved data {}: {error}", path.display()),
            )
        })
    }

    /// Saves a versioned wincode value.
    pub fn sync_save_wincode<T>(&self, name: WincodeSavedDataName, data: &T) -> io::Result<()>
    where
        T: SchemaWrite<DefaultConfig, Src = T>,
    {
        let Some(path) = self.wincode_path_for(name) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            sync_fs::create_dir_all(parent)?;
        }

        let payload = wincode::serialize(data)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let mut bytes = Vec::with_capacity(6 + payload.len());
        bytes.extend_from_slice(&name.magic);
        bytes.extend_from_slice(&name.version.to_le_bytes());
        bytes.extend_from_slice(&payload);
        write_atomically_sync(&path, &bytes)
    }

    /// Saves a typed saved-data value.
    pub async fn save<T>(&self, name: SavedDataName, data: &T) -> io::Result<()>
    where
        T: Serialize,
    {
        let Some(path) = self.path_for(name) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let content = toml::to_string_pretty(data)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        write_atomically(&path, content.as_bytes()).await
    }

    fn path_for(&self, name: SavedDataName) -> Option<PathBuf> {
        self.data_dir
            .as_ref()
            .map(|data_dir| data_dir.join(name.file_name()))
    }

    fn wincode_path_for(&self, name: WincodeSavedDataName) -> Option<PathBuf> {
        self.data_dir
            .as_ref()
            .map(|data_dir| data_dir.join(name.file_name()))
    }
}

fn invalid_binary_data(path: &Path, message: impl Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Invalid binary saved data {}: {message}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        io::{Error, ErrorKind},
        path::PathBuf,
        sync::{Arc, Barrier, mpsc},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use serde::{Deserialize, Serialize};
    use tokio::{fs as tokio_fs, sync::oneshot, task::yield_now, time::timeout};

    use wincode::{SchemaRead, SchemaWrite};

    use super::{
        SavedDataManager, SavedDataName, WincodeSavedDataName, sync_fs,
        write_atomically_sync_with_parent_sync, write_atomically_with_before_rename,
    };

    const TEST_DATA: SavedDataName = SavedDataName::trusted("test_data");
    const TEST_BINARY_DATA: WincodeSavedDataName =
        WincodeSavedDataName::trusted("test_binary_data", *b"TEST", 3);

    #[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct TestData {
        value: i32,
    }

    #[derive(Debug, PartialEq, Eq, SchemaWrite, SchemaRead)]
    struct TestBinaryData {
        value: i32,
    }

    #[test]
    fn saved_data_name_rejects_paths() {
        assert!(SavedDataName::try_new("valid_name").is_ok());
        assert!(SavedDataName::try_new("../outside").is_err());
        assert!(SavedDataName::try_new("nested/name").is_err());
        assert!(SavedDataName::try_new("nested\\name").is_err());
        assert!(SavedDataName::try_new("").is_err());
        assert!(WincodeSavedDataName::try_new("valid_name", *b"TEST", 1).is_ok());
        assert!(WincodeSavedDataName::try_new("../outside", *b"TEST", 1).is_err());
    }

    #[tokio::test]
    async fn overlapping_async_atomic_writes_publish_without_colliding() {
        let dir = temp_world_dir("overlapping-async-writes");
        sync_fs::create_dir_all(&dir).expect("test directory should be created");
        let path = dir.join("saved.toml");
        let barrier = Arc::new(Barrier::new(2));

        let first_barrier = Arc::clone(&barrier);
        let first =
            write_atomically_with_before_rename(path.clone(), b"first".to_vec(), move || {
                first_barrier.wait();
                Ok(())
            });
        let second_barrier = Arc::clone(&barrier);
        let second =
            write_atomically_with_before_rename(path.clone(), b"second".to_vec(), move || {
                second_barrier.wait();
                Ok(())
            });

        let (first_result, second_result) = tokio::join!(first, second);
        assert!(
            first_result.is_ok(),
            "first overlapping write should succeed"
        );
        assert!(
            second_result.is_ok(),
            "second overlapping write should succeed"
        );
        let contents = sync_fs::read(&path).expect("published file should be readable");
        assert!(contents == b"first" || contents == b"second");
        sync_fs::remove_dir_all(dir).expect("test directory should be removed");
    }

    #[test]
    fn sync_atomic_write_syncs_the_containing_directory_after_rename() {
        let dir = temp_world_dir("sync-parent-sync");
        sync_fs::create_dir_all(&dir).expect("test directory should be created");
        let path = dir.join("saved.bin");
        let expected_parent = dir.clone();

        let error = write_atomically_sync_with_parent_sync(
            &path,
            b"saved",
            |parent| {
                assert_eq!(parent, expected_parent);
                Err(Error::other("parent sync failed"))
            },
            || Ok(()),
        )
        .expect_err("the injected directory sync failure should be returned");

        assert_eq!(error.kind(), ErrorKind::Other);
        assert_eq!(
            sync_fs::read(&path).expect("rename should precede directory sync"),
            b"saved"
        );
        sync_fs::remove_dir_all(dir).expect("test directory should be removed");
    }

    #[test]
    fn overlapping_sync_atomic_writes_publish_without_colliding() {
        let dir = temp_world_dir("overlapping-sync-writes");
        sync_fs::create_dir_all(&dir).expect("test directory should be created");
        let path = dir.join("saved.toml");
        let barrier = Arc::new(Barrier::new(2));

        let first_path = path.clone();
        let first_barrier = Arc::clone(&barrier);
        let first = thread::spawn(move || {
            write_atomically_sync_with_parent_sync(
                &first_path,
                b"first",
                |_| Ok(()),
                move || {
                    first_barrier.wait();
                    Ok(())
                },
            )
        });
        let second_path = path.clone();
        let second_barrier = Arc::clone(&barrier);
        let second = thread::spawn(move || {
            write_atomically_sync_with_parent_sync(
                &second_path,
                b"second",
                |_| Ok(()),
                move || {
                    second_barrier.wait();
                    Ok(())
                },
            )
        });

        assert!(first.join().expect("first writer should not panic").is_ok());
        assert!(
            second
                .join()
                .expect("second writer should not panic")
                .is_ok()
        );
        let contents = sync_fs::read(&path).expect("published file should be readable");
        assert!(contents == b"first" || contents == b"second");
        sync_fs::remove_dir_all(dir).expect("test directory should be removed");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelling_async_atomic_write_finishes_the_blocking_transaction() {
        let dir = temp_world_dir("cancelled-async-write");
        sync_fs::create_dir_all(&dir).expect("test directory should be created");
        let path = dir.join("saved.toml");
        let (started_sender, started_receiver) = oneshot::channel();
        let (release_sender, release_receiver) = mpsc::channel();

        let writer = tokio::spawn(write_atomically_with_before_rename(
            path.clone(),
            b"saved".to_vec(),
            move || {
                started_sender
                    .send(())
                    .map_err(|()| Error::other("cancellation test receiver dropped"))?;
                release_receiver
                    .recv()
                    .map_err(|_| Error::other("cancellation test release dropped"))?;
                Ok(())
            },
        ));
        started_receiver
            .await
            .expect("blocking transaction should start");

        writer.abort();
        assert!(
            writer
                .await
                .expect_err("writer should be cancelled")
                .is_cancelled()
        );
        release_sender
            .send(())
            .expect("blocking transaction should be released");

        timeout(Duration::from_secs(5), async {
            loop {
                if tokio_fs::try_exists(&path)
                    .await
                    .expect("final path existence should be readable")
                {
                    break;
                }
                yield_now().await;
            }
        })
        .await
        .expect("blocking transaction should finish after cancellation");

        assert_eq!(
            sync_fs::read(&path).expect("published file should be readable"),
            b"saved"
        );
        let entries = sync_fs::read_dir(&dir)
            .expect("temporary directory should be readable")
            .collect::<Result<Vec<_>, _>>()
            .expect("directory entries should be readable");
        assert_eq!(
            entries.len(),
            1,
            "cancelled write should not orphan a temp file"
        );
        assert_eq!(entries[0].file_name(), "saved.toml");
        sync_fs::remove_dir_all(dir).expect("test directory should be removed");
    }

    fn temp_world_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        temp_dir().join(format!("foton-saved-data-{test_name}-{unique}"))
    }

    #[tokio::test]
    async fn missing_saved_data_loads_default() {
        let dir = temp_world_dir("missing");
        let manager = SavedDataManager::new(Some(dir.as_path()));

        let loaded: TestData = manager
            .load_or_default(TEST_DATA)
            .await
            .expect("missing saved data should load default");

        assert_eq!(loaded, TestData::default());
    }

    #[tokio::test]
    async fn saved_data_round_trips_through_world_data_dir() {
        let dir = temp_world_dir("round-trip");
        let manager = SavedDataManager::new(Some(dir.as_path()));

        manager
            .save(TEST_DATA, &TestData { value: 42 })
            .await
            .expect("saved data should write");
        let loaded: TestData = manager
            .load_or_default(TEST_DATA)
            .await
            .expect("saved data should load");

        assert_eq!(loaded, TestData { value: 42 });
        assert!(dir.join("data").join("test_data.toml").exists());
    }

    #[tokio::test]
    async fn ephemeral_saved_data_does_not_write() {
        let manager = SavedDataManager::new(None);

        manager
            .save(TEST_DATA, &TestData { value: 42 })
            .await
            .expect("ephemeral save should be a no-op");
        let loaded: TestData = manager
            .load_or_default(TEST_DATA)
            .await
            .expect("ephemeral load should return default");

        assert_eq!(loaded, TestData::default());
    }

    #[test]
    fn wincode_saved_data_round_trips_with_header() {
        let dir = temp_world_dir("binary-round-trip");
        let manager = SavedDataManager::new(Some(dir.as_path()));

        manager
            .sync_save_wincode(TEST_BINARY_DATA, &TestBinaryData { value: 42 })
            .expect("binary saved data should write");
        let loaded: TestBinaryData = manager
            .sync_load_wincode(TEST_BINARY_DATA)
            .expect("binary saved data should load")
            .expect("binary saved data should exist");

        assert_eq!(loaded, TestBinaryData { value: 42 });
        let bytes = sync_fs::read(dir.join("data").join("test_binary_data.bin"))
            .expect("binary saved data file should exist");
        assert_eq!(&bytes[..6], b"TEST\x03\x00");

        let newer_format = WincodeSavedDataName::trusted("test_binary_data", *b"TEST", 4);
        let error = manager
            .sync_load_wincode::<TestBinaryData>(newer_format)
            .expect_err("mismatched binary format version should fail");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn missing_and_ephemeral_wincode_data_return_none() {
        let dir = temp_world_dir("binary-missing");
        let persistent = SavedDataManager::new(Some(dir.as_path()));
        let ephemeral = SavedDataManager::new(None);

        assert!(
            persistent
                .sync_load_wincode::<TestBinaryData>(TEST_BINARY_DATA)
                .expect("missing binary data should load")
                .is_none()
        );
        assert!(
            ephemeral
                .sync_load_wincode::<TestBinaryData>(TEST_BINARY_DATA)
                .expect("ephemeral binary data should load")
                .is_none()
        );
        ephemeral
            .sync_save_wincode(TEST_BINARY_DATA, &TestBinaryData { value: 42 })
            .expect("ephemeral binary save should be a no-op");
    }
}
