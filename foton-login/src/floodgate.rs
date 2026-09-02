//! Accepting a verified Bedrock identity at login.
//!
//! Geyser's Floodgate extension puts a Bedrock player's identity in the Java
//! handshake's hostname, encrypted with a key this server and its Geyser
//! share (`foton_bedrock::key`). This module is the login-time half: it
//! decides whether a handshake's hostname carries a Floodgate payload and,
//! if so, whether to trust it.
//!
//! A Floodgate handshake is an assertion of identity that skips Mojang
//! entirely, so every failure here is a hard reject. A payload that fails
//! any check -- untrusted source address, failed decryption, malformed
//! fields, an uninitialized shared key -- must never fall back to an
//! ordinary offline login: that would turn a failed forgery into a
//! successful one. Only a hostname carrying no Floodgate payload at all
//! falls through to the normal login path.

use std::net::{IpAddr, SocketAddr};

use foton_bedrock::config::BedrockConfig;
use foton_bedrock::floodgate::{self, FloodgateError};
use foton_bedrock::key;
use foton_core::player::GameProfile;
use thiserror::Error;

/// Why a handshake claiming to carry Bedrock identity was not honored.
#[derive(Debug, Error)]
pub(crate) enum FloodgateLoginError {
    /// A Floodgate payload arrived from an address not in `trusted_proxies`.
    #[error("Floodgate handshake from untrusted address {0}")]
    UntrustedAddress(IpAddr),
    /// The payload did not decrypt or did not parse.
    #[error("Floodgate handshake rejected: {0}")]
    Rejected(FloodgateError),
    /// A Floodgate payload arrived but no shared key has been loaded.
    ///
    /// This is the server's own uninitialized state, not the client's
    /// fault, but it is refused exactly like a rejected handshake: an
    /// uninitialized key must never be treated as a reason to fall back to
    /// an ordinary login, only as a reason to refuse.
    #[error("Floodgate handshake received but no shared key is loaded")]
    KeyUnavailable,
}

/// Turns a handshake hostname into a Bedrock player's profile, if it carries
/// one, given the shared key directly.
///
/// Returns `Ok(None)` when the hostname is an ordinary Java client's, or
/// Bedrock support is disabled, so the caller falls through to the normal
/// login path. Every other non-success is an error: a Floodgate payload that
/// fails any check is refused outright and never downgraded to an offline
/// login.
///
/// Pure and deterministic -- no I/O, no global state -- so it stays directly
/// testable. Production code should generally call
/// [`resolve_floodgate_login`] instead, which sources the key from
/// [`foton_bedrock::key::shared`] and enforces that an uninitialized key
/// refuses rather than silently skipping the check.
pub(crate) fn resolve_floodgate(
    hostname: &str,
    peer: SocketAddr,
    key: &[u8; 16],
    config: &BedrockConfig,
) -> Result<Option<GameProfile>, FloodgateLoginError> {
    if !config.enable {
        return Ok(None);
    }

    let Some((payload, _rest)) = floodgate::extract_payload(hostname) else {
        return Ok(None);
    };

    if !is_trusted(peer.ip(), &config.trusted_proxies) {
        return Err(FloodgateLoginError::UntrustedAddress(peer.ip()));
    }

    let plaintext =
        floodgate::decrypt(payload.as_bytes(), key).map_err(FloodgateLoginError::Rejected)?;
    let data = floodgate::BedrockData::parse(&plaintext).map_err(FloodgateLoginError::Rejected)?;
    let id = data.java_uuid().map_err(FloodgateLoginError::Rejected)?;

    Ok(Some(GameProfile {
        id,
        name: data.java_username(&config.username_prefix),
        properties: vec![],
        profile_actions: None,
    }))
}

/// [`resolve_floodgate`], sourcing the shared key from
/// [`foton_bedrock::key::shared`] instead of taking one as a parameter.
///
/// This is what the login handler should call in production.
///
/// # Security
///
/// An uninitialized shared key (`key::shared() == None`) is a hard reject
/// for any hostname carrying a Floodgate payload -- see
/// [`foton_bedrock::key::shared`]'s documented contract. It is never a
/// fallback to the ordinary login path, and never a retry: uninitialized
/// means Bedrock support never started, which must mean no Floodgate login
/// is ever accepted. A hostname with no Floodgate payload at all is
/// unaffected either way -- it never looks at the key.
pub(crate) fn resolve_floodgate_login(
    hostname: &str,
    peer: SocketAddr,
    config: &BedrockConfig,
) -> Result<Option<GameProfile>, FloodgateLoginError> {
    if !config.enable || floodgate::extract_payload(hostname).is_none() {
        return Ok(None);
    }

    let shared_key = key::shared().ok_or(FloodgateLoginError::KeyUnavailable)?;
    resolve_floodgate(hostname, peer, shared_key, config)
}

/// Whether an address is one Foton accepts Bedrock identity from.
fn is_trusted(address: IpAddr, trusted: &[String]) -> bool {
    trusted
        .iter()
        .filter_map(|entry| entry.parse::<IpAddr>().ok())
        .any(|entry| entry == address)
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use foton_bedrock::floodgate::encrypt;
    use foton_bedrock::key;

    use super::{BedrockConfig, FloodgateLoginError, resolve_floodgate, resolve_floodgate_login};

    /// The 12 fields in `BedrockData.toString()` order, matching
    /// `foton-bedrock`'s own fixture shape, encrypted into a full handshake
    /// hostname.
    fn floodgate_hostname(key: &[u8; 16], username: &str, xuid: &str) -> String {
        let plaintext =
            format!("2\0{username}\0{xuid}\07\0fr_FR\01\02\0127.0.0.1\0null\00\0123\0abcdef");
        let payload = encrypt(&plaintext, key, &[3u8; 12]).expect("test vector encrypts");
        format!("mc.example.com\0{payload}\x00127.0.0.1")
    }

    fn bedrock_config(trusted_proxies: &[&str]) -> BedrockConfig {
        // Only these three fields reach the login path; the rest belong to the
        // supervisor and are left at their defaults on purpose, so a new field
        // there cannot silently change what these tests exercise.
        BedrockConfig {
            enable: true,
            username_prefix: ".".to_owned(),
            trusted_proxies: trusted_proxies
                .iter()
                .map(|entry| (*entry).to_owned())
                .collect(),
            ..BedrockConfig::default()
        }
    }

    fn addr(text: &str) -> SocketAddr {
        text.parse().expect("valid test socket address")
    }

    #[test]
    fn a_floodgate_handshake_from_loopback_produces_the_bedrock_profile() {
        let key = [7u8; 16];
        let hostname = floodgate_hostname(&key, "Steve", "2535428478404012");
        let config = bedrock_config(&["127.0.0.1"]);

        let profile = resolve_floodgate(&hostname, addr("127.0.0.1:52000"), &key, &config)
            .expect("a valid handshake from a trusted address is accepted")
            .expect("hostname carries a Floodgate payload");

        assert_eq!(profile.id.as_u64_pair(), (0, 2_535_428_478_404_012));
        assert_eq!(profile.name, ".Steve");
    }

    #[test]
    fn a_floodgate_handshake_from_an_untrusted_address_is_refused() {
        let key = [7u8; 16];
        let hostname = floodgate_hostname(&key, "Steve", "2535428478404012");
        let config = bedrock_config(&["127.0.0.1"]);

        let result = resolve_floodgate(&hostname, addr("203.0.113.9:52000"), &key, &config);

        // Refused for being untrusted, and never quietly downgraded to an
        // ordinary offline login: that would turn a failed forgery into a
        // successful one.
        assert!(matches!(
            result,
            Err(FloodgateLoginError::UntrustedAddress(_))
        ));
    }

    #[test]
    fn a_forged_handshake_is_refused() {
        let hostname = floodgate_hostname(&[9u8; 16], "Mallory", "1");
        let config = bedrock_config(&["127.0.0.1"]);

        let result = resolve_floodgate(&hostname, addr("127.0.0.1:52000"), &[7u8; 16], &config);

        assert!(matches!(result, Err(FloodgateLoginError::Rejected(_))));
    }

    #[test]
    fn an_ordinary_hostname_is_not_a_floodgate_login() {
        let config = bedrock_config(&["127.0.0.1"]);

        let result = resolve_floodgate(
            "mc.example.com",
            addr("127.0.0.1:52000"),
            &[7u8; 16],
            &config,
        );

        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn a_disabled_config_never_looks_at_the_hostname() {
        let key = [7u8; 16];
        // A well-formed, correctly encrypted payload from a trusted address --
        // the only thing standing between it and acceptance is the config
        // switch, which must be enough on its own.
        let hostname = floodgate_hostname(&key, "Steve", "1");
        let mut config = bedrock_config(&["127.0.0.1"]);
        config.enable = false;

        let result = resolve_floodgate(&hostname, addr("127.0.0.1:52000"), &key, &config);

        assert!(matches!(result, Ok(None)));
    }

    /// `foton_bedrock::key::shared()` is a process-wide `OnceLock` that
    /// nothing in this crate's test binary ever initializes, so it is
    /// reliably `None` here -- exactly the "Bedrock support never started"
    /// state that must hard-refuse rather than fall back to an offline
    /// login.
    #[test]
    fn a_floodgate_handshake_is_refused_when_the_shared_key_is_uninitialized() {
        assert!(
            key::shared().is_none(),
            "no test in this crate initializes the shared key"
        );

        let key = [7u8; 16];
        let hostname = floodgate_hostname(&key, "Steve", "2535428478404012");
        let config = bedrock_config(&["127.0.0.1"]);

        let result = resolve_floodgate_login(&hostname, addr("127.0.0.1:52000"), &config);

        assert!(matches!(result, Err(FloodgateLoginError::KeyUnavailable)));
    }

    #[test]
    fn resolve_floodgate_login_falls_through_for_an_ordinary_hostname_even_without_a_key() {
        let config = bedrock_config(&["127.0.0.1"]);

        let result = resolve_floodgate_login("mc.example.com", addr("127.0.0.1:52000"), &config);

        assert!(matches!(result, Ok(None)));
    }
}
