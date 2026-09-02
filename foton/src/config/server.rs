// The Bedrock settings live in `foton-bedrock` rather than here: the login
// path reads them too, and `foton-login` cannot depend on this binary crate.
pub use foton_bedrock::config::BedrockConfig;
use foton_core::{
    chunk::chunk_ticket_manager::MAX_SUPPORTED_VIEW_DISTANCE,
    config::{
        BugReportWebhook, CompressionInfo, RuntimeConfig, ServerLinks, validate_login_security,
    },
};
use reqwest::Url;
use serde::Deserialize;

const fn default_spam_threshold_seconds() -> i32 {
    10
}

const fn default_max_chained_neighbor_updates() -> i32 {
    1_000_000
}

/// The full server configuration as deserialized from TOML.
///
/// Contains both creation-time values (seed, world generator, storage)
/// and runtime values that get moved into `RuntimeConfig`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// The port the server will listen on.
    pub server_port: u16,
    /// The maximum number of players that can be on the server at once.
    pub max_players: u32,
    /// Allow `view_distance` above vanilla's 32-chunk cap.
    #[serde(default)]
    pub allow_extended_view_distance: bool,
    /// The view distance of the server.
    pub view_distance: u8,
    /// The simulation distance of the server.
    pub simulation_distance: u8,
    /// Maximum queued neighbor-update tasks in one chained run; negative means unlimited.
    #[serde(default = "default_max_chained_neighbor_updates")]
    pub max_chained_neighbor_updates: i32,
    /// Whether the server is in online mode.
    pub online_mode: bool,
    /// Optional authentication endpoint for online-mode `hasJoined` checks.
    pub auth_server: Option<String>,
    /// Optional endpoint for online-mode player name-to-profile lookups.
    pub profile_server: Option<String>,
    /// Optional endpoint for Mojang-compatible service public keys.
    pub services_server: Option<String>,
    /// Whether the server should use encryption. Required in online mode.
    pub encryption: bool,
    /// Whether vanilla floating/flying movement checks permit unauthorized flight.
    #[serde(default)]
    pub allow_flight: bool,
    /// The message of the day.
    pub motd: String,
    /// Whether to use a favicon.
    pub use_favicon: bool,
    /// The path to the favicon.
    pub favicon: String,
    /// Whether to enforce secure chat.
    pub enforce_secure_chat: bool,
    /// Vanilla chat spam threshold window in seconds
    #[serde(default = "default_spam_threshold_seconds")]
    pub chat_spam_threshold_seconds: i32,
    /// Vanilla command spam threshold window in seconds
    #[serde(default = "default_spam_threshold_seconds")]
    pub command_spam_threshold_seconds: i32,
    /// The compression settings for the server.
    pub compression: Option<CompressionInfo>,
    /// All settings and configurations for server links.
    pub server_links: Option<ServerLinks>,
    /// Thread counts for server thread pools.
    #[serde(default)]
    pub threads: ThreadConfig,
    /// Remote administration over the Source Rcon protocol.
    #[serde(default)]
    pub rcon: RconConfig,
    /// Letting Bedrock Edition players in.
    #[serde(default)]
    pub bedrock: BedrockConfig,
    /// Where player-filed bug reports are sent, on top of the local file.
    #[serde(default)]
    pub bug_reports: BugReportsConfig,
}

/// Where player-filed bug reports go once they are on disk.
///
/// Leaving `webhook_url` unset keeps reports local, which is the right default
/// for a server nobody is collecting from.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BugReportsConfig {
    /// Endpoint each filed report is posted to. Unset means reports stay local.
    pub webhook_url: Option<String>,
    /// Optional bearer token sent with each post.
    pub webhook_token: Option<String>,
}

impl BugReportsConfig {
    /// Resolves the configured endpoint, rejecting an address that cannot work.
    ///
    /// A typo here is otherwise invisible until a tester writes out a repro
    /// that then goes nowhere, so it is a startup failure rather than a
    /// warning -- the same reasoning `RconConfig` uses for a blank password.
    pub(super) fn into_webhook(self) -> Result<Option<BugReportWebhook>, String> {
        let Some(url) = self.webhook_url else {
            return Ok(None);
        };
        let url = Url::parse(&url)
            .map_err(|error| format!("bug_reports.webhook_url is not a valid URL: {error}"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(format!(
                "bug_reports.webhook_url must be http or https, not {}",
                url.scheme()
            ));
        }
        Ok(Some(BugReportWebhook {
            url,
            token: self.webhook_token,
        }))
    }
}

impl ServerConfig {
    /// Extracts the `RuntimeConfig` from this full config.
    ///
    /// # Errors
    ///
    /// Returns a message describing any setting that cannot be resolved.
    pub fn into_runtime_config(self) -> Result<RuntimeConfig, String> {
        let bug_report_webhook = self.bug_reports.into_webhook()?;
        Ok(RuntimeConfig {
            max_players: self.max_players,
            view_distance: self.view_distance,
            simulation_distance: self.simulation_distance,
            max_chained_neighbor_updates: self.max_chained_neighbor_updates,
            online_mode: self.online_mode,
            auth_server: self.auth_server,
            profile_server: self.profile_server,
            services_server: self.services_server,
            encryption: self.encryption,
            allow_flight: self.allow_flight,
            motd: self.motd,
            use_favicon: self.use_favicon,
            favicon: self.favicon,
            enforce_secure_chat: self.enforce_secure_chat,
            chat_spam_threshold_seconds: self.chat_spam_threshold_seconds,
            command_spam_threshold_seconds: self.command_spam_threshold_seconds,
            compression: self.compression,
            server_links: self.server_links,
            packet_workers: self.threads.packet_workers,
            chunk_generation_threads: self.threads.chunk_generation,
            chunk_encoding_threads: self.threads.chunk_encoding,
            bug_report_webhook,
        })
    }
}

/// Remote administration over the Source Rcon protocol.
///
/// Vanilla parity: `enable-rcon`, `rcon.port` and `rcon.password`. Vanilla
/// warns and quietly disables Rcon when the password is blank; Foton refuses
/// to start instead, because an administrator who asked for remote access and
/// silently did not get it is worse off than one who is told why.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RconConfig {
    /// Whether the Rcon port is opened at all.
    pub enable: bool,
    /// The port Rcon listens on.
    pub port: u16,
    /// The password every client must send before it may run a command.
    pub password: String,
}

impl Default for RconConfig {
    fn default() -> Self {
        Self {
            enable: false,
            // Vanilla's default `rcon.port`.
            port: 25575,
            password: String::new(),
        }
    }
}

/// Optional worker counts for server thread pools.
///
/// A value of `0` or an omitted field uses the pool's automatic default.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThreadConfig {
    /// Worker threads for the primary Tokio runtime.
    pub main_runtime: Option<usize>,
    /// Worker threads for the chunk Tokio runtime.
    pub chunk_runtime: Option<usize>,
    /// Persistent workers for inter-tick gameplay packet processing.
    pub packet_workers: Option<usize>,
    /// Worker threads for the Rayon chunk generation pool.
    pub chunk_generation: Option<usize>,
    /// Worker threads for the Rayon chunk encoding pool.
    pub chunk_encoding: Option<usize>,
}

/// Validates the server configuration.
///
/// # Errors
/// This function will return an error if the configuration is invalid.
pub(super) fn validate(config: &ServerConfig) -> Result<(), &'static str> {
    validate_login_security(config.online_mode, config.encryption)?;
    if !config.allow_extended_view_distance && !(1..=32).contains(&config.view_distance) {
        return Err("View distance must in range 1..32");
    }
    if config.allow_extended_view_distance
        && !(1..=MAX_SUPPORTED_VIEW_DISTANCE).contains(&config.view_distance)
    {
        return Err("View distance must in range 1..128");
    }
    if let Some(auth_server) = &config.auth_server {
        let Ok(url) = Url::parse(auth_server) else {
            return Err("auth_server must be an absolute URL");
        };
        if !matches!(url.scheme(), "http" | "https") {
            return Err("auth_server must use http or https");
        }
    }
    if let Some(profile_server) = &config.profile_server {
        let Ok(url) = Url::parse(profile_server) else {
            return Err("profile_server must be an absolute URL");
        };
        if !matches!(url.scheme(), "http" | "https") {
            return Err("profile_server must use http or https");
        }
    }
    if let Some(services_server) = &config.services_server {
        let Ok(url) = Url::parse(services_server) else {
            return Err("services_server must be an absolute URL");
        };
        if !matches!(url.scheme(), "http" | "https") {
            return Err("services_server must use http or https");
        }
    }
    if config.simulation_distance > config.view_distance {
        return Err("Simulation distance must be less than or equal to view distance");
    }
    if let Some(compression) = config.compression {
        if compression.threshold.get() < 256 {
            return Err("Compression threshold must be greater than or equal to 256");
        }
        if !(1..=9).contains(&compression.level) {
            return Err("Compression level must be between 1 and 9");
        }
    }
    if config.rcon.enable {
        if config.rcon.password.is_empty() {
            return Err("rcon.password must be set when rcon is enabled");
        }
        if config.rcon.port == 0 {
            return Err("rcon.port must be a real port when rcon is enabled");
        }
        if config.rcon.port == config.server_port {
            return Err("rcon.port must differ from server_port");
        }
    }
    if config.bedrock.enable {
        if config.bedrock.port == 0 {
            return Err("bedrock.port must be a real port when bedrock is enabled");
        }
        if config.bedrock.port == config.server_port {
            return Err("bedrock.port must differ from server_port");
        }
        if config.bedrock.trusted_proxies.is_empty() {
            return Err("bedrock.trusted_proxies must list at least one address");
        }
        if config.bedrock.username_prefix.chars().count() >= 16 {
            return Err("bedrock.username_prefix leaves no room for a username");
        }
    }
    if config.enforce_secure_chat {
        if !config.online_mode {
            return Err("online_mode must be true when enforce_secure_chat is enabled");
        }
        if !config.encryption {
            return Err("encryption must be true when enforce_secure_chat is enabled");
        }
    }
    Ok(())
}
