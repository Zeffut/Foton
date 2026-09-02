//! # Foton
//!
//! The main library for the Foton Minecraft server.

use std::{
    error::Error,
    fmt, io,
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
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

        // The server's own run directory. The Geyser supervisor started below
        // must resolve the shared key under this exact same directory — a
        // second, independently-typed path would let the two silently drift
        // and fail every Bedrock login to decrypt.
        let run_directory = Path::new(".");

        // The shared key is loaded before the first connection can arrive, so
        // the login path never sees a window where Bedrock is enabled but the
        // key is missing. A failure here disables the feature rather than
        // taking the server down: an operator who cannot read their key file
        // should get a Java server and a loud message, not no server.
        if bedrock_config.enable {
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
        let bedrock_supervisor =
            start_bedrock_supervisor(&bedrock_config, run_directory, server_port, &cancel_token)
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

/// Starts this server's Geyser supervisor, if `[server.bedrock] enable` is
/// set — resolving Java, fetching the pinned jar, writing the shared
/// Floodgate key and `config.yml`, and starting Geyser.
///
/// A failure here is logged and turned into `None` rather than propagated:
/// this mirrors the key-loading policy in [`FotonServer::new_with_commands`]
/// rather than the Rcon listener's — Bedrock is an optional feature, and an
/// operator who misconfigured it should get a working Java server and a loud
/// message, not no server at all.
async fn start_bedrock_supervisor(
    bedrock_config: &BedrockConfig,
    run_directory: &Path,
    server_port: u16,
    cancel_token: &CancellationToken,
) -> Option<Supervisor> {
    if !bedrock_config.enable {
        return None;
    }

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
