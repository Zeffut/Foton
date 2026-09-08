//! Server configuration loading.
//!
//! This module handles loading the server configuration from disk.
//! The config is loaded once at startup, split into creation-time values
//! (consumed by the server constructor) and a `RuntimeConfig` (stored on `Server`).

mod groups;
mod logging;
mod server;

pub use groups::FilePermissionGroupStore;
pub use logging::{LogConfig, LogLevel, LogTimeFormat, RotationTimeFormat};
pub use server::{ServerConfig, ThreadConfig};

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
    process,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use foton_core::{
    config::WorldsConfig,
    permission::{PermissionGroupStore, PermissionGroupsConfig},
};
use serde::Deserialize;

use self::{groups::load_or_create_groups, server::validate};

#[cfg(feature = "stand-alone")]
const DEFAULT_FAVICON: &[u8] = include_bytes!("../../../package-content/favicon.png");

const DEFAULT_CONFIG: &str = include_str!("../../../package-content/config.toml");
const DEFAULT_WORLDS: &str = include_str!("../../../package-content/worlds.toml");

static TEMPORARY_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Top-level TOML deserialization target — used once at startup, not stored globally.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FotonConfig {
    /// The full server configuration (`[server]` section)
    pub server: ServerConfig,
    /// Logging configuration (`[log]` section)
    pub log: Option<LogConfig>,
    /// World and domain configuration from `worlds.toml`.
    #[serde(skip, default = "empty_worlds_config")]
    pub worlds: WorldsConfig,
    /// Permission group configuration from `groups.toml`.
    #[serde(skip, default)]
    pub groups: PermissionGroupsConfig,
    /// Path to the loaded `groups.toml`.
    #[serde(skip, default)]
    pub groups_path: Option<PathBuf>,
}

impl FotonConfig {
    /// Builds the store used for persistence-first permission group updates.
    #[must_use]
    pub fn permission_group_store(&self) -> Option<Arc<dyn PermissionGroupStore>> {
        self.groups_path.as_ref().map(|path| {
            Arc::new(FilePermissionGroupStore::new(path.clone())) as Arc<dyn PermissionGroupStore>
        })
    }
}

const fn empty_worlds_config() -> WorldsConfig {
    WorldsConfig {
        save_path: String::new(),
        seed: None,
        default_gamemode: None,
        difficulty: None,
        storage: None,
        player_storage: None,
        domains: BTreeMap::new(),
    }
}

/// Loads the server configuration from the given path, or creates it if it doesn't exist.
pub fn load_or_create(path: &Path) -> Result<FotonConfig, String> {
    let parent = config_parent(path)
        .ok_or_else(|| format!("failed to get config directory for {}", path.display()))?;
    let mut config = if path.exists() {
        let config_str = fs::read_to_string(path)
            .map_err(|e| format!("failed to read config file {}: {e}", path.display()))?;
        let config: FotonConfig = toml::from_str(config_str.as_str())
            .map_err(|e| format!("failed to parse config: {e}"))?;
        validate(&config.server).map_err(|e| format!("failed to validate config: {e}"))?;
        config
    } else {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "failed to create config directory {}: {e}",
                parent.display()
            )
        })?;
        write_atomic_config(path, DEFAULT_CONFIG.as_bytes())
            .map_err(|e| format!("failed to write config file {}: {e}", path.display()))?;
        let config: FotonConfig = toml::from_str(DEFAULT_CONFIG)
            .map_err(|e| format!("failed to parse default config: {e}"))?;
        validate(&config.server).map_err(|e| format!("failed to validate default config: {e}"))?;
        config
    };

    let worlds_path = parent.join("worlds.toml");
    config.worlds = load_or_create_worlds(&worlds_path)?;
    let groups_path = parent.join("groups.toml");
    config.groups = load_or_create_groups(&groups_path)?;
    config.groups_path = Some(groups_path);

    // If icon file doesnt exist, write it
    #[cfg(feature = "stand-alone")]
    if config.server.use_favicon && !Path::new(&config.server.favicon).exists() {
        fs::write(Path::new(&config.server.favicon), DEFAULT_FAVICON).map_err(|e| {
            format!(
                "failed to write favicon file {}: {e}",
                config.server.favicon
            )
        })?;
    }

    Ok(config)
}

fn load_or_create_worlds(path: &Path) -> Result<WorldsConfig, String> {
    if path.exists() {
        let worlds_str = fs::read_to_string(path)
            .map_err(|e| format!("failed to read worlds config file {}: {e}", path.display()))?;
        toml::from_str(worlds_str.as_str())
            .map_err(|e| format!("failed to parse worlds config {}: {e}", path.display()))
    } else {
        write_atomic_config(path, DEFAULT_WORLDS.as_bytes())
            .map_err(|e| format!("failed to write worlds config file {}: {e}", path.display()))?;
        toml::from_str(DEFAULT_WORLDS)
            .map_err(|e| format!("failed to parse default worlds config: {e}"))
    }
}

fn config_parent(path: &Path) -> Option<&Path> {
    let parent = path.parent()?;
    Some(if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    })
}

pub(super) fn write_atomic_config(path: &Path, contents: &[u8]) -> io::Result<()> {
    write_atomic_config_with_before_rename(path, contents, || Ok(()))
}

pub(super) fn write_atomic_config_with_before_rename<F>(
    path: &Path,
    contents: &[u8],
    before_rename: F,
) -> io::Result<()>
where
    F: FnOnce() -> io::Result<()>,
{
    let parent = config_parent(path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "config path has no parent"))?;
    fs::create_dir_all(parent)?;

    let (temporary, mut file) = create_temporary_file(path)?;
    let publication = (|| {
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        before_rename()?;
        fs::rename(&temporary, path)
    })();

    if let Err(error) = publication {
        return match fs::remove_file(&temporary) {
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

    if cfg!(unix) {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let counter = TEMPORARY_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut file_name = match path.file_name() {
        Some(file_name) => file_name.to_os_string(),
        None => OsString::from("config"),
    };
    file_name.push(format!(".tmp-{}-{counter}", process::id()));
    path.with_file_name(file_name)
}

fn create_temporary_file(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    loop {
        let temporary = temporary_path(path);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod atomic_tests {
    use std::{
        env::temp_dir,
        fs,
        path::Path,
        sync::{Arc, Barrier},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::groups::DEFAULT_GROUPS;
    use super::{
        DEFAULT_CONFIG, DEFAULT_WORLDS, load_or_create, load_or_create_worlds,
        write_atomic_config_with_before_rename,
    };

    fn temp_config_root(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        temp_dir().join(format!("foton-atomic-config-{name}-{unique}"))
    }

    #[test]
    fn initial_server_config_preserves_packaged_contents() {
        let root = temp_config_root("server");
        let path = root.join("config.toml");

        let result = load_or_create(&path).expect("initial config should be created");

        assert_eq!(
            fs::read(&path).expect("config should be readable"),
            DEFAULT_CONFIG.as_bytes()
        );
        assert_eq!(result.groups_path, Some(root.join("groups.toml")));
        fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn initial_worlds_config_preserves_packaged_contents() {
        let root = temp_config_root("worlds");
        let path = root.join("worlds.toml");

        load_or_create_worlds(&path).expect("initial worlds config should be created");

        assert_eq!(
            fs::read(&path).expect("worlds config should be readable"),
            DEFAULT_WORLDS.as_bytes()
        );
        fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn overlapping_config_writes_publish_without_colliding() {
        let root = temp_config_root("overlapping-writes");
        fs::create_dir_all(&root).expect("test directory should be created");
        let path = root.join("config.toml");
        let barrier = Arc::new(Barrier::new(2));

        let first_path = path.clone();
        let first_barrier = Arc::clone(&barrier);
        let first = std::thread::spawn(move || {
            write_atomic_config_with_before_rename(&first_path, b"first", move || {
                first_barrier.wait();
                Ok(())
            })
        });
        let second_path = path.clone();
        let second_barrier = Arc::clone(&barrier);
        let second = std::thread::spawn(move || {
            write_atomic_config_with_before_rename(&second_path, b"second", move || {
                second_barrier.wait();
                Ok(())
            })
        });

        assert!(first.join().expect("first writer should not panic").is_ok());
        assert!(
            second
                .join()
                .expect("second writer should not panic")
                .is_ok()
        );
        let contents = fs::read(&path).expect("published config should be readable");
        assert!(contents == b"first" || contents == b"second");
        fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn bare_relative_config_path_is_supported_without_writing_to_the_caller_cwd() {
        if std::env::var_os("FOTON_BARE_RELATIVE_CONFIG_CHILD").is_some() {
            let config = load_or_create(Path::new("config.toml"))
                .expect("bare relative config path should load");
            assert_eq!(
                fs::read("config.toml").expect("relative config should be readable"),
                DEFAULT_CONFIG.as_bytes()
            );
            assert!(config.groups_path.is_some());
            assert_eq!(
                fs::read("worlds.toml").expect("relative worlds config should be readable"),
                DEFAULT_WORLDS.as_bytes()
            );
            assert_eq!(
                fs::read("groups.toml").expect("relative groups config should be readable"),
                DEFAULT_GROUPS.as_bytes()
            );
            return;
        }

        let root = temp_config_root("bare-relative");
        fs::create_dir_all(&root).expect("test directory should be created");
        let status = std::process::Command::new(
            std::env::current_exe().expect("test executable should exist"),
        )
        .current_dir(&root)
        .env("FOTON_BARE_RELATIVE_CONFIG_CHILD", "1")
        .args([
            "--exact",
            "config::atomic_tests::bare_relative_config_path_is_supported_without_writing_to_the_caller_cwd",
            "--nocapture",
        ])
        .status()
        .expect("child test should run");

        assert!(status.success(), "bare relative config child should pass");
        fs::remove_dir_all(root).expect("test directory should be removed");
    }
}
