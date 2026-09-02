//! The Geyser that turns Bedrock into Java, run as a child of this server.
//!
//! Not in the JVM `foton-plugin` starts: a process gets one `JNI_CreateJavaVM`,
//! and sharing it would make Bedrock support depend on the plugin host's
//! lifecycle. A child process costs one process and buys isolation — Geyser
//! crashing restarts Geyser, and an operator who runs no Bedrock runs no JVM.
//!
//! [`render_config`]'s key names, and the section layout they live in, were
//! read from a real `config.yml` a pinned Geyser 2.11.2 build 1233 generated
//! for itself — never guessed from Geyser's own documentation, an older
//! release's schema, or memory. `design/bedrock-stage0-findings.md`, Step 3
//! and its addendum, records every key this module writes and the re-run
//! that confirmed a partially-filled config of exactly this shape starts
//! cleanly against the pinned build, with every override (including an
//! escaped MOTD) surviving intact.

use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};
use std::{env, io};

use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::{fs, task, time};
use tokio_util::sync::CancellationToken;

use crate::key;

/// The pinned Geyser release. Bumped by a person who checked that the build
/// still speaks the protocol `Cargo.toml` targets — never followed as `latest`,
/// because that turns a protocol bump into a silent outage.
pub const GEYSER_VERSION: &str = "2.11.2";
/// The pinned build number within [`GEYSER_VERSION`].
pub const GEYSER_BUILD: u32 = 1233;
/// SHA-256 of `Geyser-Standalone.jar` for the pinned build.
pub const GEYSER_SHA256: &str = "f1a4c6a5cad7ee4820b03c27cd3805680e8c06bd66ce7244f96335d83b652e0e";
/// Where the pinned jar is fetched from.
pub const GEYSER_URL: &str = "https://download.geysermc.org/v2/projects/geyser/versions/2.11.2/builds/1233/downloads/standalone";

/// The file name the pinned jar is cached under, inside the Bedrock directory.
const JAR_FILE_NAME: &str = "Geyser-Standalone.jar";

/// The lowest Java major version this pinned Geyser build runs on.
const MIN_JAVA_VERSION: u32 = 21;

/// How long a freshly (re)started Geyser must stay up before a later exit is
/// treated as a new failure streak rather than a continuation of the last one.
const MIN_HEALTHY_UPTIME: Duration = Duration::from_secs(60);
/// The restart backoff before the first retry after an unexpected exit.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// The restart backoff never grows past this.
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Consecutive restart failures the supervisor tolerates before it stops
/// trying and stays down.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;
/// How long [`Supervisor`] waits for a polite shutdown before killing Geyser.
const TERMINATE_GRACE: Duration = Duration::from_secs(10);

/// Bounded total timeout for [`download`] — covering DNS, connect, TLS and
/// the whole response body, not just a between-bytes idle window.
///
/// [`Supervisor::start`] runs synchronously during server startup, after the
/// Java port is already bound but before the accept loop runs. A download
/// with no timeout at all means a hung connection to
/// `download.geysermc.org` blocks startup forever: the Java port sits bound
/// with nothing accepting on it, which a connecting client cannot tell apart
/// from a dead server. This bound turns that into an ordinary
/// [`GeyserError::Download`] that the existing log-and-continue policy
/// handles, instead of a silent hang.
///
/// The pinned jar is roughly 10 MB; at a conservatively slow 1 Mbps
/// (~125 KB/s) connection that is about 80 seconds. 120 seconds leaves
/// headroom for connection setup and jitter on top of that while staying
/// finite. `dev/doctor.sh`'s `curl --max-time 10` is sized for a small API
/// ping, not a multi-megabyte transfer — too short a bound to reuse here.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// Why the supervisor could not start or keep running Geyser.
#[derive(Debug, Error)]
pub enum GeyserError {
    /// No `java` binary could be run at all.
    #[error(
        "no Java runtime found at {searched} ({source}); set bedrock.java_home to a JDK/JRE of Java {MIN_JAVA_VERSION} or newer, install one on the PATH, or set JAVA_HOME"
    )]
    JavaNotFound {
        /// The binary path the supervisor tried to run.
        searched: String,
        /// Why running it failed.
        #[source]
        source: io::Error,
    },
    /// `java -version` ran, but its banner did not contain a version this
    /// crate knows how to read.
    #[error("could not read a Java version from {path}: {reason}")]
    JavaVersionUnreadable {
        /// The runtime that was probed.
        path: String,
        /// The output that could not be parsed.
        reason: String,
    },
    /// A working `java` was found, but it reports a major version below
    /// [`MIN_JAVA_VERSION`].
    #[error(
        "found Java {found} at {path}, but Geyser needs Java {MIN_JAVA_VERSION} or newer; point bedrock.java_home at a newer JDK/JRE"
    )]
    JavaTooOld {
        /// The runtime that was found.
        path: String,
        /// The major version it reported.
        found: u32,
    },
    /// The pinned jar could not be downloaded — including because it did not
    /// finish within [`DOWNLOAD_TIMEOUT`].
    #[error(
        "failed to download Geyser from {url}: {source}; set bedrock.jar_path to a Geyser jar already on disk instead of relying on this download"
    )]
    Download {
        /// The URL the download was attempted from.
        url: String,
        /// The underlying HTTP error.
        #[source]
        source: reqwest::Error,
    },
    /// A jar's bytes did not match [`GEYSER_SHA256`], whether it was already
    /// on disk or freshly downloaded.
    #[error(
        "jar does not match the pinned Geyser build (expected sha256 {expected}, got {actual}); delete it and let Foton re-download the pinned build, or point bedrock.jar_path at a jar with a matching checksum"
    )]
    ChecksumMismatch {
        /// The checksum this build is pinned to.
        expected: String,
        /// What the jar's bytes actually hashed to.
        actual: String,
    },
    /// A file under the Bedrock directory could not be read or written.
    #[error("failed to access {path}: {source}")]
    Io {
        /// The file that could not be read or written.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Starting the `java` process itself failed.
    #[error("failed to start Geyser ({java} -jar {jar}): {source}")]
    Spawn {
        /// The Java binary that could not be run.
        java: String,
        /// The jar that could not be run.
        jar: String,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },
}

/// Everything the supervisor needs to run this server's Geyser.
///
/// A caller (the server, at startup) builds one of these from
/// [`BedrockConfig`](crate::config::BedrockConfig) and passes it to
/// [`Supervisor::start`].
#[derive(Debug, Clone)]
pub struct GeyserOptions {
    /// This server's own run directory; `bedrock/` under it holds Geyser's
    /// jar, its generated `config.yml`, and the shared Floodgate key.
    pub run_directory: PathBuf,
    /// The UDP port Bedrock clients connect to.
    ///
    /// Already resolved — a caller building this from
    /// [`BedrockConfig`](crate::config::BedrockConfig) must call
    /// [`BedrockConfig::resolved_port`](crate::config::BedrockConfig::resolved_port)
    /// first. Unlike [`BedrockConfig::port`](crate::config::BedrockConfig::port),
    /// `0` here has no special meaning; it is simply not a usable port.
    pub bedrock_port: u16,
    /// This Java server's own port, which Geyser is told to connect to on
    /// loopback.
    pub java_port: u16,
    /// What Bedrock clients see in the server list; empty reuses the Java
    /// server's own MOTD instead (see [`render_config`]).
    pub motd: String,
    /// Prepended to a Bedrock player's gamertag on the Java side.
    ///
    /// Not written into Geyser's own config — Geyser has no concept of a
    /// username prefix. Carried here so `GeyserOptions` mirrors
    /// [`BedrockConfig`](crate::config::BedrockConfig) in full, and so a
    /// caller building the login path's options and the supervisor's from
    /// one place cannot let the two drift.
    pub username_prefix: String,
    /// A JDK/JRE to run Geyser with; `None` falls back to `JAVA_HOME`, then
    /// `java` on the `PATH`.
    pub java_home: Option<PathBuf>,
    /// An operator-supplied Geyser jar; `None` fetches and caches the pinned
    /// build.
    pub jar_path: Option<PathBuf>,
}

/// Escapes a string into a YAML double-quoted scalar, backslash first so the
/// escape for `"` is not itself re-escaped.
///
/// Every operator-supplied string that reaches [`render_config`] — today,
/// only the MOTD — goes through this. A config file is a parser, and an
/// unescaped MOTD is exactly how operator input reaching a parser adds a key
/// instead of setting one: an embedded `"` closes the scalar early, and
/// anything after it is read as new YAML.
fn yaml_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

/// Renders Geyser's `config.yml`, from scratch, every time.
///
/// Foton owns this file: nothing here reads an existing `config.yml` back or
/// tries to preserve edits Geyser made to it (Geyser rewrites `metrics-uuid`,
/// migrates `config-version`, and fills in every key this function leaves
/// out with its own defaults — confirmed by running the pinned build against
/// a config of exactly this shape, `design/bedrock-stage0-findings.md`'s
/// Step 3 addendum). Every [`Supervisor::start`] regenerates
/// the whole file from `options`, so the operator's actual settings are the
/// only source of truth, never whatever the previous Geyser process happened
/// to leave on disk.
///
/// An empty [`GeyserOptions::motd`] emits `passthrough-motd: true` and no
/// `primary-motd` at all, so Geyser relays the Java server's own MOTD to
/// Bedrock clients — the mechanism
/// [`BedrockConfig::motd`](crate::config::BedrockConfig::motd)'s own doc
/// comment promises ("empty reuses the server MOTD"). A non-empty one emits
/// `passthrough-motd: false` and the quoted MOTD, so Geyser shows it instead.
#[must_use]
pub fn render_config(options: &GeyserOptions) -> String {
    let key_file = key::key_path(&options.run_directory);
    let motd_section = if options.motd.is_empty() {
        "motd:\n  passthrough-motd: true\n".to_owned()
    } else {
        format!(
            "motd:\n  primary-motd: {motd}\n  passthrough-motd: false\n",
            motd = yaml_quote(&options.motd),
        )
    };

    format!(
        "\
bedrock:
  address: 0.0.0.0
  port: {bedrock_port}
java:
  address: 127.0.0.1
  port: {java_port}
  auth-type: floodgate
  forward-hostname: false
{motd_section}advanced:
  floodgate-key-file: {key_file}
  bedrock:
    validate-bedrock-login: true
",
        bedrock_port = options.bedrock_port,
        java_port = options.java_port,
        key_file = yaml_quote(&key_file.display().to_string()),
    )
}

/// Renders `bytes` as lowercase hex.
///
/// A hand-rolled loop rather than a `LowerHex` format: `sha2`'s digest output
/// is a fixed-size array type from the `hybrid-array`/`crypto-common` stack
/// that does not implement it, and pulling in the `hex` crate for one call
/// site is not worth a sixth dependency this module's brief did not ask for.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // `Write for String` never fails.
        let _ignored = write!(hex, "{byte:02x}");
    }
    hex
}

/// Hashes `bytes` with SHA-256 and compares the digest against `expected`,
/// case-insensitively — a checksum pasted from a webpage may be either case.
///
/// # Errors
///
/// Returns [`GeyserError::ChecksumMismatch`] if the digests differ.
pub fn verify_checksum(bytes: &[u8], expected: &str) -> Result<(), GeyserError> {
    let digest = Sha256::digest(bytes);
    let actual = to_hex(&digest);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(GeyserError::ChecksumMismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}

/// The `java` executable's conventional name under a JDK/JRE's `bin/`.
const fn java_binary_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// Resolves which `java` binary to run: `java_home` if given, else
/// `JAVA_HOME`, else a bare name resolved against `PATH`.
fn java_binary_path(java_home: Option<&Path>) -> PathBuf {
    let home = java_home
        .map(Path::to_path_buf)
        .or_else(|| env::var_os("JAVA_HOME").map(PathBuf::from));
    match home {
        Some(home) => home.join("bin").join(java_binary_name()),
        None => PathBuf::from(java_binary_name()),
    }
}

/// Reads the major version number out of `java -version`'s banner line, e.g.
/// `openjdk version "21.0.5" 2024-10-15`, or the legacy `java version
/// "1.8.0_292"` form, where the real major version is the second component.
fn parse_java_major_version(banner: &str) -> Option<u32> {
    let quoted = banner.split('"').nth(1)?;
    let mut parts = quoted.split(['.', '-', '_', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Resolves and validates the Java runtime Geyser will run under.
async fn resolve_java(java_home: Option<&Path>) -> Result<PathBuf, GeyserError> {
    let binary = java_binary_path(java_home);

    let output = Command::new(&binary)
        .arg("-version")
        .output()
        .await
        .map_err(|source| GeyserError::JavaNotFound {
            searched: binary.display().to_string(),
            source,
        })?;

    // `java -version` writes its banner to stderr, not stdout.
    let banner = String::from_utf8_lossy(&output.stderr);
    let major =
        parse_java_major_version(&banner).ok_or_else(|| GeyserError::JavaVersionUnreadable {
            path: binary.display().to_string(),
            reason: banner.trim().to_owned(),
        })?;

    if major < MIN_JAVA_VERSION {
        return Err(GeyserError::JavaTooOld {
            path: binary.display().to_string(),
            found: major,
        });
    }

    Ok(binary)
}

/// Downloads `url` whole into memory, aborting the whole attempt — DNS,
/// connect, TLS and body included — if it has not finished within `timeout`.
/// The pinned jar is tens of megabytes, so buffering it rather than
/// streaming to disk keeps the checksum check — which must happen before
/// anything is written — simple.
///
/// A dedicated [`reqwest::Client`] is built for this rather than the
/// crate-wide convenience of `reqwest::get`, because that bare function uses
/// a lazily-initialized client with no timeout at all — exactly the hang
/// this bound exists to prevent.
async fn download(url: &str, timeout: Duration) -> Result<Vec<u8>, GeyserError> {
    let wrap = |source: reqwest::Error| GeyserError::Download {
        url: url.to_owned(),
        source,
    };
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(wrap)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(wrap)?
        .error_for_status()
        .map_err(wrap)?;
    let bytes = response.bytes().await.map_err(wrap)?;
    Ok(bytes.to_vec())
}

/// Resolves the Geyser jar: an operator-supplied one, or the pinned build
/// cached under `bedrock_dir`, downloading it first if absent. Either way,
/// the bytes are checked against [`GEYSER_SHA256`] before this returns.
async fn resolve_jar(options: &GeyserOptions, bedrock_dir: &Path) -> Result<PathBuf, GeyserError> {
    let jar_path = options
        .jar_path
        .clone()
        .unwrap_or_else(|| bedrock_dir.join(JAR_FILE_NAME));

    if jar_path.is_file() {
        let bytes = fs::read(&jar_path)
            .await
            .map_err(|source| GeyserError::Io {
                path: jar_path.clone(),
                source,
            })?;
        verify_checksum(&bytes, GEYSER_SHA256)?;
        return Ok(jar_path);
    }

    tracing::info!(
        target: "geyser",
        "downloading Geyser {GEYSER_VERSION} build {GEYSER_BUILD} from {GEYSER_URL}"
    );
    let bytes = download(GEYSER_URL, DOWNLOAD_TIMEOUT).await?;
    verify_checksum(&bytes, GEYSER_SHA256)?;
    fs::write(&jar_path, &bytes)
        .await
        .map_err(|source| GeyserError::Io {
            path: jar_path.clone(),
            source,
        })?;
    Ok(jar_path)
}

/// Writes `config.yml` and ensures the shared Floodgate key exists, so both
/// files are in place before Geyser's first start — a key created only after
/// Geyser has already read a missing one fails silently from an operator's
/// point of view (`design/bedrock-stage0-findings.md`, Step 3).
async fn write_config_and_key(
    options: &GeyserOptions,
    bedrock_dir: &Path,
) -> Result<(), GeyserError> {
    let config_path = bedrock_dir.join("config.yml");
    let yaml = render_config(options);
    fs::write(&config_path, yaml)
        .await
        .map_err(|source| GeyserError::Io {
            path: config_path,
            source,
        })?;

    let key_path = key::key_path(&options.run_directory);
    let key_path_for_blocking = key_path.clone();
    // `key::load_or_create` is ordinary blocking `std::fs`, shared with the
    // server's own startup path — run it off the async executor rather than
    // duplicating it as an async version for this one caller.
    task::spawn_blocking(move || key::load_or_create(&key_path_for_blocking))
        .await
        .map_err(|join_error| GeyserError::Io {
            path: key_path.clone(),
            source: io::Error::other(join_error),
        })?
        .map_err(|source| GeyserError::Io {
            path: key_path,
            source,
        })?;

    Ok(())
}

/// Spawns `java -jar <jar_path>`, with `bedrock_dir` as its working
/// directory (so it reads and writes `config.yml`, `key.pem`, its cache and
/// its logs there) and its stdout/stderr piped for [`relay_child_logs`].
///
/// Not `async`: `Command::spawn` starts the process and returns immediately,
/// it does not wait on it.
fn spawn_geyser(
    java_binary: &Path,
    jar_path: &Path,
    bedrock_dir: &Path,
) -> Result<Child, GeyserError> {
    Command::new(java_binary)
        .arg("-jar")
        .arg(jar_path)
        .current_dir(bedrock_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| GeyserError::Spawn {
            java: java_binary.display().to_string(),
            jar: jar_path.display().to_string(),
            source,
        })
}

/// Reads Geyser's own leading level marker out of a console line, e.g. the
/// `INFO` in `[12:00:56 INFO] Started Geyser on UDP port 19132`, and maps it
/// onto a `tracing` level. Unrecognized or missing markers default to `info`.
fn geyser_log_level(line: &str) -> tracing::Level {
    let marker = line.split(']').next().unwrap_or(line).to_ascii_uppercase();
    if marker.contains("ERROR") || marker.contains("SEVERE") {
        tracing::Level::ERROR
    } else if marker.contains("WARN") {
        tracing::Level::WARN
    } else if marker.contains("DEBUG") || marker.contains("FINE") || marker.contains("TRACE") {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    }
}

/// Re-emits one line of Geyser's console through `tracing`, under
/// `target: "geyser"`.
fn emit_geyser_line(line: &str) {
    match geyser_log_level(line) {
        tracing::Level::ERROR => tracing::error!(target: "geyser", "{line}"),
        tracing::Level::WARN => tracing::warn!(target: "geyser", "{line}"),
        tracing::Level::DEBUG | tracing::Level::TRACE => {
            tracing::debug!(target: "geyser", "{line}");
        }
        tracing::Level::INFO => tracing::info!(target: "geyser", "{line}"),
    }
}

/// Reads `stream` line by line until it closes or errors, relaying each line
/// through [`emit_geyser_line`].
async fn relay_stream<R>(stream: R)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(stream).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => emit_geyser_line(&line),
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(target: "geyser", "Geyser's log stream ended unexpectedly: {error}");
                return;
            }
        }
    }
}

/// Spawns one relay task per stream Geyser writes to, taking ownership of
/// each handle so this can only be called once per child.
fn relay_child_logs(child: &mut Child) {
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(relay_stream(stdout));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(relay_stream(stderr));
    }
}

/// Sends the platform's polite termination request, best-effort.
///
/// On Unix this is `SIGTERM`, which lets the JVM run its shutdown hooks — the
/// `kill` binary is used for the one syscall rather than adding `libc`/`nix`
/// as a dependency for it. Windows has no reachable equivalent without extra
/// platform bindings this crate does not otherwise need; [`terminate`]'s
/// timeout and subsequent [`Child::kill`] still apply there.
#[cfg(unix)]
async fn request_graceful_stop(child: &Child) {
    let Some(pid) = child.id() else {
        return;
    };
    let _ignored = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .await;
}

/// See the Unix implementation's documentation.
#[cfg(not(unix))]
async fn request_graceful_stop(_child: &Child) {}

/// Stops `child`: a polite termination request, up to [`TERMINATE_GRACE`] to
/// take effect, then an unconditional kill if it hasn't.
async fn terminate(child: &mut Child) {
    request_graceful_stop(child).await;

    if time::timeout(TERMINATE_GRACE, child.wait()).await.is_err() {
        tracing::warn!(
            target: "geyser",
            "Geyser did not stop within {TERMINATE_GRACE:?}; killing it"
        );
        let _ignored = child.kill().await;
        let _ignored = child.wait().await;
    }
}

/// Logs why a Geyser process just exited.
fn log_unexpected_exit(status: &Result<ExitStatus, io::Error>) {
    match status {
        Ok(status) => {
            tracing::warn!(target: "geyser", "Geyser exited unexpectedly ({status}); restarting");
        }
        Err(error) => {
            tracing::warn!(target: "geyser", "lost track of the Geyser process ({error}); restarting");
        }
    }
}

/// Waits out the current backoff (or returns `None` if cancelled first),
/// then tries to spawn a fresh Geyser, retrying under the same backoff and
/// give-up policy if the spawn itself fails.
///
/// Returns `None` once [`MAX_CONSECUTIVE_FAILURES`] is reached without a
/// successful spawn, or if `cancel` fires while waiting — either way, the
/// caller has nothing left to supervise.
async fn respawn(
    java_binary: &Path,
    jar_path: &Path,
    bedrock_dir: &Path,
    cancel: &CancellationToken,
    backoff: &mut Duration,
    consecutive_failures: &mut u32,
) -> Option<Child> {
    loop {
        *consecutive_failures += 1;
        if *consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            tracing::error!(
                target: "geyser",
                "Geyser has failed {consecutive_failures} times in a row; not restarting it again",
                consecutive_failures = *consecutive_failures
            );
            return None;
        }

        tracing::info!(
            target: "geyser",
            "restarting Geyser in {backoff:?} (attempt {consecutive_failures})",
            backoff = *backoff,
            consecutive_failures = *consecutive_failures
        );
        tokio::select! {
            () = time::sleep(*backoff) => {}
            () = cancel.cancelled() => return None,
        }
        *backoff = (*backoff * 2).min(MAX_BACKOFF);

        match spawn_geyser(java_binary, jar_path, bedrock_dir) {
            Ok(mut child) => {
                relay_child_logs(&mut child);
                return Some(child);
            }
            Err(error) => {
                tracing::error!(target: "geyser", "failed to restart Geyser: {error}");
            }
        }
    }
}

/// Supervises an already-running Geyser `child` until `cancel` fires: waits
/// for it to exit, restarts it with backoff, and gives up rather than
/// spinning forever if it keeps failing immediately.
async fn supervise(
    java_binary: PathBuf,
    jar_path: PathBuf,
    bedrock_dir: PathBuf,
    mut child: Child,
    cancel: CancellationToken,
) {
    let mut backoff = INITIAL_BACKOFF;
    let mut consecutive_failures: u32 = 0;

    loop {
        let started_at = Instant::now();
        let exited = tokio::select! {
            status = child.wait() => Some(status),
            () = cancel.cancelled() => None,
        };

        let Some(status) = exited else {
            terminate(&mut child).await;
            return;
        };

        log_unexpected_exit(&status);

        if started_at.elapsed() >= MIN_HEALTHY_UPTIME {
            consecutive_failures = 0;
            backoff = INITIAL_BACKOFF;
        }

        let Some(next) = respawn(
            &java_binary,
            &jar_path,
            &bedrock_dir,
            &cancel,
            &mut backoff,
            &mut consecutive_failures,
        )
        .await
        else {
            return;
        };
        child = next;
    }
}

/// Runs and supervises this server's Geyser process.
///
/// Dropping a `Supervisor` does not stop Geyser — the supervision task keeps
/// running until its `CancellationToken` is cancelled. Cancel it and then
/// call [`Supervisor::wait`] for an orderly shutdown.
pub struct Supervisor {
    handle: task::JoinHandle<()>,
}

impl Supervisor {
    /// Resolves Java, fetches and verifies the pinned jar if needed, writes
    /// `config.yml` and the shared key, starts Geyser, and returns once it
    /// is running. From here on, keeping it running is a background task's
    /// job, not the caller's — see the module documentation.
    ///
    /// # Errors
    ///
    /// Returns [`GeyserError`] if no Java [`MIN_JAVA_VERSION`]+ runtime can
    /// be found, the pinned jar cannot be fetched or does not match its
    /// checksum, the Bedrock directory's files cannot be written, or the
    /// process itself cannot be spawned. None of these are retried here —
    /// only a process that started successfully and later exits is restarted
    /// automatically.
    pub async fn start(
        options: GeyserOptions,
        cancel: CancellationToken,
    ) -> Result<Self, GeyserError> {
        let bedrock_dir = options.run_directory.join("bedrock");
        fs::create_dir_all(&bedrock_dir)
            .await
            .map_err(|source| GeyserError::Io {
                path: bedrock_dir.clone(),
                source,
            })?;

        let java_binary = resolve_java(options.java_home.as_deref()).await?;
        let jar_path = resolve_jar(&options, &bedrock_dir).await?;
        write_config_and_key(&options, &bedrock_dir).await?;

        let mut child = spawn_geyser(&java_binary, &jar_path, &bedrock_dir)?;
        relay_child_logs(&mut child);

        let handle = tokio::spawn(supervise(java_binary, jar_path, bedrock_dir, child, cancel));

        Ok(Self { handle })
    }

    /// Waits for the supervisor to stop — after its `CancellationToken` is
    /// cancelled, or it gives up after too many crashes in a row.
    pub async fn wait(self) {
        // The task only ever ends by returning; nothing here aborts it, so a
        // join error is not a case this needs to report.
        let _ignored = self.handle.await;
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::path::PathBuf;
    use std::time::Duration;

    use tokio::net::TcpListener;

    use super::{GeyserError, GeyserOptions, download, render_config, verify_checksum};
    use crate::key;

    fn options() -> GeyserOptions {
        GeyserOptions {
            run_directory: PathBuf::from("/tmp/run"),
            bedrock_port: 19132,
            java_port: 25565,
            motd: "A Foton server".to_owned(),
            username_prefix: ".".to_owned(),
            java_home: None,
            jar_path: None,
        }
    }

    #[test]
    fn the_generated_config_points_geyser_at_this_server() {
        let yaml = render_config(&options());
        assert!(yaml.contains("port: 19132"));
        assert!(yaml.contains("address: 127.0.0.1"));
        assert!(yaml.contains("port: 25565"));
        assert!(yaml.contains("auth-type: floodgate"));
    }

    #[test]
    fn a_non_empty_motd_is_shown_instead_of_the_java_servers_own() {
        // `options()`'s default motd, "A Foton server", is already non-empty.
        let yaml = render_config(&options());
        assert!(yaml.contains(r#"primary-motd: "A Foton server""#));
        assert!(yaml.contains("passthrough-motd: false"));
    }

    #[test]
    fn an_empty_motd_reuses_the_java_servers_own() {
        // `BedrockConfig::motd`'s own doc comment promises this: "empty
        // reuses the server MOTD". `passthrough-motd: true` is Geyser's own
        // mechanism for it (`design/bedrock-stage0-findings.md`'s Step 3
        // addendum) -- and `primary-motd` must be absent, not merely empty,
        // since an empty `primary-motd` would fall back to Geyser's own
        // literal "Geyser" default rather than to this server's MOTD.
        let mut options = options();
        options.motd = String::new();
        let yaml = render_config(&options);
        assert!(yaml.contains("passthrough-motd: true"));
        assert!(!yaml.contains("primary-motd"));
    }

    #[test]
    fn the_generated_config_quotes_a_motd_that_would_break_yaml() {
        let mut options = options();
        options.motd = "Foton: now with #tags & \"quotes\"".to_owned();
        let yaml = render_config(&options);
        // A MOTD is operator input reaching a config parser. It must not be
        // able to add keys.
        assert!(!yaml.contains("\nnow with"));
        assert!(yaml.contains(r#""Foton: now with #tags & \"quotes\""#));
    }

    #[test]
    fn the_generated_config_escapes_a_motd_containing_a_backslash() {
        let mut options = options();
        options.motd = r"Foton: C:\Users\Test".to_owned();
        let yaml = render_config(&options);
        assert!(yaml.contains(r#""Foton: C:\\Users\\Test""#));
    }

    #[test]
    fn the_generated_config_escapes_a_motd_ending_in_a_backslash() {
        // The case the escaping exists to prevent: an unescaped trailing `\`
        // would combine with the `"` this function appends to close the
        // scalar into `\"`, an escaped quote rather than the end of the
        // string -- leaving everything after it, including the rest of the
        // file, inside one unterminated value.
        let mut options = options();
        options.motd = r"Foton\".to_owned();
        let yaml = render_config(&options);
        assert!(yaml.contains(r#""Foton\\""#));
    }

    #[test]
    fn the_generated_config_escapes_a_motd_containing_a_newline() {
        let mut options = options();
        options.motd = "Foton\nsecond line".to_owned();
        let yaml = render_config(&options);
        // Escaped to the two-character sequence `\n`, not left as a raw
        // newline byte -- YAML permits an unescaped literal line break
        // inside a double-quoted scalar (it folds rather than ending the
        // string), so only checking this doesn't break parsing wouldn't
        // prove the escape happened at all.
        assert!(yaml.contains(r#""Foton\nsecond line""#));
        assert!(!yaml.contains("Foton\nsecond line"));
    }

    #[test]
    fn the_generated_config_points_floodgate_at_this_servers_key() {
        // `key::key_path` is the single definition of where the shared key
        // lives; a second, hand-rolled path here would drift from it
        // silently. The generated config must point at exactly that file.
        let yaml = render_config(&options());
        let expected = key::key_path(&options().run_directory);
        assert!(yaml.contains(&expected.display().to_string()));
    }

    #[test]
    fn a_checksum_mismatch_is_refused() {
        assert!(verify_checksum(b"not the jar", super::GEYSER_SHA256).is_err());
    }

    #[test]
    fn a_matching_checksum_is_accepted() {
        // sha256("") — proves the comparison works, without a 40 MB fixture.
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(verify_checksum(b"", empty).is_ok());
    }

    #[test]
    fn a_checksum_is_accepted_case_insensitively() {
        let empty_uppercase = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
        assert!(verify_checksum(b"", empty_uppercase).is_ok());
    }

    #[test]
    fn reads_a_modern_java_version_banner() {
        let banner = "openjdk version \"21.0.5\" 2024-10-15\nOpenJDK Runtime Environment\n";
        assert_eq!(super::parse_java_major_version(banner), Some(21));
    }

    #[test]
    fn reads_a_legacy_java_version_banner() {
        // Pre-Java-9 versioning: the real major version is the second
        // component, not the first. A parser that just took the first
        // component would accept Java 1.8 as if it were Java 1 -- and never
        // reject it as too old.
        let banner = "java version \"1.8.0_292\"\nJava(TM) SE Runtime Environment\n";
        assert_eq!(super::parse_java_major_version(banner), Some(8));
    }

    #[test]
    fn rejects_a_banner_with_no_version() {
        assert_eq!(super::parse_java_major_version("command not found"), None);
    }

    #[tokio::test]
    async fn a_hung_download_times_out_instead_of_hanging_forever() {
        // A listener that completes the TCP handshake and then never writes
        // a byte back -- the same shape as a hung connection to
        // `download.geysermc.org`, entirely on loopback so this needs no
        // real network access.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("binding to loopback on an OS-assigned port should succeed");
        let address = listener
            .local_addr()
            .expect("a bound listener has a local address");
        tokio::spawn(async move {
            let Ok((socket, _peer)) = listener.accept().await else {
                return;
            };
            // Hold the connection open and silent forever; the test's own
            // timeout, not this task, is what ends it.
            let _held_open = socket;
            pending::<()>().await;
        });

        let url = format!("http://{address}/geyser.jar");
        let result = download(&url, Duration::from_millis(200)).await;

        match result {
            Err(GeyserError::Download { source, .. }) => {
                assert!(
                    source.is_timeout(),
                    "expected a timeout error, got: {source:?}"
                );
            }
            other => panic!("expected a GeyserError::Download from a timeout, got: {other:?}"),
        }
    }
}
