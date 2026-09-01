//! The secret Foton and its Geyser share.
//!
//! Sixteen raw bytes, which is what Geyser's `AesKeyProducer.produceFrom` reads
//! a key file as. Not PEM despite the conventional name -- Geyser reads the
//! file whole and hands it to `SecretKeySpec`.
//!
//! Everything about a Bedrock player's identity rests on this file staying
//! secret: it is the only thing standing between a forged handshake and total
//! impersonation.

use std::fs;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rand::TryRng as _;
use rand::rngs::SysRng;

/// The key size Geyser generates and expects: `AesKeyProducer.KEY_SIZE` is 128 bits.
pub const KEY_LENGTH: usize = 16;

/// The directory under a run directory that holds Bedrock's own files.
const BEDROCK_DIR: &str = "bedrock";

/// The file the shared key is stored under.
const KEY_FILE: &str = "key.pem";

/// Where the Floodgate shared key lives, for a given run directory.
///
/// The single definition of this path. The server (at startup), the login
/// path (through [`shared`]) and the Geyser supervisor all need the same
/// file, so this function -- not a repeated join -- is what each of them
/// calls: a second definition that drifted from this one would leave Foton
/// and its own Geyser holding different keys, and every Bedrock handshake
/// would fail to decrypt.
#[must_use]
pub fn key_path(run_directory: &Path) -> PathBuf {
    run_directory.join(BEDROCK_DIR).join(KEY_FILE)
}

/// Loads the shared key, generating it on first run.
///
/// The generated key comes from the operating system's randomness, never from a
/// seeded generator.
pub fn load_or_create(path: &Path) -> Result<[u8; KEY_LENGTH]> {
    if path.is_file() {
        let bytes = fs::read(path)?;
        return bytes.try_into().map_err(|_| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "{} is not a {KEY_LENGTH}-byte Floodgate key",
                    path.display()
                ),
            )
        });
    }

    let mut key = [0u8; KEY_LENGTH];
    SysRng
        .try_fill_bytes(&mut key)
        .map_err(|error| Error::other(format!("no system randomness: {error}")))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, key)?;
    restrict_permissions(path)?;
    Ok(key)
}

/// Makes the key readable only by the account running the server, where the
/// platform has a way to say that.
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

/// Windows inherits the directory's ACL, which is the operator's to set.
#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// The process-wide shared key, set once at server startup.
static SHARED_KEY: OnceLock<[u8; KEY_LENGTH]> = OnceLock::new();

/// Sets the process-wide shared key. The server calls this once at startup,
/// right after [`load_or_create`].
///
/// Returns `true` if this call is the one that set the key, `false` if it was
/// already set -- first call wins, later calls are no-ops rather than
/// replacing a key already in use.
pub fn init_shared(key: [u8; KEY_LENGTH]) -> bool {
    SHARED_KEY.set(key).is_ok()
}

/// The process-wide shared key, if [`init_shared`] has been called.
///
/// # Security
///
/// `None` is the safe default. It means Bedrock support was never
/// initialized, and it must mean **no Floodgate login is ever accepted**: the
/// login path is expected to treat an uninitialized key exactly like a
/// rejected handshake, never as a transient condition worth retrying.
#[must_use]
pub fn shared() -> Option<&'static [u8; KEY_LENGTH]> {
    SHARED_KEY.get()
}

#[cfg(test)]
mod tests {
    use super::{KEY_LENGTH, init_shared, key_path, load_or_create, shared};
    use std::fs::write;
    use std::path::Path;

    #[test]
    fn creates_a_key_once_and_reuses_it() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("key.pem");

        let first = load_or_create(&path).expect("creates");
        assert!(path.is_file());
        let second = load_or_create(&path).expect("loads");

        assert_eq!(first, second, "the key must survive a restart");
        assert_ne!(first, [0u8; KEY_LENGTH], "a key of zeroes is not a key");
    }

    #[test]
    fn refuses_a_key_file_of_the_wrong_size() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("key.pem");
        write(&path, b"too short").expect("writes");

        assert!(load_or_create(&path).is_err());
    }

    #[test]
    fn key_path_composes_the_documented_path() {
        let run_directory = Path::new("/srv/foton");

        assert_eq!(
            key_path(run_directory),
            run_directory.join("bedrock").join("key.pem")
        );
    }

    /// A single test covering set, read, and rejected second set: a
    /// [`OnceLock`](std::sync::OnceLock) is process-wide and persists across
    /// every test in this binary, so a second test calling `init_shared`
    /// would either race this one or silently observe a key some other test
    /// already installed.
    #[test]
    fn init_shared_is_first_call_wins() {
        let key = [7u8; KEY_LENGTH];
        let other = [9u8; KEY_LENGTH];

        assert!(init_shared(key), "the first call sets the key");
        assert_eq!(shared(), Some(&key));

        assert!(
            !init_shared(other),
            "a second call must not replace the key"
        );
        assert_eq!(shared(), Some(&key), "the original key must survive");
    }
}
