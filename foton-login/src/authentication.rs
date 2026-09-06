//! Mojang authentication implementation.
//!
//! Handles authentication with Mojang's session servers for online mode.

use std::sync::OnceLock;
use std::time::Duration;

use foton_core::player::GameProfile;
use reqwest::{StatusCode, Url};
use thiserror::Error;

const DEFAULT_AUTH_SERVER: &str = "https://sessionserver.mojang.com/session/minecraft/hasJoined";

/// Connect and read timeouts for a session-server call.
///
/// `reqwest`'s default client has neither, and this call sits on the login
/// path: `tcp_client` wraps only `get_raw_packet` in `PRE_PLAY_READ_TIMEOUT`,
/// so an unbounded request here holds the connection task, its socket and its
/// buffers open forever, past shutdown, with nothing to interrupt it. A session
/// server that accepts the TCP connection and then answers nothing is enough --
/// so is a misconfigured `auth_server` pointing at a black hole.
///
/// The five seconds match what the rest of the workspace already gives an
/// identity service (`server::service_keys`, `player::profile::lookup`), and
/// three attempts still fit inside a client's own login patience.
const AUTH_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const AUTH_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The one HTTP client every session-server call goes through.
///
/// `reqwest::get` builds a fresh client per call, which means a fresh TLS
/// handshake and connection pool for every login.
static AUTH_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn auth_client() -> Result<&'static reqwest::Client, AuthError> {
    if let Some(client) = AUTH_CLIENT.get() {
        return Ok(client);
    }

    // Building a client fails only when the TLS backend cannot initialize,
    // which is indistinguishable from the session service being unreachable
    // as far as the joining player is concerned.
    let client = reqwest::Client::builder()
        .connect_timeout(AUTH_CONNECT_TIMEOUT)
        .read_timeout(AUTH_READ_TIMEOUT)
        .build()
        .map_err(|_| AuthError::FailedResponse)?;

    Ok(AUTH_CLIENT.get_or_init(|| client))
}

/// An error that can occur during Mojang authentication.
#[derive(Error, Debug)]
pub enum AuthError {
    /// Authentication servers are down.
    #[error("Authentication servers are down")]
    FailedResponse,
    /// Failed to verify username.
    #[error("Failed to verify username")]
    UnverifiedUsername,
    /// You are banned from Authentication servers.
    #[error("You are banned from Authentication servers")]
    Banned,
    /// An error occurred with textures.
    #[error("Texture Error {0}")]
    TextureError(TextureError),
    /// You have disallowed actions from Authentication servers.
    #[error("You have disallowed actions from Authentication servers")]
    DisallowedAction,
    /// Failed to parse JSON into Game Profile.
    #[error("Failed to parse JSON into Game Profile")]
    FailedParse,
    /// Authentication server URL is invalid.
    #[error("Invalid authentication server URL")]
    InvalidAuthServer(String),
    /// An unknown status code was returned.
    #[error("Unknown Status Code {0}")]
    UnknownStatusCode(StatusCode),
}

/// An error that can occur with textures.
#[derive(Error, Debug)]
pub enum TextureError {
    /// Invalid URL.
    #[error("Invalid URL")]
    InvalidURL,
    /// Invalid URL scheme for player texture.
    #[error("Invalid URL scheme for player texture: {0}")]
    DisallowedUrlScheme(String),
    /// Invalid URL domain for player texture.
    #[error("Invalid URL domain for player texture: {0}")]
    DisallowedUrlDomain(String),
    /// Failed to decode base64 player texture.
    #[error("Failed to decode base64 player texture: {0}")]
    DecodeError(String),
    /// Failed to parse JSON from player texture.
    #[error("Failed to parse JSON from player texture: {0}")]
    JSONError(String),
}

const MAX_RETRIES: u32 = 3;

/// Authenticates a player with the configured session server.
pub async fn mojang_authenticate(
    username: &str,
    server_hash: &str,
    auth_server: Option<&str>,
) -> Result<GameProfile, AuthError> {
    let auth_url = build_auth_url(auth_server, username, server_hash)?;

    let client = auth_client()?;
    let mut last_error = AuthError::FailedResponse;

    for _ in 0..MAX_RETRIES {
        let Ok(response) = client.get(auth_url.clone()).send().await else {
            last_error = AuthError::FailedResponse;
            continue;
        };

        match response.status() {
            StatusCode::OK => {
                return response.json().await.map_err(|_| AuthError::FailedParse);
            }
            StatusCode::NO_CONTENT => last_error = AuthError::UnverifiedUsername,
            other => last_error = AuthError::UnknownStatusCode(other),
        }
    }

    log::warn!("Player {username} auth failed");

    Err(last_error)
}

fn build_auth_url(
    auth_server: Option<&str>,
    username: &str,
    server_hash: &str,
) -> Result<Url, AuthError> {
    let endpoint = auth_server.unwrap_or(DEFAULT_AUTH_SERVER);
    let mut url =
        Url::parse(endpoint).map_err(|_| AuthError::InvalidAuthServer(endpoint.to_string()))?;
    url.query_pairs_mut()
        .append_pair("username", username)
        .append_pair("serverId", server_hash);
    Ok(url)
}

/// Converts a signed bytes big endian to a hex string.
///
/// Equivalent to Java's `new BigInteger(bytes).toString(16)`.
/// The first byte determines sign (two's complement). Leading zero bytes
/// are not significant for magnitude but ARE significant for sign.
#[must_use]
pub fn signed_bytes_be_to_hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "0".to_string();
    }

    // Sign is determined by the MSB of the first byte in the full array,
    // NOT after stripping leading zeros. A leading 0x00 byte means the
    // number is positive even if the next byte has its high bit set.
    let is_negative = (bytes[0] & 0x80) != 0;

    if is_negative {
        // Negative case: calculate two's complement of the full byte array.
        let mut magnitude: Vec<u8> = bytes.iter().map(|b| !*b).collect();
        for byte in magnitude.iter_mut().rev() {
            let (result, carry) = byte.overflowing_add(1);
            *byte = result;
            if !carry {
                break;
            }
        }

        let hex = hex::encode(&magnitude);
        let trimmed = hex.trim_start_matches('0');
        format!("-{trimmed}")
    } else {
        let hex = hex::encode(bytes);
        let trimmed = hex.trim_start_matches('0');
        if trimmed.is_empty() {
            "0".to_string()
        } else {
            trimmed.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn test_positive_simple() {
        // BigInteger([0x01, 0x2a]).toString(16) = "12a"
        assert_eq!(signed_bytes_be_to_hex(&[0x01, 0x2a]), "12a");
    }

    #[test]
    fn test_negative_simple() {
        // BigInteger([0xff]).toString(16) = "-1"
        assert_eq!(signed_bytes_be_to_hex(&[0xff]), "-1");
    }

    #[test]
    fn test_leading_zero_preserves_positive_sign() {
        // This was the bug: [0x00, 0x9a] is positive because the first byte is 0x00.
        // BigInteger([0x00, 0x9a]).toString(16) = "9a"
        assert_eq!(signed_bytes_be_to_hex(&[0x00, 0x9a]), "9a");
    }

    #[test]
    fn test_leading_zero_with_high_bit() {
        // BigInteger([0x00, 0xff, 0xab]).toString(16) = "ffab"
        assert_eq!(signed_bytes_be_to_hex(&[0x00, 0xff, 0xab]), "ffab");
    }

    #[test]
    fn test_negative_twos_complement() {
        // BigInteger([0x80]).toString(16) = "-80"
        assert_eq!(signed_bytes_be_to_hex(&[0x80]), "-80");
    }

    #[test]
    fn test_negative_multi_byte() {
        // BigInteger([0xfe, 0xdc]).toString(16) = "-124"
        assert_eq!(signed_bytes_be_to_hex(&[0xfe, 0xdc]), "-124");
    }

    #[test]
    fn test_zero() {
        assert_eq!(signed_bytes_be_to_hex(&[0x00]), "0");
        assert_eq!(signed_bytes_be_to_hex(&[0x00, 0x00, 0x00]), "0");
    }

    #[test]
    fn test_empty() {
        assert_eq!(signed_bytes_be_to_hex(&[]), "0");
    }

    #[test]
    fn test_known_notchian_hashes() {
        // Known test vectors from wiki.vg
        assert_eq!(
            signed_bytes_be_to_hex(&sha1::Sha1::digest(b"Notch")),
            "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48"
        );
        assert_eq!(
            signed_bytes_be_to_hex(&sha1::Sha1::digest(b"jeb_")),
            "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1"
        );
        assert_eq!(
            signed_bytes_be_to_hex(&sha1::Sha1::digest(b"simon")),
            "88e16a1019277b15d58faf0541e11910eb756f6"
        );
    }

    #[test]
    fn auth_url_defaults_to_mojang_session_server() {
        let url = build_auth_url(None, "Steve", "abc123").expect("auth URL builds");

        assert_eq!(
            url.as_str(),
            "https://sessionserver.mojang.com/session/minecraft/hasJoined?username=Steve&serverId=abc123"
        );
    }

    #[test]
    fn auth_url_uses_configured_endpoint() {
        let url = build_auth_url(
            Some("https://auth.example.com/session/minecraft/hasJoined"),
            "Steve",
            "abc123",
        )
        .expect("auth URL builds");

        assert_eq!(
            url.as_str(),
            "https://auth.example.com/session/minecraft/hasJoined?username=Steve&serverId=abc123"
        );
    }
}
