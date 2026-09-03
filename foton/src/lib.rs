//! # Foton
//!
//! The main library for the Foton Minecraft server.

use std::{
    error::Error,
    fmt, io,
    net::{Ipv4Addr, SocketAddrV4},
    sync::{Arc, OnceLock},
};

use foton_core::{command::CommandRegistry, permission::PermissionGroupManager, server::Server};
use foton_login::{JavaTcpClient, ServerConnectionSession};
use foton_plugin::{PluginHost, PluginHostConfig};
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
    /// Java plugin host, enabled only when FOTON_PLUGIN_DIRECTORY is set.
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
            Self::Plugin(error) => write!(f, "plugin host failed: {error}"),
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
        let plugin_host = match std::env::var_os("FOTON_PLUGIN_DIRECTORY") {
            None => None,
            Some(plugin_directory) => {
                let plugin_directory = std::path::PathBuf::from(plugin_directory);
                let java_home = std::env::var_os("FOTON_JAVA_HOME").ok_or_else(|| {
                    FotonServerError::Plugin(
                        "FOTON_JAVA_HOME is required when FOTON_PLUGIN_DIRECTORY is set".to_owned(),
                    )
                })?;
                let api_jar = std::env::var_os("FOTON_PLUGIN_API_JAR").map_or_else(
                    || std::path::PathBuf::from("plugin-api/build/foton-plugin-api.jar"),
                    std::path::PathBuf::from,
                );
                let library_directory = std::env::var_os("FOTON_PLUGIN_LIBRARY_DIRECTORY")
                    .map(std::path::PathBuf::from);
                let host = PluginHost::start(
                    &PluginHostConfig {
                        java_home: java_home.into(),
                        api_jar,
                        library_directory,
                        plugin_directory: plugin_directory.clone(),
                    },
                    &std::sync::Weak::new(),
                )
                .map_err(|error| FotonServerError::Plugin(error.to_string()))?;
                host.load_all_on_load(&plugin_directory)
                    .map_err(|error| FotonServerError::Plugin(error.to_string()))?;
                Some(host)
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
                if let Some(host) = &plugin_host {
                    let _ = host.disable_all();
                }
                return Err(FotonServerError::Core(error));
            }
        };

        if let Some(host) = &plugin_host {
            host.bind_server(&Arc::downgrade(&server));
            if let Err(error) = host.enable_all() {
                let _ = host.disable_all();
                return Err(FotonServerError::Plugin(error.to_string()));
            }
        }

        let tcp_listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, server_port))
            .await
            .map_err(|source| FotonServerError::Bind {
                port: server_port,
                source,
            })?;

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

        Ok(Self {
            tcp_listener,
            cancel_token,
            client_id: 0,
            server,
            plugin_host,
            connection_session: Arc::new(ServerConnectionSession::default()),
            rcon_listener,
        })
    }

    /// Disables loaded plugins during an early or orderly shutdown.
    pub fn disable_plugins(&mut self) {
        if let Some(host) = self.plugin_host.take() {
            if let Err(error) = host.disable_all() {
                log::warn!("Failed to disable plugins cleanly: {error}");
            }
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
        self.disable_plugins();
    }
}
