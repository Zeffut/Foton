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
const AUTH_TOTAL_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_AUTH_RESPONSE_BODY_BYTES: usize = 1024 * 1024;

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
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if is_https_to_http_downgrade(attempt.previous(), attempt.url()) {
                attempt.error(std::io::Error::other(
                    "HTTPS-to-HTTP authentication redirect rejected",
                ))
            } else {
                attempt.follow()
            }
        }))
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
    /// The authentication server returned a profile for a different username.
    #[error("Authentication server returned a mismatched profile name")]
    ProfileNameMismatch,
    /// The authentication server response exceeded the configured size limit.
    #[error("Authentication server response is too large")]
    ResponseTooLarge,
    /// The authentication server attempted an HTTPS-to-HTTP redirect.
    #[error("Authentication server attempted an HTTPS-to-HTTP redirect")]
    RedirectDowngrade,
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
    allow_insecure_auth_server: bool,
) -> Result<GameProfile, AuthError> {
    mojang_authenticate_with_timeout(
        username,
        server_hash,
        auth_server,
        allow_insecure_auth_server,
        AUTH_TOTAL_TIMEOUT,
    )
    .await
}

async fn mojang_authenticate_with_timeout(
    username: &str,
    server_hash: &str,
    auth_server: Option<&str>,
    allow_insecure_auth_server: bool,
    timeout: Duration,
) -> Result<GameProfile, AuthError> {
    let auth_url = build_auth_url(
        auth_server,
        username,
        server_hash,
        allow_insecure_auth_server,
    )?;

    let client = auth_client()?;
    let mut last_error = AuthError::FailedResponse;

    let request = async {
        for _ in 0..MAX_RETRIES {
            let response = match client.get(auth_url.clone()).send().await {
                Ok(response) => response,
                Err(error) if error.is_redirect() => return Err(AuthError::RedirectDowngrade),
                Err(_) => {
                    last_error = AuthError::FailedResponse;
                    continue;
                }
            };

            match response.status() {
                StatusCode::OK => {
                    let body = read_auth_response(response).await?;
                    let profile: GameProfile =
                        serde_json::from_slice(&body).map_err(|_| AuthError::FailedParse)?;
                    if profile.name != username {
                        return Err(AuthError::ProfileNameMismatch);
                    }
                    return Ok(profile);
                }
                StatusCode::NO_CONTENT => return Err(AuthError::UnverifiedUsername),
                status
                    if status.is_server_error()
                        || matches!(
                            status,
                            StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
                        ) =>
                {
                    last_error = AuthError::UnknownStatusCode(status);
                }
                other => return Err(AuthError::UnknownStatusCode(other)),
            }
        }

        Err(last_error)
    };

    tokio::time::timeout(timeout, request)
        .await
        .unwrap_or(Err(AuthError::FailedResponse))
}

async fn read_auth_response(mut response: reqwest::Response) -> Result<Vec<u8>, AuthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AUTH_RESPONSE_BODY_BYTES as u64)
    {
        return Err(AuthError::ResponseTooLarge);
    }

    let mut body = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or_default()
            .min(MAX_AUTH_RESPONSE_BODY_BYTES as u64) as usize,
    );
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AuthError::FailedResponse)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_AUTH_RESPONSE_BODY_BYTES {
            return Err(AuthError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn build_auth_url(
    auth_server: Option<&str>,
    username: &str,
    server_hash: &str,
    allow_insecure_auth_server: bool,
) -> Result<Url, AuthError> {
    let endpoint = auth_server.unwrap_or(DEFAULT_AUTH_SERVER);
    let mut url =
        Url::parse(endpoint).map_err(|_| AuthError::InvalidAuthServer(endpoint.to_string()))?;
    if url.scheme() != "https"
        && !(url.scheme() == "http" && (allow_insecure_auth_server || is_loopback_url(&url)))
    {
        return Err(AuthError::InvalidAuthServer(endpoint.to_string()));
    }
    url.query_pairs_mut()
        .append_pair("username", username)
        .append_pair("serverId", server_hash);
    Ok(url)
}

fn is_loopback_url(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

fn is_https_to_http_downgrade(previous: &[Url], next: &Url) -> bool {
    previous.iter().any(|url| url.scheme() == "https") && next.scheme() == "http"
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
    use std::{
        net::SocketAddr,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
    };

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
        let url = build_auth_url(None, "Steve", "abc123", false).expect("auth URL builds");

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
            false,
        )
        .expect("auth URL builds");

        assert_eq!(
            url.as_str(),
            "https://auth.example.com/session/minecraft/hasJoined?username=Steve&serverId=abc123"
        );
    }

    #[test]
    fn auth_url_rejects_remote_http_without_opt_in() {
        let result = build_auth_url(
            Some("http://auth.example.com/session/minecraft/hasJoined"),
            "Steve",
            "abc123",
            false,
        );

        assert!(matches!(result, Err(AuthError::InvalidAuthServer(_))));
    }

    #[test]
    fn auth_url_accepts_loopback_and_explicitly_opted_in_http() {
        let loopback = build_auth_url(
            Some("http://[::1]/session/minecraft/hasJoined"),
            "Steve",
            "abc123",
            false,
        );
        let opted_in = build_auth_url(
            Some("http://auth.example.com/session/minecraft/hasJoined"),
            "Steve",
            "abc123",
            true,
        );

        assert!(loopback.is_ok());
        assert!(opted_in.is_ok());
    }

    #[test]
    fn https_to_http_redirects_are_rejected() {
        let previous = [Url::parse("https://auth.example.com/session").expect("valid URL")];
        let downgrade = Url::parse("http://auth.example.com/session").expect("valid URL");
        let secure = Url::parse("https://auth.example.com/other").expect("valid URL");

        assert!(is_https_to_http_downgrade(&previous, &downgrade));
        assert!(!is_https_to_http_downgrade(&previous, &secure));
    }

    #[tokio::test]
    async fn oversized_auth_response_is_rejected_before_json_parsing() {
        let body = "x".repeat(MAX_AUTH_RESPONSE_BODY_BYTES + 1);
        let (url, requests, server) = spawn_http_server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
            1,
        )
        .await;

        let result = mojang_authenticate("Steve", "abc123", Some(&url), false).await;

        assert!(matches!(result, Err(AuthError::ResponseTooLarge)));
        assert_eq!(requests.load(Ordering::Relaxed), 1);
        server.await.expect("HTTP test server should finish");
    }

    #[tokio::test]
    async fn slow_auth_response_is_bounded_by_the_total_request_timeout() {
        let (url, server) = spawn_slow_http_server().await;

        let result = mojang_authenticate_with_timeout(
            "Steve",
            "abc123",
            Some(&url),
            false,
            Duration::from_millis(20),
        )
        .await;

        assert!(matches!(result, Err(AuthError::FailedResponse)));
        server.abort();
    }

    #[tokio::test]
    async fn non_transient_auth_status_is_not_retried() {
        let (url, requests, server) = spawn_http_server(
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
            1,
        )
        .await;

        let result = mojang_authenticate("Steve", "abc123", Some(&url), false).await;

        assert!(matches!(
            result,
            Err(AuthError::UnknownStatusCode(StatusCode::NOT_FOUND))
        ));
        assert_eq!(requests.load(Ordering::Relaxed), 1);
        server.await.expect("HTTP test server should finish");
    }

    #[tokio::test]
    async fn transient_auth_status_is_retried() {
        let (url, requests, server) = spawn_http_server(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_owned(),
            MAX_RETRIES as usize,
        )
        .await;

        let result = mojang_authenticate("Steve", "abc123", Some(&url), false).await;

        assert!(matches!(
            result,
            Err(AuthError::UnknownStatusCode(
                StatusCode::SERVICE_UNAVAILABLE
            ))
        ));
        assert_eq!(requests.load(Ordering::Relaxed), MAX_RETRIES as usize);
        server.await.expect("HTTP test server should finish");
    }

    #[tokio::test]
    async fn mismatched_auth_profile_name_is_rejected() {
        let body = r#"{"id":"8667ba71-b85a-4004-af54-457a9734eed7","name":"Alex","properties":[],"profileActions":null}"#;
        let (url, requests, server) = spawn_http_server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
            1,
        )
        .await;

        let result = mojang_authenticate("Steve", "abc123", Some(&url), false).await;

        assert!(matches!(result, Err(AuthError::ProfileNameMismatch)));
        assert_eq!(requests.load(Ordering::Relaxed), 1);
        server.await.expect("HTTP test server should finish");
    }

    async fn spawn_http_server(
        response: String,
        expected_requests: usize,
    ) -> (String, Arc<AtomicUsize>, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("HTTP test server should bind");
        let address = listener
            .local_addr()
            .expect("HTTP test server should have an address");
        let requests = Arc::new(AtomicUsize::new(0));
        let request_count = Arc::clone(&requests);
        let server = tokio::spawn(async move {
            for _ in 0..expected_requests {
                let (mut stream, _) = listener
                    .accept()
                    .await
                    .expect("HTTP test server should accept");
                request_count.fetch_add(1, Ordering::Relaxed);
                let mut request = [0; 4096];
                let _ = stream.read(&mut request).await;
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        (format!("http://{address}/session"), requests, server)
    }

    async fn spawn_slow_http_server() -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("HTTP test server should bind");
        let address: SocketAddr = listener
            .local_addr()
            .expect("HTTP test server should have an address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("HTTP test server should accept");
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n")
                .await
                .expect("HTTP test server should write headers");
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = stream.write_all(b"{}").await;
        });

        (format!("http://{address}/session"), server)
    }
}
