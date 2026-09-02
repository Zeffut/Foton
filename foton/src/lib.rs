//! # Foton
//!
//! The main library for the Foton Minecraft server.

use std::{
    error::Error,
    fmt, io,
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf, absolute},
    sync::{Arc, OnceLock},
};

use foton_bedrock::config::BedrockConfig;
use foton_bedrock::geyser::{GeyserOptions, Supervisor};
use foton_bedrock::key;
use foton_core::{command::CommandRegistry, permission::PermissionGroupManager, server::Server};
use foton_login::{JavaTcpClient, ServerConnectionSession};
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
    /// The Rcon listener could not bind.
    RconBind {
        /// Rcon port that failed to bind.
        port: u16,
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
            Self::RconBind { port, source } => {
                write!(f, "failed to bind to rcon port {port}: {source}")
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

        let server = Server::new_with_commands(
            chunk_runtime,
            cancel_token.clone(),
            runtime_config,
            worlds_config,
            permission_groups,
            command_registry,
        )
        .await
        .map_err(FotonServerError::Core)?;

        let tcp_listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, server_port))
            .await
            .map_err(|source| FotonServerError::Bind {
                port: server_port,
                source,
            })?;

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
            Some(
                rcon::RconListener::bind(rcon_config.port, rcon_config.password.into())
                    .await
                    .map_err(|source| FotonServerError::RconBind {
                        port: rcon_config.port,
                        source,
                    })?,
            )
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
            server: Arc::new(server),
            connection_session: Arc::new(ServerConnectionSession::default()),
            rcon_listener,
            bedrock: bedrock_config,
            bedrock_supervisor,
        })
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

        loop {
            select! {
                () = self.cancel_token.cancelled() => {
                    break;
                }
                accept_result = self.tcp_listener.accept() => {
                    let Ok((connection, address)) = accept_result else {
                        continue;
                    };
                    if let Err(e) = connection.set_nodelay(true) {
                        log::warn!("Failed to set TCP_NODELAY: {e}");
                    }
                    let (java_client, sender_recv, net_reader) = JavaTcpClient::new(
                        connection,
                        address,
                        self.client_id,
                        self.cancel_token.child_token(),
                        self.server.clone(),
                        self.connection_session.clone(),
                        task_tracker.clone(),
                        self.bedrock.clone(),
                    );
                    self.client_id = self.client_id.wrapping_add(1);
                    log::info!("Accepted connection from Java Edition: {address} (id {})", self.client_id);

                    let java_client = Arc::new(java_client);
                    java_client.start_outgoing_packet_task(sender_recv);
                    java_client.start_incoming_packet_task(net_reader);
                    // Java_client won't drop until the incoming and outcoming task close
                    // So we dont need to care about them here anymore
                }
            }
        }
        let _ = server_handle.await;
    }
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
        username_prefix: bedrock_config.username_prefix.clone(),
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
    use std::env;

    use super::{
        BedrockConfig, CancellationToken, resolve_run_directory, start_bedrock_supervisor,
    };

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
