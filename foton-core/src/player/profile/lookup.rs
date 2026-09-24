//! Online player name-to-identity lookup.

use reqwest::{StatusCode, Url};
use serde::Deserialize;
use thiserror::Error;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use super::known_players::KnownPlayer;

const DEFAULT_PROFILE_SERVER: &str =
    "https://api.minecraftservices.com/minecraft/profile/lookup/name";
const MAX_PROFILE_LOOKUP_ATTEMPTS: usize = 3;
const PROFILE_LOOKUP_RETRY_DELAY: Duration = Duration::from_millis(750);
/// Bounds every attempt so suspended administrative commands always release their ordering barrier.
const PROFILE_LOOKUP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PROFILE_RESPONSE_BYTES: usize = 64 * 1024;

/// Failure while resolving a player identity through the configured profile service.
#[derive(Debug, Error)]
pub enum ProfileLookupError {
    /// No profile exists for the requested name.
    #[error("Unknown player {0}")]
    UnknownPlayer(String),
    /// The configured profile lookup endpoint is invalid.
    #[error("Invalid profile server URL configured: {0}")]
    InvalidProfileServer(String),
    /// The request failed before a response was received.
    #[error("Profile lookup failed for {name}: {source}")]
    Request {
        /// Requested player name.
        name: String,
        /// Transport error.
        source: reqwest::Error,
    },
    /// The service returned an unexpected status.
    #[error("Profile lookup service returned status {status} for {name}")]
    ServiceResponse {
        /// Requested player name.
        name: String,
        /// HTTP response status.
        status: StatusCode,
    },
    /// The service returned malformed identity data.
    #[error("Invalid profile lookup response for {name}: {reason}")]
    InvalidResponse {
        /// Requested player name.
        name: String,
        /// Response validation failure.
        reason: String,
    },
}

#[derive(Deserialize)]
struct ProfileLookupResponse {
    id: String,
    name: String,
}

/// Resolves one online-mode profile through the configured service.
///
/// The caller handles local caches, offline mode, and name validation first.
#[cfg_attr(
    not(test),
    expect(
        clippy::unreachable,
        reason = "packet dispatch is split by kind before this point, and the player's inventory keeps the concrete type it was created with"
    )
)]
pub async fn lookup_online_profile(
    client: &reqwest::Client,
    profile_server: Option<&str>,
    name: &str,
) -> Result<KnownPlayer, ProfileLookupError> {
    let lookup_name = name.to_ascii_lowercase();
    let url = profile_lookup_url(profile_server, &lookup_name)?;
    for attempt in 1..=MAX_PROFILE_LOOKUP_ATTEMPTS {
        let result =
            lookup_online_profile_once(client, url.as_str(), name, PROFILE_LOOKUP_REQUEST_TIMEOUT)
                .await;
        match result {
            Ok(profile) => return Ok(profile),
            Err(error @ ProfileLookupError::UnknownPlayer(_)) => return Err(error),
            Err(error) if attempt == MAX_PROFILE_LOOKUP_ATTEMPTS => return Err(error),
            Err(_) => sleep(PROFILE_LOOKUP_RETRY_DELAY).await,
        }
    }
    unreachable!("the profile lookup attempt range is non-empty")
}

async fn lookup_online_profile_once(
    client: &reqwest::Client,
    url: &str,
    name: &str,
    request_timeout: Duration,
) -> Result<KnownPlayer, ProfileLookupError> {
    let response = client
        .get(url)
        .timeout(request_timeout)
        .send()
        .await
        .map_err(|source| ProfileLookupError::Request {
            name: name.to_owned(),
            source,
        })?;

    match response.status() {
        StatusCode::OK => parse_profile_response(response, name).await,
        StatusCode::NO_CONTENT | StatusCode::NOT_FOUND => {
            Err(ProfileLookupError::UnknownPlayer(name.to_owned()))
        }
        status => Err(ProfileLookupError::ServiceResponse {
            name: name.to_owned(),
            status,
        }),
    }
}

fn profile_lookup_url(
    profile_server: Option<&str>,
    normalized_name: &str,
) -> Result<Url, ProfileLookupError> {
    let server = profile_server.unwrap_or(DEFAULT_PROFILE_SERVER);
    let endpoint = format!("{}/{normalized_name}", server.trim_end_matches('/'));
    Url::parse(&endpoint).map_err(|_| ProfileLookupError::InvalidProfileServer(endpoint))
}

async fn parse_profile_response(
    mut response: reqwest::Response,
    requested_name: &str,
) -> Result<KnownPlayer, ProfileLookupError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PROFILE_RESPONSE_BYTES as u64)
    {
        return Err(ProfileLookupError::InvalidResponse {
            name: requested_name.to_owned(),
            reason: format!("response exceeds the {MAX_PROFILE_RESPONSE_BYTES}-byte limit"),
        });
    }

    let mut body = Vec::new();
    while let Some(chunk) =
        response
            .chunk()
            .await
            .map_err(|source| ProfileLookupError::InvalidResponse {
                name: requested_name.to_owned(),
                reason: source.to_string(),
            })?
    {
        let remaining = MAX_PROFILE_RESPONSE_BYTES.saturating_sub(body.len());
        if chunk.len() > remaining {
            return Err(ProfileLookupError::InvalidResponse {
                name: requested_name.to_owned(),
                reason: format!("response exceeds the {MAX_PROFILE_RESPONSE_BYTES}-byte limit"),
            });
        }
        body.extend_from_slice(&chunk);
    }
    let profile = serde_json::from_slice::<ProfileLookupResponse>(&body).map_err(|source| {
        ProfileLookupError::InvalidResponse {
            name: requested_name.to_owned(),
            reason: source.to_string(),
        }
    })?;
    let uuid =
        Uuid::parse_str(&profile.id).map_err(|source| ProfileLookupError::InvalidResponse {
            name: requested_name.to_owned(),
            reason: source.to_string(),
        })?;
    Ok(KnownPlayer::new(uuid, profile.name))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::{
        MAX_PROFILE_RESPONSE_BYTES, ProfileLookupError, lookup_online_profile_once,
        profile_lookup_url,
    };

    #[test]
    fn profile_lookup_url_uses_mojangs_default_endpoint() {
        let url = profile_lookup_url(None, "steve");
        let Ok(url) = url else {
            panic!("default profile lookup URL should build");
        };
        assert_eq!(
            url.as_str(),
            "https://api.minecraftservices.com/minecraft/profile/lookup/name/steve"
        );
    }

    #[test]
    fn profile_lookup_url_uses_the_configured_endpoint() {
        let url = profile_lookup_url(Some("https://profiles.example.com/lookup/"), "steve");
        let Ok(url) = url else {
            panic!("configured profile lookup URL should build");
        };
        assert_eq!(url.as_str(), "https://profiles.example.com/lookup/steve");
    }

    #[tokio::test]
    async fn rejects_an_oversized_profile_response_from_its_headers() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("client should connect");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = socket
                    .read(&mut chunk)
                    .await
                    .expect("request should be readable");
                assert_ne!(count, 0, "request ended before its headers");
                request.extend_from_slice(&chunk[..count]);
            }
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_PROFILE_RESPONSE_BYTES + 1
            );
            socket
                .write_all(headers.as_bytes())
                .await
                .expect("headers should write");
        });
        let client = reqwest::Client::new();
        let url = format!("http://{address}/steve");

        let result =
            lookup_online_profile_once(&client, &url, "Steve", Duration::from_secs(1)).await;

        assert!(matches!(
            result,
            Err(ProfileLookupError::InvalidResponse { reason, .. })
                if reason.contains("exceeds")
        ));
        server.await.expect("test server should stop cleanly");
    }
}
