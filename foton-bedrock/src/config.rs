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
            // Bedrock's default port.
            port: 19132,
            motd: String::new(),
            username_prefix: ".".to_owned(),
            trusted_proxies: vec!["127.0.0.1".to_owned(), "::1".to_owned()],
            java_home: String::new(),
            jar_path: String::new(),
        }
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
}
