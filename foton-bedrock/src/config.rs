//! What an operator turns on, and what both sides of the feature read.
//!
//! This lives here rather than beside the rest of the server's configuration
//! for a structural reason: the login path needs it, and `foton-login` cannot
//! reach the `foton` binary crate — the dependency runs the other way. Both
//! `foton` and `foton-login` already depend on `foton-bedrock`, so this is the
//! one place both can see, and one declaration is the point. Two mirrors of
//! the same operator-facing settings drift, and the drift is silent.

use serde::Deserialize;

/// Letting Bedrock Edition players in, through a Geyser this server runs.
///
/// Off by default: enabling it starts a Java process, and nothing should pay
/// for a feature it did not ask for.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BedrockConfig {
    /// Whether Geyser is started and the Bedrock port opened at all.
    ///
    /// Off also means the login path takes every hostname down the ordinary
    /// Java route, including one carrying a Floodgate payload.
    pub enable: bool,
    /// The UDP port Bedrock clients connect to.
    ///
    /// `0` — the default — means "share the Java server's port": one port
    /// number for an operator to open, and it keeps following `server_port`
    /// if that ever moves. [`BedrockConfig::resolved_port`] is the single
    /// place this is turned into an actual port number; TCP (Java) and
    /// UDP (Bedrock/RakNet) do not share a port namespace, so this is not a
    /// collision.
    ///
    /// The honest trade-off: a Bedrock client's own "Add Server" dialog
    /// pre-fills `19132`, so sharing the Java port means a player must type
    /// the port in by hand. An operator who wants that automatic experience
    /// back sets this to `19132` explicitly.
    pub port: u16,
    /// What Bedrock clients see in the server list; empty reuses the server MOTD.
    pub motd: String,
    /// Prepended to a Bedrock player's gamertag so it cannot collide with a
    /// Java player's name. Empty means no prefix, and collisions become possible.
    pub username_prefix: String,
    /// Addresses a Floodgate handshake is accepted from.
    ///
    /// Foton starts Geyser locally, so loopback is the whole list by default. A
    /// handshake claiming Floodgate from anywhere else is refused, because the
    /// alternative is that anyone who can reach the Java port becomes anyone.
    pub trusted_proxies: Vec<String>,
    /// A JDK or JRE for Geyser; empty means `JAVA_HOME`, then `java` on the path.
    pub java_home: String,
    /// An operator-supplied Geyser jar; empty means fetch the pinned build.
    pub jar_path: String,
}

impl Default for BedrockConfig {
    fn default() -> Self {
        Self {
            enable: false,
            // 0: share the Java server's port. See the field's own doc comment.
            port: 0,
            motd: String::new(),
            username_prefix: ".".to_owned(),
            trusted_proxies: vec!["127.0.0.1".to_owned(), "::1".to_owned()],
            java_home: String::new(),
            jar_path: String::new(),
        }
    }
}

impl BedrockConfig {
    /// Resolves the actual port Geyser binds: `java_port` when
    /// [`BedrockConfig::port`] is `0` ("share the Java server's port"),
    /// otherwise `port` itself.
    ///
    /// This is the single place that resolution happens — a caller that
    /// needs the real Bedrock port number calls this rather than reading
    /// `port` directly, which would still be `0` when it means "shared".
    #[must_use]
    pub const fn resolved_port(&self, java_port: u16) -> u16 {
        if self.port == 0 { java_port } else { self.port }
    }
}

#[cfg(test)]
mod tests {
    use super::BedrockConfig;

    #[test]
    fn the_default_is_off_and_loopback_only() {
        // Both halves of this matter: a feature nobody asked for must not start
        // a JVM, and a Floodgate handshake must not be trusted from anywhere
        // but the Geyser this server launched.
        let config = BedrockConfig::default();
        assert!(!config.enable);
        assert_eq!(config.trusted_proxies, vec!["127.0.0.1", "::1"]);
    }

    #[test]
    fn resolved_port_defaults_to_sharing_the_java_servers_port() {
        // `port`'s own default is `0`; `resolved_port` is what turns that
        // into an actual port number.
        let config = BedrockConfig::default();
        assert_eq!(config.port, 0);
        assert_eq!(config.resolved_port(25565), 25565);
    }

    #[test]
    fn resolved_port_honours_an_explicit_port() {
        let config = BedrockConfig {
            port: 19132,
            ..BedrockConfig::default()
        };
        assert_eq!(config.resolved_port(25565), 19132);
    }
}
