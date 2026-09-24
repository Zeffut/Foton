//! # Foton
//!
//! The main library for the Foton Minecraft server.

// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. The lint
// is scoped to `not(test)` on purpose -- a panicking test is how a test reports
// a failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic, clippy::unreachable, clippy::todo))]

use std::{
    env,
    error::Error,
    fmt, fs, io,
    io::Read,
    iter,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    path::{Path, PathBuf, absolute},
    sync::{Arc, OnceLock, Weak},
    thread::Builder,
};

use foton_bedrock::config::BedrockConfig;
use foton_bedrock::geyser::{GeyserOptions, Supervisor};
use foton_bedrock::key;
use foton_core::{command::CommandRegistry, permission::PermissionGroupManager, server::Server};
use foton_login::{JavaTcpClient, ServerConnectionSession};
use foton_plugin::{PluginHost, PluginHostConfig};
use foton_utils::locks::SyncMutex;
use rustc_hash::FxHashMap;
use sha2::{Digest, Sha256};
use tokio::{net::TcpListener, runtime::Runtime, select};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

/// Command-line arguments.
pub mod args;
/// Server configuration module.
pub mod config;
/// A module for logging utilities.
pub mod logger;
/// Remote administration over the Source Rcon protocol.
pub mod rcon;

/// Static access to the server
pub static SERVER: OnceLock<Arc<Server>> = OnceLock::new();

/// Concurrent pre-play sockets one source address may retain.
///
/// The permit is released at the play handoff, so this protects login
/// resources without imposing a player cap on shared NAT, proxy or Geyser
/// addresses.
const MAX_PRE_PLAY_CONNECTIONS_PER_IP: usize = 32;

/// `ViaBackwards` registers its complete protocol graph recursively during
/// `onLoad`. That synchronous Java bootstrap exceeds the platform default
/// native-thread stack before a connection can exist, so it runs once on a
/// deliberately sized thread rather than on Tokio's runtime thread.
const PLUGIN_BOOTSTRAP_STACK_SIZE: usize = 32 * 1024 * 1024;

struct SourceConnectionLimiter {
    limit: usize,
    counts: SyncMutex<FxHashMap<IpAddr, usize>>,
}

impl SourceConnectionLimiter {
    fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit,
            counts: SyncMutex::new(FxHashMap::default()),
        })
    }

    fn try_acquire(self: &Arc<Self>, source: IpAddr) -> Option<SourceConnectionPermit> {
        let mut counts = self.counts.lock();
        let count = counts.entry(source).or_default();
        if *count >= self.limit {
            return None;
        }
        *count += 1;
        Some(SourceConnectionPermit {
            limiter: Arc::clone(self),
            source,
        })
    }

    #[cfg(test)]
    fn tracked_sources(&self) -> usize {
        self.counts.lock().len()
    }
}

struct SourceConnectionPermit {
    limiter: Arc<SourceConnectionLimiter>,
    source: IpAddr,
}

impl Drop for SourceConnectionPermit {
    fn drop(&mut self) {
        let mut counts = self.limiter.counts.lock();
        let Some(count) = counts.get_mut(&self.source) else {
            return;
        };
        if *count <= 1 {
            let _ = counts.remove(&self.source);
        } else {
            *count -= 1;
        }
    }
}

/// The main server struct.
pub struct FotonServer {
    /// The TCP listener for incoming connections.
    pub tcp_listener: TcpListener,
    /// The cancellation token for graceful shutdown.
    pub cancel_token: CancellationToken,
    /// The next client ID to be assigned.
    pub client_id: u64,
    /// The shared server state.
    pub server: Arc<Server>,
    /// Session id UUID state
    pub connection_session: Arc<ServerConnectionSession>,
    /// The bound Rcon port, when remote administration is enabled.
    pub rcon_listener: Option<rcon::RconListener>,
    /// Bedrock policy handed to every accepted connection.
    ///
    /// Carried here rather than read per-connection so that what the login
    /// path enforces is fixed at startup, beside the key it was loaded with.
    pub bedrock: BedrockConfig,
    /// The running Geyser supervisor, when Bedrock support started successfully.
    ///
    /// `None` covers both "Bedrock is disabled" and "Geyser failed to start" —
    /// either way there is nothing to hand to [`FotonServer::start`]. A
    /// failure here never stops the Java server: see the log message where
    /// this is set.
    pub bedrock_supervisor: Option<Supervisor>,
    /// Java plugin host, enabled only when `FOTON_PLUGIN_DIRECTORY` is set.
    plugin_host: Option<PluginHost>,
}

/// Startup error for expected operational failures.
#[derive(Debug)]
pub enum FotonServerError {
    /// Core server startup failed.
    Core(String),
    /// TCP listener could not bind.
    Bind {
        /// Server port that failed to bind.
        port: u16,
        /// Underlying IO error.
        source: io::Error,
    },
    /// Plugin host could not be started or loaded.
    Plugin(String),
    /// The Rcon listener could not bind.
    RconBind {
        /// Complete configured Rcon socket address that failed to bind.
        address: SocketAddr,
        /// Underlying IO error.
        source: io::Error,
    },
}

impl fmt::Display for FotonServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => f.write_str(error),
            Self::Bind { port, source } => {
                write!(f, "failed to bind to server port {port}: {source}")
            }
            Self::Plugin(error) => write!(f, "plugin host failed: {error}"),
            Self::RconBind { address, source } => {
                write!(f, "failed to bind rcon to {address}: {source}")
            }
        }
    }
}

impl Error for FotonServerError {}

impl FotonServer {
    /// Creates a new Foton server.
    ///
    pub async fn new(
        chunk_runtime: Arc<Runtime>,
        cancel_token: CancellationToken,
        foton_config: config::FotonConfig,
    ) -> Result<Self, FotonServerError> {
        Self::new_with_commands(
            chunk_runtime,
            cancel_token,
            foton_config,
            CommandRegistry::new(),
        )
        .await
    }

    /// Creates a new Foton server with additional commands registered atomically at startup.
    #[expect(
        clippy::too_many_lines,
        reason = "one straight assembly of a server -- worlds, then registries, \n                  then the plugin host -- where each step needs what the last one \n                  built; cutting it up would only hide the order"
    )]
    pub async fn new_with_commands(
        chunk_runtime: Arc<Runtime>,
        cancel_token: CancellationToken,
        foton_config: config::FotonConfig,
        command_registry: CommandRegistry,
    ) -> Result<Self, FotonServerError> {
        log::info!("Starting Foton Server");

        let permission_group_store = foton_config.permission_group_store();
        let server_port = foton_config.server.server_port;
        let rcon_config = foton_config.server.rcon.clone();
        let bedrock_config = foton_config.server.bedrock.clone();
        // Captured here, ahead of `into_runtime_config`'s move below, purely
        // so the Bedrock startup warnings further down can still read them.
        let online_mode = foton_config.server.online_mode;
        let enforce_secure_chat = foton_config.server.enforce_secure_chat;
        let worlds_config = foton_config.worlds;
        let permission_groups =
            PermissionGroupManager::new(foton_config.groups, permission_group_store).map_err(
                |error| {
                    FotonServerError::Core(format!("failed to validate groups config: {error}"))
                },
            )?;
        let runtime_config = foton_config
            .server
            .into_runtime_config()
            .map_err(FotonServerError::Core)?;

        // Bukkit's onLoad phase must run before core publishes immutable
        // registries. The host is started without a server binding; native
        // gameplay calls are bound only after Server::new_with_commands has
        // completed its registry bootstrap.
        let mut plugin_host = match env::var_os("FOTON_PLUGIN_DIRECTORY") {
            None => None,
            Some(plugin_directory) => {
                let plugin_directory = PathBuf::from(plugin_directory);
                let java_home = env::var_os("FOTON_JAVA_HOME").ok_or_else(|| {
                    FotonServerError::Plugin(
                        "FOTON_JAVA_HOME is required when FOTON_PLUGIN_DIRECTORY is set".to_owned(),
                    )
                })?;
                let explicit_api = env::var_os("FOTON_PLUGIN_API_JAR").map(PathBuf::from);
                let explicit_libraries =
                    env::var_os("FOTON_PLUGIN_LIBRARY_DIRECTORY").map(PathBuf::from);
                // Explicit paths are the source-tree/operator fallback. Only
                // the release layout is accepted as an installed runtime.
                let installed_runtime = if explicit_api.is_none() && explicit_libraries.is_none() {
                    installed_plugin_runtime_directory().map_err(FotonServerError::Plugin)?
                } else {
                    None
                };
                let api_jar = explicit_api
                    .or_else(|| {
                        installed_runtime
                            .as_ref()
                            .map(|directory| directory.join("foton-plugin-api.jar"))
                    })
                    .unwrap_or_else(|| PathBuf::from("plugin-api/build/foton-plugin-api.jar"));
                let library_directory = explicit_libraries
                    .or_else(|| installed_runtime.map(|directory| directory.join("lib")))
                    .or_else(|| Some(PathBuf::from("plugin-api/lib")));
                let config = PluginHostConfig {
                    java_home: java_home.into(),
                    api_jar,
                    library_directory,
                    plugin_directory: plugin_directory.clone(),
                };
                Some(start_plugin_host(config, plugin_directory)?)
            }
        };

        let server = match Server::new_with_commands(
            chunk_runtime,
            cancel_token.clone(),
            runtime_config,
            worlds_config,
            permission_groups,
            command_registry,
        )
        .await
        {
            Ok(server) => Arc::new(server),
            Err(error) => {
                cleanup_plugin_host(&mut plugin_host, None);
                return Err(FotonServerError::Core(error));
            }
        };

        if let Some(host) = &plugin_host {
            host.bind_server(&Arc::downgrade(&server));
            if let Err(error) = host.enable_all() {
                shutdown_plugin_host(&mut plugin_host, Some(&server)).await;
                return Err(FotonServerError::Plugin(error.to_string()));
            }
        }

        let tcp_listener =
            match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, server_port)).await {
                Ok(listener) => listener,
                Err(source) => {
                    shutdown_plugin_host(&mut plugin_host, Some(&server)).await;
                    return Err(FotonServerError::Bind {
                        port: server_port,
                        source,
                    });
                }
            };

        // The server's own run directory, resolved to an absolute path once,
        // here, before anything derives a path from it -- but only when
        // Bedrock is actually enabled. Resolving it reads the current
        // directory and, on failure, logs a Bedrock-branded warning (see
        // `resolve_run_directory`); an operator who switched Bedrock off must
        // see neither that syscall nor that message. The Geyser supervisor
        // started below spawns Geyser with its own working directory one
        // level deeper (`bedrock/`); a path derived from a *relative*
        // `run_directory` -- the jar path, the Floodgate key path written
        // into Geyser's config -- would be handed to that child and resolved
        // against its cwd instead of ours, landing one `bedrock/` too deep.
        // This is also what keeps the login path (right below, via
        // `key::key_path`) and the generated config (inside the supervisor,
        // via the same function) agreeing on the key's location: both derive
        // it from this one absolute value, so there is only ever one file on
        // disk either of them can mean.
        let run_directory = bedrock_config.enable.then(resolve_run_directory);

        // The shared key is loaded before the first connection can arrive, so
        // the login path never sees a window where Bedrock is enabled but the
        // key is missing. A failure here disables the feature rather than
        // taking the server down: an operator who cannot read their key file
        // should get a Java server and a loud message, not no server.
        //
        // `run_directory` is `Some` exactly when Bedrock is enabled, so
        // matching on it here is the enable check.
        if let Some(run_directory) = &run_directory {
            // Purely config checks -- no I/O, so they run regardless of
            // whether the key below loads. Gated on this same block (which
            // is `Some` exactly when Bedrock is enabled) so an operator with
            // Bedrock off sees nothing new, and each logs once, here at
            // startup, rather than per-connection.
            warn_about_risky_bedrock_config(&bedrock_config, online_mode, enforce_secure_chat);

            let path = key::key_path(run_directory);
            match key::load_or_create(&path) {
                Ok(loaded) => {
                    key::init_shared(loaded);
                    log::info!(
                        "Bedrock: shared Floodgate key loaded from {}",
                        path.display()
                    );
                }
                Err(error) => {
                    log::error!(
                        "Bedrock: could not load the Floodgate key at {}: {error}. \
                         Bedrock logins will be refused.",
                        path.display()
                    );
                }
            }
        }

        let rcon_listener = if rcon_config.enable {
            let address = SocketAddr::new(rcon_config.bind_address, rcon_config.port);
            match rcon::RconListener::bind(
                rcon_config.bind_address,
                rcon_config.port,
                rcon_config.password.into(),
            )
            .await
            {
                Ok(listener) => Some(listener),
                Err(source) => {
                    shutdown_plugin_host(&mut plugin_host, Some(&server)).await;
                    return Err(FotonServerError::RconBind { address, source });
                }
            }
        } else {
            None
        };

        // Bedrock is an optional feature: a Geyser that cannot start must not
        // take the Java server down with it, unlike Rcon's bind above.
        let bedrock_supervisor = start_bedrock_supervisor(
            &bedrock_config,
            run_directory.as_deref(),
            server_port,
            &cancel_token,
        )
        .await;

        Ok(Self {
            tcp_listener,
            cancel_token,
            client_id: 0,
            server,
            plugin_host,
            connection_session: Arc::new(ServerConnectionSession::default()),
            rcon_listener,
            bedrock: bedrock_config,
            bedrock_supervisor,
        })
    }

    /// Disables loaded plugins during an early or orderly shutdown.
    pub fn disable_plugins(&mut self) {
        cleanup_plugin_host(&mut self.plugin_host, Some(&self.server));
    }

    /// Closes plugin persistence, drains every accepted update, then unloads plugins.
    pub async fn shutdown_plugins(&mut self) {
        shutdown_plugin_host(&mut self.plugin_host, Some(&self.server)).await;
    }

    fn mark_plugins_stopping(&self) {
        if let Some(host) = &self.plugin_host
            && let Err(error) = host.mark_stopping()
        {
            log::warn!("Failed to publish plugin shutdown state: {error}");
        }
    }

    /// Starts the server and begins accepting connections.
    pub async fn start(&mut self, task_tracker: TaskTracker) {
        log::info!("Started Foton Server");

        let server = self.server.clone();
        let token = self.cancel_token.clone();
        let server_handle = tokio::spawn(async move {
            server.run(token).await;
        });

        if let Some(rcon_listener) = self.rcon_listener.take() {
            task_tracker.spawn(rcon_listener.run(
                self.server.clone(),
                self.cancel_token.clone(),
                task_tracker.clone(),
            ));
        }

        // The supervisor already started Geyser and is supervising it on its
        // own background task (see `Supervisor::start`). Tracking `.wait()`
        // here, rather than dropping the handle, is what makes shutdown
        // actually wait for Geyser to stop instead of racing the process exit.
        if let Some(supervisor) = self.bedrock_supervisor.take() {
            task_tracker.spawn(supervisor.wait());
        }

        let java_connection_limiter = JavaTcpClient::connection_limiter();
        let java_source_limiter = SourceConnectionLimiter::new(MAX_PRE_PLAY_CONNECTIONS_PER_IP);
        loop {
            select! {
                () = self.cancel_token.cancelled() => {
                    break;
                }
                accept_result = self.tcp_listener.accept() => {
                    let Ok((connection, address)) = accept_result else {
                        continue;
                    };
                    let Some(connection_permit) = JavaTcpClient::try_acquire_connection_slot(
                        &java_connection_limiter,
                    ) else {
                        log::warn!(
                            "Rejecting Java connection from {address}: the global limit of {} live connections is full",
                            JavaTcpClient::MAX_LIVE_CONNECTIONS,
                        );
                        continue;
                    };
                    let Some(source_permit) = java_source_limiter.try_acquire(address.ip()) else {
                        log::debug!(
                            "Rejecting Java connection from {address}: its source already has {MAX_PRE_PLAY_CONNECTIONS_PER_IP} pre-play connections",
                        );
                        continue;
                    };
                    if let Err(e) = connection.set_nodelay(true) {
                        log::warn!("Failed to set TCP_NODELAY: {e}");
                    }
                    let client_id = self.client_id;
                    let translation = match &self.plugin_host {
                        Some(host) => match host.open_packet_translation().await {
                            Ok(translation) => translation,
                            Err(error) => {
                                log::warn!("Rejecting Java connection {client_id}: could not create Via pipeline: {error}");
                                continue;
                            }
                        },
                        None => None,
                    };
                    let (java_client, sender_recv, net_reader) = JavaTcpClient::new(
                        connection,
                        address,
                        client_id,
                        self.cancel_token.child_token(),
                        self.server.clone(),
                        self.connection_session.clone(),
                        task_tracker.clone(),
                        self.bedrock.clone(),
                        connection_permit,
                        source_permit,
                        translation,
                    );
                    self.client_id = self.client_id.wrapping_add(1);
                    log::info!("Accepted connection from Java Edition: {address} (id {client_id})");

                    let java_client = Arc::new(java_client);
                    java_client.start_outgoing_packet_task(sender_recv);
                    java_client.start_incoming_packet_task(net_reader);
                    // Both tasks retain the same admission slot through their
                    // play-phase handoff, so this local handle can now drop.
                }
            }
        }
        self.mark_plugins_stopping();
        let _ = server_handle.await;
        // Per-connection Via channels own plugin classes and Netty buffers.
        // Drain every network task (which closes those channels) before the
        // plugin lifecycle removes ViaVersion's handlers and class loaders.
        task_tracker.close();
        task_tracker.wait().await;
        self.disable_plugins();
    }
}

fn start_plugin_host(
    config: PluginHostConfig,
    plugin_directory: PathBuf,
) -> Result<PluginHost, FotonServerError> {
    let bootstrap = Builder::new()
        .name("foton-plugin-bootstrap".to_owned())
        .stack_size(PLUGIN_BOOTSTRAP_STACK_SIZE)
        .spawn(move || {
            let host = PluginHost::start(&config, &Weak::new())
                .map_err(|error| FotonServerError::Plugin(error.to_string()))?;
            if let Err(error) = host.load_all_on_load(&plugin_directory) {
                let _ = host.mark_stopping();
                let _ = host.disable_all();
                return Err(FotonServerError::Plugin(error.to_string()));
            }
            Ok(host)
        })
        .map_err(|error| {
            FotonServerError::Plugin(format!("could not start plugin bootstrap thread: {error}"))
        })?;

    bootstrap
        .join()
        .map_err(|_| FotonServerError::Plugin("plugin bootstrap thread panicked".to_owned()))?
}

impl Drop for FotonServer {
    fn drop(&mut self) {
        // Public embedders may drop a constructed server without ever calling
        // `start`; keep that exit path subject to the same lifecycle boundary.
        self.disable_plugins();
    }
}

fn cleanup_plugin_host(plugin_host: &mut Option<PluginHost>, server: Option<&Arc<Server>>) {
    mark_plugin_host_stopping(plugin_host.as_ref());
    cleanup_plugin_host_after_stopping(plugin_host, server);
}

async fn shutdown_plugin_host(plugin_host: &mut Option<PluginHost>, server: Option<&Arc<Server>>) {
    mark_plugin_host_stopping(plugin_host.as_ref());
    if let Some(server) = server {
        server.close_and_drain_plugin_persistence_updates().await;
    }
    cleanup_plugin_host_after_stopping(plugin_host, server);
}

fn mark_plugin_host_stopping(plugin_host: Option<&PluginHost>) {
    if let Some(host) = plugin_host
        && let Err(error) = host.mark_stopping()
    {
        log::warn!("Failed to publish plugin shutdown state: {error}");
    }
}

fn cleanup_plugin_host_after_stopping(
    plugin_host: &mut Option<PluginHost>,
    server: Option<&Arc<Server>>,
) {
    let Some(host) = plugin_host.take() else {
        return;
    };
    if let Some(server) = server {
        PluginHost::unsubscribe(server);
    }
    if let Err(error) = host.disable_all() {
        log::warn!("Failed to disable plugins cleanly: {error}");
    }
}

/// Finds the plugin runtime installed beside the executable or in the working
/// directory. The executable-relative lookup lets service managers start Foton
/// from another directory without losing the release bundle; the working
/// directory remains useful for unpacked and developer installations.
fn installed_plugin_runtime_directory() -> Result<Option<PathBuf>, String> {
    let executable_directory = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    for directory in executable_directory
        .into_iter()
        .map(|directory| directory.join("plugin-runtime"))
        .chain(iter::once(PathBuf::from("plugin-runtime")))
    {
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "could not inspect {}: {error}",
                    directory.display()
                ));
            }
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(format!(
                    "plugin runtime is not a direct directory: {}",
                    directory.display()
                ));
            }
            Ok(_) => validate_installed_plugin_runtime(&directory)?,
        }
        return Ok(Some(directory));
    }
    Ok(None)
}

const PLUGIN_RUNTIME_JARS: [&str; 24] = [
    "adventure-api-5.2.0.jar",
    "adventure-key-5.2.0.jar",
    "adventure-text-logger-slf4j-5.2.0.jar",
    "adventure-text-serializer-plain-5.2.0.jar",
    "annotations-26.1.0.jar",
    "brigadier-1.3.10.jar",
    "error_prone_annotations-2.47.0.jar",
    "failureaccess-1.0.3.jar",
    "gson-2.14.0.jar",
    "guava-33.6.0-jre.jar",
    "j2objc-annotations-3.1.jar",
    "joml-1.10.8.jar",
    "jspecify-1.0.0.jar",
    "kotlin-stdlib-1.8.20.jar",
    "kotlin-stdlib-common-1.8.20.jar",
    "kotlin-stdlib-jdk7-1.8.20.jar",
    "kotlin-stdlib-jdk8-1.8.20.jar",
    "netty-buffer-4.2.15.Final.jar",
    "netty-codec-base-4.2.15.Final.jar",
    "netty-common-4.2.15.Final.jar",
    "netty-resolver-4.2.15.Final.jar",
    "netty-transport-4.2.15.Final.jar",
    "slf4j-api-2.0.17.jar",
    "snakeyaml-2.2.jar",
];

const PLUGIN_RUNTIME_LICENSES: [&str; 6] = [
    "ADVENTURE-MIT.txt",
    "APACHE-2.0.txt",
    "BRIGADIER-MIT.txt",
    "JOML-MIT.txt",
    "SLF4J-MIT.txt",
    "THIRD-PARTY-NOTICES.txt",
];

fn direct_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn validate_runtime_directory_entries(
    directory: &Path,
    subdirectory: &str,
    expected: &[&str],
) -> Result<(), String> {
    let path = directory.join(subdirectory);
    let mut actual = fs::read_dir(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?
        .map(|entry| {
            let entry =
                entry.map_err(|error| format!("could not read {}: {error}", path.display()))?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
                format!("could not inspect {}: {error}", entry.path().display())
            })?;
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "plugin runtime entry is not a direct regular file: {}",
                    entry.path().display()
                ));
            }
            Ok(entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    actual.sort();
    let mut expected = expected
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    expected.sort();
    if actual != expected {
        return Err(format!(
            "plugin runtime {subdirectory} directory is not the exact supported file set"
        ));
    }
    Ok(())
}

fn validate_runtime_manifest(directory: &Path, expected_paths: &[String]) -> Result<(), String> {
    let manifest = fs::read_to_string(directory.join("SHA256SUMS"))
        .map_err(|error| format!("could not read plugin runtime manifest: {error}"))?;
    let mut manifest_paths = Vec::new();
    for line in manifest.lines() {
        let (expected_hash, relative) = line
            .split_once("  ")
            .ok_or_else(|| "plugin runtime manifest has an invalid line".to_owned())?;
        if expected_hash.len() != 64
            || !expected_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !expected_paths.iter().any(|expected| expected == relative)
        {
            return Err("plugin runtime manifest has an unexpected entry".to_owned());
        }
        let path = directory.join(relative);
        if !direct_regular_file(&path) {
            return Err(format!(
                "plugin runtime file is missing or unsafe: {}",
                path.display()
            ));
        }
        let mut file = fs::File::open(&path)
            .map_err(|error| format!("could not open {}: {error}", path.display()))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("could not hash {}: {error}", path.display()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        if lowercase_hex(&hasher.finalize()) != expected_hash {
            return Err(format!(
                "plugin runtime checksum mismatch: {}",
                path.display()
            ));
        }
        manifest_paths.push(relative.to_owned());
    }
    manifest_paths.sort();
    if manifest_paths != expected_paths {
        return Err("plugin runtime manifest is not the exact supported file set".to_owned());
    }
    Ok(())
}

fn validate_installed_plugin_runtime(directory: &Path) -> Result<(), String> {
    let expected_root = [
        ".release-tag",
        "SHA256SUMS",
        "foton-plugin-api.jar",
        "lib",
        "licenses",
    ];
    let mut root_entries = fs::read_dir(directory)
        .map_err(|error| {
            format!(
                "could not read plugin runtime {}: {error}",
                directory.display()
            )
        })?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            format!(
                "could not read plugin runtime {}: {error}",
                directory.display()
            )
        })?;
    root_entries.sort();
    if root_entries != expected_root {
        return Err(format!(
            "plugin runtime {} has unexpected root entries",
            directory.display()
        ));
    }
    for subdirectory in ["lib", "licenses"] {
        let path = directory.join(subdirectory);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        if !metadata.file_type().is_dir() {
            return Err(format!(
                "plugin runtime entry is not a direct directory: {}",
                path.display()
            ));
        }
    }

    validate_runtime_directory_entries(directory, "lib", &PLUGIN_RUNTIME_JARS)?;
    validate_runtime_directory_entries(directory, "licenses", &PLUGIN_RUNTIME_LICENSES)?;

    for name in ["foton-plugin-api.jar", "SHA256SUMS", ".release-tag"] {
        let path = directory.join(name);
        if !direct_regular_file(&path) {
            return Err(format!(
                "plugin runtime entry is not a direct regular file: {}",
                path.display()
            ));
        }
    }

    let tag = fs::read_to_string(directory.join(".release-tag"))
        .map_err(|error| format!("could not read plugin runtime release tag: {error}"))?;
    if tag.trim() != format!("v{}", env!("CARGO_PKG_VERSION")) {
        return Err("plugin runtime release tag does not match this Foton binary".to_owned());
    }

    let mut expected_paths = vec!["foton-plugin-api.jar".to_owned()];
    expected_paths.extend(PLUGIN_RUNTIME_JARS.map(|name| format!("lib/{name}")));
    expected_paths.extend(PLUGIN_RUNTIME_LICENSES.map(|name| format!("licenses/{name}")));
    expected_paths.sort();

    validate_runtime_manifest(directory, &expected_paths)
}

/// Warns at startup about `[server.bedrock]` combinations that are valid but
/// dangerous or self-defeating -- called only once Bedrock is confirmed
/// enabled (see the call site in [`FotonServer::new_with_commands`]), never
/// hard refusals: those genuine rejections belong in
/// `config::server::validate` instead, which runs before a server is built at
/// all. These three are things `validate` cannot express as a plain error
/// without also refusing configurations operators may knowingly want.
fn warn_about_risky_bedrock_config(
    bedrock_config: &BedrockConfig,
    online_mode: bool,
    enforce_secure_chat: bool,
) {
    if bedrock_config.username_prefix_could_collide_with_java_names() {
        log::warn!(
            "Bedrock: bedrock.username_prefix ({:?}) does not contain a character a vanilla \
             Java username can never contain, so a Bedrock player's derived name can collide \
             with a real Java player's. The default prefix (\".\") prevents this by \
             construction; an empty or alphanumeric-only prefix does not.",
            bedrock_config.username_prefix
        );
    }

    // A Floodgate player has no Mojang profile key and Geyser sends unsigned
    // chat, which `foton-core`'s chat handling disconnects on when secure
    // chat is enforced -- a valid configuration today, and a mystifying
    // mid-session kick for every Bedrock player's first chat message.
    if enforce_secure_chat {
        log::warn!(
            "Bedrock: enforce_secure_chat is enabled alongside Bedrock support. Bedrock \
             players send unsigned chat (Geyser/Floodgate players have no Mojang profile \
             key), so every Bedrock player will be disconnected the moment they send a chat \
             message. Disable enforce_secure_chat, or accept that Bedrock players cannot \
             chat, if this combination is intentional."
        );
    }

    // Without online_mode, a Java client can claim any username -- including
    // a Bedrock player's derived one -- so the only thing still protecting
    // Bedrock identity is bedrock.username_prefix containing a character a
    // Java username can never hold.
    if !online_mode {
        log::warn!(
            "Bedrock: online_mode is false while Bedrock support is enabled. A Java client \
             can then claim any username, including a Bedrock player's derived one, since \
             names are not verified against Mojang; the only remaining protection is \
             bedrock.username_prefix containing a character a Java username can never hold. \
             Enable online_mode for identity-safe Bedrock logins, or accept this risk \
             knowingly."
        );
    }
}

/// Resolves this process's own run directory to an absolute path.
///
/// `absolute` (`std::path::absolute`) rather than `std::fs::canonicalize`:
/// canonicalize requires the path to already exist and, on Windows, returns
/// a `\\?\`-prefixed UNC path -- not every tool handed a path derived from it
/// (Geyser's own JVM included) is guaranteed to accept that form. `absolute`
/// does neither: it never touches the filesystem beyond reading the current
/// directory, and it never resolves symlinks.
///
/// Falls back to a bare relative `.` -- the previous, buggy behavior -- only
/// if the current directory itself cannot be read; that failure mode is rare
/// enough (a deleted or permission-stripped cwd) that refusing to start the
/// whole server over it would be a worse trade than degrading Bedrock
/// support the way every other optional-feature failure here already does.
///
/// Only ever called when `[server.bedrock] enable` is set (see the call site
/// in [`FotonServer::new_with_commands`]): both the syscall and the warning
/// this can log are Bedrock-branded, and an operator who disabled Bedrock
/// must see neither.
fn resolve_run_directory() -> PathBuf {
    absolute(".").unwrap_or_else(|error| {
        log::warn!(
            "Bedrock: could not resolve the run directory to an absolute path ({error}); \
             falling back to a relative one, which can break Bedrock support if Geyser's \
             own working directory differs from this process's."
        );
        PathBuf::from(".")
    })
}

/// Starts this server's Geyser supervisor, if `[server.bedrock] enable` is
/// set — resolving Java, fetching the pinned jar, writing the shared
/// Floodgate key and `config.yml`, and starting Geyser.
///
/// `run_directory` is `None` exactly when `bedrock_config.enable` is `false`
/// (the caller only resolves it when Bedrock is enabled); the explicit
/// `enable` check below still comes first so this function's contract does
/// not depend on that pairing holding.
///
/// A failure here is logged and turned into `None` rather than propagated:
/// this mirrors the key-loading policy in [`FotonServer::new_with_commands`]
/// rather than the Rcon listener's — Bedrock is an optional feature, and an
/// operator who misconfigured it should get a working Java server and a loud
/// message, not no server at all.
async fn start_bedrock_supervisor(
    bedrock_config: &BedrockConfig,
    run_directory: Option<&Path>,
    server_port: u16,
    cancel_token: &CancellationToken,
) -> Option<Supervisor> {
    if !bedrock_config.enable {
        return None;
    }
    let run_directory = run_directory?;

    let options = GeyserOptions {
        run_directory: run_directory.to_path_buf(),
        bedrock_port: bedrock_config.resolved_port(server_port),
        java_port: server_port,
        // Passed through unresolved: an empty string is exactly what tells
        // `render_config` to emit Geyser's own `passthrough-motd: true` and
        // relay this Java server's *live* MOTD, rather than one frozen at
        // this moment during startup.
        motd: bedrock_config.motd.clone(),
        java_home: (!bedrock_config.java_home.is_empty())
            .then(|| PathBuf::from(&bedrock_config.java_home)),
        jar_path: (!bedrock_config.jar_path.is_empty())
            .then(|| PathBuf::from(&bedrock_config.jar_path)),
    };

    match Supervisor::start(options, cancel_token.clone()).await {
        Ok(supervisor) => Some(supervisor),
        Err(error) => {
            log::error!(
                "Bedrock: Geyser failed to start ({error}). Check bedrock.java_home, \
                 bedrock.jar_path and the bedrock/ directory under the run directory, \
                 then restart the server. Continuing without Bedrock support; \
                 Java players are unaffected."
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs, io,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        path::Path,
    };

    use sha2::{Digest, Sha256};

    use super::{
        BedrockConfig, CancellationToken, FotonServerError, PLUGIN_RUNTIME_JARS,
        PLUGIN_RUNTIME_LICENSES, SourceConnectionLimiter, lowercase_hex, resolve_run_directory,
        start_bedrock_supervisor, validate_installed_plugin_runtime,
    };

    fn write_runtime_fixture(directory: &Path) {
        fs::create_dir_all(directory.join("lib")).expect("runtime library directory");
        fs::create_dir_all(directory.join("licenses")).expect("runtime license directory");
        fs::write(directory.join("foton-plugin-api.jar"), b"api").expect("API fixture");
        for name in PLUGIN_RUNTIME_JARS {
            fs::write(directory.join("lib").join(name), name).expect("library fixture");
        }
        for name in PLUGIN_RUNTIME_LICENSES {
            fs::write(directory.join("licenses").join(name), name).expect("license fixture");
        }
        fs::write(
            directory.join(".release-tag"),
            format!("v{}\n", env!("CARGO_PKG_VERSION")),
        )
        .expect("release tag fixture");
        let mut paths = vec!["foton-plugin-api.jar".to_owned()];
        paths.extend(PLUGIN_RUNTIME_JARS.map(|name| format!("lib/{name}")));
        paths.extend(PLUGIN_RUNTIME_LICENSES.map(|name| format!("licenses/{name}")));
        paths.sort();
        let manifest = paths
            .into_iter()
            .map(|relative| {
                let bytes = fs::read(directory.join(&relative)).expect("manifest fixture file");
                format!("{}  {relative}", lowercase_hex(&Sha256::digest(bytes)))
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(directory.join("SHA256SUMS"), format!("{manifest}\n")).expect("manifest fixture");
    }

    #[test]
    fn installed_plugin_runtime_requires_exact_hashed_closure() {
        let temporary = tempfile::tempdir().expect("runtime fixture directory");
        write_runtime_fixture(temporary.path());
        validate_installed_plugin_runtime(temporary.path()).expect("complete runtime");

        fs::remove_file(temporary.path().join("lib/failureaccess-1.0.3.jar"))
            .expect("remove transitive fixture");
        assert!(validate_installed_plugin_runtime(temporary.path()).is_err());
    }

    #[test]
    fn installed_plugin_runtime_rejects_content_changed_after_manifest() {
        let temporary = tempfile::tempdir().expect("runtime fixture directory");
        write_runtime_fixture(temporary.path());
        fs::write(temporary.path().join("foton-plugin-api.jar"), b"changed")
            .expect("corrupt API fixture");

        assert!(validate_installed_plugin_runtime(temporary.path()).is_err());
    }

    #[test]
    fn installed_plugin_runtime_rejects_unmanifested_files() {
        let temporary = tempfile::tempdir().expect("runtime fixture directory");
        write_runtime_fixture(temporary.path());
        fs::write(temporary.path().join("lib/unpinned.jar"), b"untrusted")
            .expect("unmanifested fixture");

        assert!(validate_installed_plugin_runtime(temporary.path()).is_err());
    }

    #[test]
    fn per_source_pre_play_limit_releases_and_forgets_idle_sources() {
        let limiter = SourceConnectionLimiter::new(2);
        let first_ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let second_ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));

        let first = limiter
            .try_acquire(first_ip)
            .expect("the first source slot should be available");
        let second = limiter
            .try_acquire(first_ip)
            .expect("the second source slot should be available");
        assert!(limiter.try_acquire(first_ip).is_none());
        let other = limiter
            .try_acquire(second_ip)
            .expect("one saturated source must not block another");
        assert_eq!(limiter.tracked_sources(), 2);

        drop(first);
        assert!(limiter.try_acquire(first_ip).is_some());
        drop(second);
        drop(other);
        assert_eq!(
            limiter.tracked_sources(),
            0,
            "source accounting must not grow with historical addresses"
        );
    }

    #[test]
    fn rcon_bind_error_reports_the_complete_configured_address() {
        let address: SocketAddr = "[::1]:25575".parse().expect("the test address is valid");
        let error = FotonServerError::RconBind {
            address,
            source: io::Error::new(io::ErrorKind::AddrInUse, "occupied"),
        };

        assert_eq!(
            error.to_string(),
            "failed to bind rcon to [::1]:25575: occupied"
        );
    }

    #[test]
    fn resolve_run_directory_is_always_absolute() {
        // Everything Bedrock support derives from this value -- the shared
        // Floodgate key this process loads, and every path `GeyserOptions`
        // hands the supervisor -- crosses Geyser's own `current_dir`
        // boundary (its working directory is `bedrock/`, one level deeper).
        // A relative run directory does not survive that boundary; this is
        // the defect this function exists to close.
        assert!(
            resolve_run_directory().is_absolute(),
            "a relative run directory breaks Bedrock support the moment Geyser's \
             own working directory differs from this process's"
        );
    }

    #[test]
    fn resolve_run_directory_agrees_with_a_fresh_current_dir_lookup() {
        // The property the whole fix rests on: the login path (which reads
        // the shared key from `key::key_path(run_directory)`) and the
        // supervisor (which writes that same path into Geyser's generated
        // config) must resolve to the identical file on disk. Both start
        // from this one function's return value, so proving it matches an
        // independent, fresh lookup of the current directory is what rules
        // out the two silently drifting apart.
        let resolved = resolve_run_directory();
        let expected =
            env::current_dir().expect("the process's own current directory must be readable");
        assert_eq!(resolved, expected);
    }

    #[tokio::test]
    async fn start_bedrock_supervisor_tolerates_a_missing_run_directory_when_disabled() {
        // The call site (`FotonServer::new_with_commands`) only resolves a
        // run directory when Bedrock is enabled, so a disabled config is
        // always paired with `run_directory: None` here. This asserts that
        // pairing is safe: the function must short-circuit on `enable`
        // before it ever needs `run_directory`, not panic reaching for one.
        // It cannot assert the other half of the fix -- that
        // `resolve_run_directory` (and the syscall/log inside it) is never
        // even called for a disabled operator -- without faking a broken
        // `env::current_dir`, which is beyond what a unit test here can do;
        // that half is enforced by construction, since `bedrock_config.enable
        // .then(resolve_run_directory)` at the call site cannot invoke the
        // function when `enable` is `false`.
        let bedrock_config = BedrockConfig::default();
        assert!(!bedrock_config.enable);
        let cancel_token = CancellationToken::new();

        let supervisor =
            start_bedrock_supervisor(&bedrock_config, None, 25565, &cancel_token).await;

        assert!(supervisor.is_none());
    }
}
