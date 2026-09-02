//! The secret Foton and its Geyser share.
//!
//! Sixteen raw bytes, which is what Geyser's `AesKeyProducer.produceFrom` reads
//! a key file as. Not PEM despite the conventional name -- Geyser reads the
//! file whole and hands it to `SecretKeySpec`.
//!
//! Everything about a Bedrock player's identity rests on this file staying
//! secret: it is the only thing standing between a forged handshake and total
//! impersonation.

use std::fs::{self, File, OpenOptions};
use std::io::{Error, ErrorKind, Result, Write as _};
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
        return read_key(path);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut key = [0u8; KEY_LENGTH];
    SysRng
        .try_fill_bytes(&mut key)
        .map_err(|error| Error::other(format!("no system randomness: {error}")))?;

    match create_restricted(path) {
        Ok(mut file) => {
            file.write_all(&key)?;
            Ok(key)
        }
        // Another process created the key first, in the window between the
        // `is_file` check above and this call. `create_new` (inside
        // `create_restricted`) is what makes that detectable at all instead
        // of silently overwritten: the old `fs::write` here let both
        // processes "win", each returning the random bytes *it* generated --
        // bytes that might not be what ends up on disk once the other
        // process's write lands. Geyser reads the key file directly, not
        // this process's copy, so a process holding a key that doesn't match
        // the file would fail every Floodgate handshake with no clear
        // reason why. Reading back what the other process wrote, rather
        // than erroring out, is what keeps every process -- and Geyser --
        // agreeing on one key.
        //
        // This still has a narrow gap: a reader landing between the
        // winner's `create_restricted` and its `write_all` would see a
        // 0-byte file and get `read_key`'s "wrong size" error rather than
        // the key. Closing that fully would mean writing to a sibling
        // temp file and renaming it into place, which trades this gap for
        // last-rename-wins silently discarding the loser's own generated
        // bytes instead -- a different failure mode, not obviously a
        // better one. Left as is because two Foton processes racing to
        // initialize the same run directory's key is already a
        // misconfiguration outside what this function can make safe, and
        // the window is one small, uninterrupted `write` syscall wide.
        Err(error) if error.kind() == ErrorKind::AlreadyExists => read_key(path),
        Err(error) => Err(error),
    }
}

/// Reads and validates an existing key file.
fn read_key(path: &Path) -> Result<[u8; KEY_LENGTH]> {
    let bytes = fs::read(path)?;
    bytes.try_into().map_err(|_| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "{} is not a {KEY_LENGTH}-byte Floodgate key",
                path.display()
            ),
        )
    })
}

/// Creates `path` exclusively, with the mode restricted to owner
/// read/write from the moment it exists, where the platform has a way to
/// say that at creation time.
///
/// The mode is set *at creation* (`OpenOptionsExt::mode`, not a later
/// `set_permissions`) deliberately: applying it as a second step after
/// `fs::write` already created the file leaves a window where the file
/// exists at whatever the process umask allows -- commonly `0o644`, world
/// readable -- and what is readable through that window is the key that
/// authenticates every Bedrock player's identity to this server. There is
/// no second factor behind it, so that window is worth closing even though
/// it is normally short.
///
/// `create_new` rather than plain `create`: it fails with
/// [`ErrorKind::AlreadyExists`] instead of silently truncating a file
/// another process just created, which is what lets [`load_or_create`]
/// detect and recover from a concurrent first run instead of the two
/// processes fighting over which one's key ends up on disk.
#[cfg(unix)]
fn create_restricted(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// Windows has no mode to set at creation; the directory's ACL, which is
/// the operator's to set, is what governs instead. `create_new` is kept
/// even here so both platforms detect the same concurrent-creation race
/// (see [`load_or_create`]'s `AlreadyExists` handling), even though this
/// branch cannot also close the Unix permission window.
#[cfg(not(unix))]
fn create_restricted(path: &Path) -> Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
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

    /// Unix-only: permission bits are a POSIX mode, which Windows does not
    /// have -- there, the governing control is the directory's ACL, which
    /// this crate never touches (see `create_restricted`'s non-Unix
    /// doc comment), so there is nothing analogous for a Windows-only
    /// variant of this test to assert.
    #[cfg(unix)]
    #[test]
    fn the_created_key_file_is_readable_only_by_its_owner() {
        use std::fs::metadata;
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("key.pem");

        load_or_create(&path).expect("creates");

        let mode = metadata(&path)
            .expect("the freshly created file has metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "the key file must be created non-world-readable, not merely fixed up afterwards"
        );
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
