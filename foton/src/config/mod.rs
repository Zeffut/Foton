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
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::Arc,
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
    let mut config = if path.exists() {
        let config_str = fs::read_to_string(path)
            .map_err(|e| format!("failed to read config file {}: {e}", path.display()))?;
        let config: FotonConfig = toml::from_str(config_str.as_str())
            .map_err(|e| format!("failed to parse config: {e}"))?;
        validate(&config.server).map_err(|e| format!("failed to validate config: {e}"))?;
        config
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| format!("failed to get config directory for {}", path.display()))?;
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

    let worlds_path = path
        .parent()
        .ok_or_else(|| format!("failed to get config directory for {}", path.display()))?
        .join("worlds.toml");
    config.worlds = load_or_create_worlds(&worlds_path)?;
    let groups_path = path
        .parent()
        .ok_or_else(|| format!("failed to get config directory for {}", path.display()))?
        .join("groups.toml");
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

pub(super) fn write_atomic_config(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "config path has no parent"))?;
    fs::create_dir_all(parent)?;

    let temporary = path.with_extension("toml.tmp");
    let publication = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod atomic_tests {
    use std::{
        env::temp_dir,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{load_or_create, load_or_create_worlds};

    fn temp_config_root(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        temp_dir().join(format!("foton-atomic-config-{name}-{unique}"))
    }

    #[test]
    fn initial_server_config_uses_atomic_publication() {
        let root = temp_config_root("server");
        fs::create_dir_all(root.join("config.toml.tmp"))
            .expect("blocking the atomic temporary path should succeed");
        let path = root.join("config.toml");

        let result = load_or_create(&path);

        assert!(result.is_err(), "the blocked atomic write should fail");
        assert!(
            !path.exists(),
            "a failed atomic write must not publish a file"
        );
        fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn initial_worlds_config_uses_atomic_publication() {
        let root = temp_config_root("worlds");
        fs::create_dir_all(root.join("worlds.toml.tmp"))
            .expect("blocking the atomic temporary path should succeed");
        let path = root.join("worlds.toml");

        let result = load_or_create_worlds(&path);

        assert!(result.is_err(), "the blocked atomic write should fail");
        assert!(
            !path.exists(),
            "a failed atomic write must not publish a file"
        );
        fs::remove_dir_all(root).expect("test directory should be removed");
    }
}
