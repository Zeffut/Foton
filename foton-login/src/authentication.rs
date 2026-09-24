//! Mojang authentication implementation.
//!
//! Handles authentication with Mojang's session servers for online mode.

use std::sync::OnceLock;
use std::time::Duration;

use foton_core::player::GameProfile;
use reqwest::{Client, Response, StatusCode, Url, header::RETRY_AFTER};
use thiserror::Error;
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::time::{Instant, sleep_until, timeout, timeout_at};
use tokio_util::sync::CancellationToken;

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
/// Absolute ceiling for one session-server attempt, including the response body.
const AUTH_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Absolute ceiling for every attempt and retry delay combined.
const AUTH_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
/// Session profiles are small JSON documents, even with signed texture data.
const MAX_AUTH_RESPONSE_BYTES: usize = 256 * 1024;
/// Session-service retry delays use short exponential backoff.
const AUTH_RETRY_BASE_MILLIS: u64 = 100;
const AUTH_RETRY_MAX_MILLIS: u64 = 1_000;
/// A remote service may not park a login for an arbitrary `Retry-After`.
const MAX_AUTH_RETRY_AFTER: Duration = Duration::from_secs(5);

/// Maximum number of expensive RSA/session-server authentications in flight.
///
/// Login sockets are accepted before the player cap can apply. Without a
/// separate pre-login bound, a connection flood can make every runtime worker
/// perform private-key operations and hold an HTTP request at the same time.
const AUTHENTICATION_CONCURRENCY_LIMIT: usize = 32;

/// How long a login may wait for one of the bounded authentication slots.
const AUTHENTICATION_QUEUE_TIMEOUT: Duration = Duration::from_secs(5);

static AUTHENTICATION_SLOTS: Semaphore = Semaphore::const_new(AUTHENTICATION_CONCURRENCY_LIMIT);

/// The one HTTP client every session-server call goes through.
///
/// `reqwest::get` builds a fresh client per call, which means a fresh TLS
/// handshake and connection pool for every login.
static AUTH_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Reserves capacity for the complete encrypted-login authentication path.
///
/// The returned permit must be retained across both RSA operations and the
/// session-server request. `None` means the pre-login admission queue remained
/// saturated long enough that retaining this unauthenticated socket is no
/// longer useful.
pub(crate) async fn acquire_authentication_slot() -> Option<SemaphorePermit<'static>> {
    timeout(AUTHENTICATION_QUEUE_TIMEOUT, AUTHENTICATION_SLOTS.acquire())
        .await
        .ok()?
        .ok()
}

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
        .timeout(AUTH_REQUEST_TIMEOUT)
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
    /// Authentication response exceeds the bounded profile-document size.
    #[error("Authentication response is too large")]
    ResponseTooLarge,
    /// Authentication server URL is invalid.
    #[error("Invalid authentication server URL")]
    InvalidAuthServer(String),
    /// The connection closed or the server started shutting down.
    #[error("Authentication was cancelled")]
    Cancelled,
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

enum AuthAttempt {
    Profile(GameProfile),
    Unverified,
    Retry {
        status: StatusCode,
        retry_after: Option<Duration>,
    },
}

/// Authenticates a player with the configured session server.
pub async fn mojang_authenticate(
    username: &str,
    server_hash: &str,
    auth_server: Option<&str>,
) -> Result<GameProfile, AuthError> {
    mojang_authenticate_with_cancel(
        username,
        server_hash,
        auth_server,
        &CancellationToken::new(),
    )
    .await
}

pub(crate) async fn mojang_authenticate_with_cancel(
    username: &str,
    server_hash: &str,
    auth_server: Option<&str>,
    cancellation: &CancellationToken,
) -> Result<GameProfile, AuthError> {
    let auth_url = build_auth_url(auth_server, username, server_hash)?;

    let client = auth_client()?;
    authenticate_with_policy(
        client,
        auth_url,
        username,
        AUTH_TOTAL_TIMEOUT,
        rand::random(),
        cancellation,
    )
    .await
}

async fn authenticate_with_policy(
    client: &Client,
    auth_url: Url,
    username: &str,
    total_timeout: Duration,
    jitter_seed: u64,
    cancellation: &CancellationToken,
) -> Result<GameProfile, AuthError> {
    let deadline = Instant::now() + total_timeout;
    let mut last_error = AuthError::FailedResponse;

    for attempt in 0..MAX_RETRIES {
        let attempt_deadline = deadline.min(Instant::now() + AUTH_REQUEST_TIMEOUT);
        let retry_delay = tokio::select! {
            () = cancellation.cancelled() => return Err(AuthError::Cancelled),
            result = request_auth_attempt(client, auth_url.clone(), attempt_deadline) => match result {
                Ok(AuthAttempt::Profile(profile)) => return Ok(profile),
                Ok(AuthAttempt::Unverified) => return Err(AuthError::UnverifiedUsername),
                Ok(AuthAttempt::Retry {
                    status,
                    retry_after,
                }) => {
                    last_error = if status.is_server_error() {
                        AuthError::FailedResponse
                    } else {
                        AuthError::UnknownStatusCode(status)
                    };
                    transient_retry_delay(attempt, jitter_seed, retry_after)
                }
                Err(AuthError::FailedResponse) => {
                    last_error = AuthError::FailedResponse;
                    transient_retry_delay(attempt, jitter_seed, None)
                }
                Err(error) => return Err(error),
            },
        };

        if attempt + 1 == MAX_RETRIES {
            break;
        }
        if !wait_for_retry(retry_delay, deadline, cancellation).await? {
            return Err(last_error);
        }
    }

    log::warn!("Player {username} auth failed");

    Err(last_error)
}

async fn request_auth_attempt(
    client: &Client,
    auth_url: Url,
    deadline: Instant,
) -> Result<AuthAttempt, AuthError> {
    let response = timeout_at(deadline, client.get(auth_url).send())
        .await
        .map_err(|_| AuthError::FailedResponse)?
        .map_err(|_| AuthError::FailedResponse)?;

    match response.status() {
        StatusCode::OK => read_profile_response(response, deadline)
            .await
            .map(AuthAttempt::Profile),
        // `204` is Mojang's authoritative "has not joined" answer, not a
        // transient service failure. Retrying it turns one invalid login into
        // three externally visible HTTP requests.
        StatusCode::NO_CONTENT => Ok(AuthAttempt::Unverified),
        status if status.is_server_error() => Ok(AuthAttempt::Retry {
            status,
            retry_after: None,
        }),
        StatusCode::TOO_MANY_REQUESTS => Ok(AuthAttempt::Retry {
            status: StatusCode::TOO_MANY_REQUESTS,
            retry_after: Some(bounded_retry_after(&response)?),
        }),
        other => Err(AuthError::UnknownStatusCode(other)),
    }
}

fn bounded_retry_after(response: &Response) -> Result<Duration, AuthError> {
    let error = || AuthError::UnknownStatusCode(StatusCode::TOO_MANY_REQUESTS);
    let seconds = response
        .headers()
        .get(RETRY_AFTER)
        .ok_or_else(error)?
        .to_str()
        .map_err(|_| error())?
        .parse::<u64>()
        .map_err(|_| error())?;
    let delay = Duration::from_secs(seconds);
    if delay > MAX_AUTH_RETRY_AFTER {
        return Err(error());
    }
    Ok(delay)
}

fn server_error_backoff(attempt: u32, jitter_seed: u64) -> Duration {
    let exponent = 1_u64 << attempt.min(3);
    let base = AUTH_RETRY_BASE_MILLIS
        .saturating_mul(exponent)
        .min(AUTH_RETRY_MAX_MILLIS);
    let mixed = jitter_seed
        .wrapping_add(u64::from(attempt).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let jitter = (mixed ^ (mixed >> 31)) % (base / 2 + 1);
    Duration::from_millis(base + jitter)
}

fn transient_retry_delay(
    attempt: u32,
    jitter_seed: u64,
    retry_after: Option<Duration>,
) -> Duration {
    let backoff = server_error_backoff(attempt, jitter_seed);
    retry_after.map_or(backoff, |delay| delay.max(backoff))
}

async fn wait_for_retry(
    delay: Duration,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<bool, AuthError> {
    let Some(wake_at) = Instant::now().checked_add(delay) else {
        return Ok(false);
    };
    if wake_at > deadline {
        return Ok(false);
    }
    tokio::select! {
        () = cancellation.cancelled() => Err(AuthError::Cancelled),
        () = sleep_until(wake_at) => Ok(true),
    }
}

async fn read_profile_response(
    mut response: Response,
    deadline: Instant,
) -> Result<GameProfile, AuthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AUTH_RESPONSE_BYTES as u64)
    {
        return Err(AuthError::ResponseTooLarge);
    }

    let initial_capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default()
        .min(MAX_AUTH_RESPONSE_BYTES);
    let mut body = Vec::with_capacity(initial_capacity);

    loop {
        let chunk = timeout_at(deadline, response.chunk())
            .await
            .map_err(|_| AuthError::FailedResponse)?
            .map_err(|_| AuthError::FailedResponse)?;
        let Some(chunk) = chunk else { break };
        let Some(new_len) = body.len().checked_add(chunk.len()) else {
            return Err(AuthError::ResponseTooLarge);
        };
        if new_len > MAX_AUTH_RESPONSE_BYTES {
            return Err(AuthError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&body).map_err(|_| AuthError::FailedParse)
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
    use std::{
        future::Future,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::*;
    use sha2::Digest;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::task::{JoinHandle, yield_now};
    use tokio::time::sleep;

    async fn auth_server_with_responses(
        responses: Vec<&'static str>,
    ) -> (String, Arc<AtomicUsize>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test auth listener binds");
        let address = listener
            .local_addr()
            .expect("test auth listener has an address");
        let requests = Arc::new(AtomicUsize::new(0));
        let request_count = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener
                    .accept()
                    .await
                    .expect("test auth listener accepts request");
                let mut request = [0; 2_048];
                let _ = stream
                    .read(&mut request)
                    .await
                    .expect("test auth listener reads request");
                request_count.fetch_add(1, Ordering::SeqCst);
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("test auth listener writes response");
            }
        });
        (
            format!("http://{address}/session/minecraft/hasJoined"),
            requests,
            task,
        )
    }

    async fn one_shot_auth_server<F, Fut>(respond: F) -> (Url, JoinHandle<()>)
    where
        F: FnOnce(TcpStream) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test auth listener binds");
        let address = listener
            .local_addr()
            .expect("test auth listener has an address");
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("test auth listener accepts request");
            let mut request = [0; 2_048];
            let _ = stream
                .read(&mut request)
                .await
                .expect("test auth listener reads request");
            respond(stream).await;
        });
        (
            Url::parse(&format!("http://{address}/session/minecraft/hasJoined"))
                .expect("test auth URL parses"),
            task,
        )
    }

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

    #[tokio::test]
    async fn unverified_login_is_not_retried() {
        let (endpoint, requests, server) = auth_server_with_responses(vec![
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;

        let result = mojang_authenticate("Mallory", "invalid", Some(&endpoint)).await;

        assert!(matches!(result, Err(AuthError::UnverifiedUsername)));
        server.await.expect("test auth server completes");
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn transient_auth_failures_keep_the_bounded_retry() {
        let (endpoint, requests, server) = auth_server_with_responses(vec![
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;

        let result = mojang_authenticate("Mallory", "invalid", Some(&endpoint)).await;

        assert!(matches!(result, Err(AuthError::UnverifiedUsername)));
        server.await.expect("test auth server completes");
        assert_eq!(requests.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn rate_limit_retries_only_with_a_bounded_retry_after() {
        let (endpoint, requests, server) = auth_server_with_responses(vec![
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;

        let result = mojang_authenticate("Mallory", "invalid", Some(&endpoint)).await;

        assert!(matches!(result, Err(AuthError::UnverifiedUsername)));
        server.await.expect("test auth server completes");
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn missing_or_excessive_retry_after_fails_without_retrying() {
        for response in [
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 6\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ] {
            let (endpoint, requests, server) = auth_server_with_responses(vec![response]).await;

            let result = mojang_authenticate("Mallory", "invalid", Some(&endpoint)).await;

            assert!(matches!(
                result,
                Err(AuthError::UnknownStatusCode(StatusCode::TOO_MANY_REQUESTS))
            ));
            server.await.expect("test auth server completes");
            assert_eq!(requests.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn server_error_backoff_is_bounded_and_deterministic() {
        for (attempt, minimum, maximum) in [(0, 100, 150), (1, 200, 300), (2, 400, 600)] {
            let delay = server_error_backoff(attempt, 0x1234);
            assert_eq!(delay, server_error_backoff(attempt, 0x1234));
            assert!(delay >= Duration::from_millis(minimum));
            assert!(delay <= Duration::from_millis(maximum));
        }
    }

    #[test]
    fn retry_after_never_shortens_the_jittered_backoff() {
        let backoff = server_error_backoff(0, 0x1234);

        assert_eq!(
            transient_retry_delay(0, 0x1234, Some(Duration::ZERO)),
            backoff
        );
        assert_eq!(
            transient_retry_delay(0, 0x1234, Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
    }

    #[tokio::test]
    async fn exhausted_server_errors_report_the_authentication_service_as_down() {
        let (endpoint, requests, server) = auth_server_with_responses(vec![
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let auth_url =
            build_auth_url(Some(&endpoint), "Mallory", "invalid").expect("test auth URL builds");

        let result = authenticate_with_policy(
            &Client::new(),
            auth_url,
            "Mallory",
            Duration::from_secs(2),
            0x1234,
            &CancellationToken::new(),
        )
        .await;

        assert!(matches!(result, Err(AuthError::FailedResponse)));
        server.await.expect("test auth server completes");
        assert_eq!(requests.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_backoff_cannot_outlive_the_total_authentication_deadline() {
        let (endpoint, requests, server) = auth_server_with_responses(vec![
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let auth_url =
            build_auth_url(Some(&endpoint), "Mallory", "invalid").expect("test auth URL builds");

        let result = authenticate_with_policy(
            &Client::new(),
            auth_url,
            "Mallory",
            Duration::from_millis(50),
            0x1234,
            &CancellationToken::new(),
        )
        .await;

        assert!(matches!(result, Err(AuthError::FailedResponse)));
        server.await.expect("test auth server completes");
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancellation_interrupts_authentication_retry_work() {
        let (endpoint, requests, server) = auth_server_with_responses(vec![
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let auth_url =
            build_auth_url(Some(&endpoint), "Mallory", "invalid").expect("test auth URL builds");
        let cancellation = CancellationToken::new();
        let cancel_after_response = cancellation.clone();
        let request_count = Arc::clone(&requests);
        tokio::spawn(async move {
            while request_count.load(Ordering::SeqCst) == 0 {
                yield_now().await;
            }
            cancel_after_response.cancel();
        });

        let result = timeout(
            Duration::from_millis(75),
            authenticate_with_policy(
                &Client::new(),
                auth_url,
                "Mallory",
                Duration::from_secs(5),
                0x1234,
                &cancellation,
            ),
        )
        .await
        .expect("cancellation must interrupt the retry delay");

        assert!(matches!(result, Err(AuthError::Cancelled)));
        server.await.expect("test auth server completes");
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn chunked_auth_response_is_rejected_at_the_streaming_size_limit() {
        let (endpoint, server) = one_shot_auth_server(|mut stream| async move {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("test auth server writes headers");
            let chunk = vec![b'x'; 4_096];
            let header = format!("{:x}\r\n", chunk.len());
            for _ in 0..=(MAX_AUTH_RESPONSE_BYTES / chunk.len()) {
                if stream.write_all(header.as_bytes()).await.is_err()
                    || stream.write_all(&chunk).await.is_err()
                    || stream.write_all(b"\r\n").await.is_err()
                {
                    break;
                }
            }
        })
        .await;

        let result = request_auth_attempt(
            &Client::new(),
            endpoint,
            Instant::now() + Duration::from_secs(2),
        )
        .await;

        assert!(matches!(result, Err(AuthError::ResponseTooLarge)));
        server.await.expect("test auth server completes");
    }

    #[tokio::test]
    async fn trickled_auth_body_cannot_extend_the_absolute_attempt_deadline() {
        let (endpoint, server) = one_shot_auth_server(|mut stream| async move {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n")
                .await
                .expect("test auth server writes headers");
            for byte in [b'{'; 100] {
                if stream.write_all(&[byte]).await.is_err() {
                    break;
                }
                sleep(Duration::from_millis(25)).await;
            }
        })
        .await;

        let result = request_auth_attempt(
            &Client::new(),
            endpoint,
            Instant::now() + Duration::from_millis(80),
        )
        .await;

        assert!(matches!(result, Err(AuthError::FailedResponse)));
        server.await.expect("test auth server completes");
    }

    #[tokio::test]
    async fn authentication_slots_bound_parallel_expensive_work() {
        let mut permits = Vec::with_capacity(AUTHENTICATION_CONCURRENCY_LIMIT);
        for _ in 0..AUTHENTICATION_CONCURRENCY_LIMIT {
            permits.push(
                acquire_authentication_slot()
                    .await
                    .expect("configured authentication slot is available"),
            );
        }

        assert!(
            timeout(Duration::from_millis(25), AUTHENTICATION_SLOTS.acquire())
                .await
                .is_err()
        );
        drop(permits.pop());
        let replacement = timeout(Duration::from_millis(250), AUTHENTICATION_SLOTS.acquire())
            .await
            .expect("released authentication slot wakes a waiter")
            .expect("authentication semaphore stays open");
        drop(replacement);
        drop(permits);
    }
}
