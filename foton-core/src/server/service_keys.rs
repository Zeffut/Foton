use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwapOption;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use foton_crypto::{CryptError, public_key_from_bytes, signature::ProfileKeyValidator};
use serde::Deserialize;
use thiserror::Error;
use tokio::{sync::oneshot, time};
use tokio_util::sync::CancellationToken;

const DEFAULT_SERVICES_SERVER: &str = "https://api.minecraftservices.com/publickeys";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const SERVICE_KEYS_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SERVICE_KEYS_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_SERVICE_KEYS_PER_KIND: usize = 64;
const MAX_SERVICE_KEY_ENCODED_BYTES: usize = 16 * 1024;
const MAX_SERVICE_KEY_DER_BYTES: usize = 12 * 1024;
const DAILY_REFRESH_INTERVAL: Duration = Duration::from_hours(24);
const BASE_FAILURE_INTERVAL: Duration = Duration::from_mins(5);
const MAX_BACKOFF_EXPONENT: u32 = 6;

#[derive(Debug, Error)]
pub(super) enum ServiceKeyError {
    #[error("invalid services key endpoint '{endpoint}': {reason}")]
    InvalidEndpoint { endpoint: String, reason: String },
    #[error("failed to build services key HTTP client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("services key request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("services key request exceeded its {timeout:?} deadline")]
    RequestDeadlineExceeded { timeout: Duration },
    #[error("services key response exceeds the {limit}-byte limit")]
    ResponseTooLarge { limit: usize },
    #[error("services key response is not valid JSON: {0}")]
    InvalidResponse(#[from] serde_json::Error),
    #[error("services key response contains {count} keys; the limit is {limit}")]
    TooManyKeys { count: usize, limit: usize },
    #[error("services key contains {size} base64 bytes; the limit is {limit}")]
    EncodedKeyTooLarge { size: usize, limit: usize },
    #[error("services key contains {size} decoded bytes; the limit is {limit}")]
    DecodedKeyTooLarge { size: usize, limit: usize },
    #[error("services key response contains invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("services key response contains an invalid public key: {0}")]
    PublicKey(#[from] CryptError),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceKeyResponse {
    profile_property_keys: Option<Vec<ServiceKeyData>>,
    player_certificate_keys: Option<Vec<ServiceKeyData>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceKeyData {
    public_key: String,
}

/// Cached Mojang service keys used to validate player-key certificates.
pub(super) struct ServiceKeyStore {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    profile_key_validator: ArcSwapOption<ProfileKeyValidator>,
}

impl ServiceKeyStore {
    pub(super) fn new(endpoint: Option<&str>) -> Result<Self, ServiceKeyError> {
        let endpoint = endpoint.unwrap_or(DEFAULT_SERVICES_SERVER);
        let parsed_endpoint =
            reqwest::Url::parse(endpoint).map_err(|error| ServiceKeyError::InvalidEndpoint {
                endpoint: endpoint.to_owned(),
                reason: error.to_string(),
            })?;
        if !matches!(parsed_endpoint.scheme(), "http" | "https") {
            return Err(ServiceKeyError::InvalidEndpoint {
                endpoint: endpoint.to_owned(),
                reason: "expected http or https".to_owned(),
            });
        }
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()
            .map_err(ServiceKeyError::Client)?;

        Ok(Self {
            client,
            endpoint: parsed_endpoint,
            profile_key_validator: ArcSwapOption::empty(),
        })
    }

    pub(super) fn profile_key_validator(&self) -> Option<Arc<ProfileKeyValidator>> {
        self.profile_key_validator.load_full()
    }

    /// Starts the initial fetch and returns a signal completed after its first attempt.
    pub(super) fn start(
        self: &Arc<Self>,
        cancel_token: CancellationToken,
    ) -> oneshot::Receiver<()> {
        let (ready_tx, ready_rx) = oneshot::channel();
        let store = Arc::clone(self);
        drop(tokio::spawn(async move {
            let mut has_successful_snapshot = match store.refresh().await {
                Ok(()) => true,
                Err(error) => {
                    log::warn!("Failed to load Minecraft services public keys: {error}");
                    false
                }
            };
            let _ = ready_tx.send(());

            let mut failure_count = 0;
            loop {
                let delay = if has_successful_snapshot {
                    DAILY_REFRESH_INTERVAL
                } else {
                    failure_delay(failure_count)
                };
                tokio::select! {
                    () = cancel_token.cancelled() => return,
                    () = time::sleep(delay) => {}
                }

                match store.refresh().await {
                    Ok(()) => {
                        has_successful_snapshot = true;
                        failure_count = 0;
                    }
                    Err(error) => {
                        log::warn!("Failed to refresh Minecraft services public keys: {error}");
                        if !has_successful_snapshot {
                            failure_count = failure_count.saturating_add(1);
                        }
                    }
                }
            }
        }));
        ready_rx
    }

    async fn refresh(&self) -> Result<(), ServiceKeyError> {
        self.refresh_with_timeout(SERVICE_KEYS_REQUEST_TIMEOUT)
            .await
    }

    async fn refresh_with_timeout(&self, request_timeout: Duration) -> Result<(), ServiceKeyError> {
        let response = time::timeout(request_timeout, self.fetch_response())
            .await
            .map_err(|_| ServiceKeyError::RequestDeadlineExceeded {
                timeout: request_timeout,
            })??;
        let validator = profile_key_validator(response)?;
        self.profile_key_validator.store(validator.map(Arc::new));
        Ok(())
    }

    async fn fetch_response(&self) -> Result<ServiceKeyResponse, ServiceKeyError> {
        let response = self
            .client
            .get(self.endpoint.clone())
            .send()
            .await?
            .error_for_status()?;
        let body = read_bounded_response(response, MAX_SERVICE_KEYS_RESPONSE_BYTES).await?;
        serde_json::from_slice(&body).map_err(ServiceKeyError::from)
    }
}

async fn read_bounded_response(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, ServiceKeyError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(ServiceKeyError::ResponseTooLarge { limit });
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = limit.saturating_sub(body.len());
        if chunk.len() > remaining {
            return Err(ServiceKeyError::ResponseTooLarge { limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn profile_key_validator(
    response: ServiceKeyResponse,
) -> Result<Option<ProfileKeyValidator>, ServiceKeyError> {
    // Authlib rejects the complete snapshot when either service-key list is malformed.
    parse_keys(response.profile_property_keys)?;
    let keys = parse_keys(response.player_certificate_keys)?;
    Ok(ProfileKeyValidator::new(keys))
}

fn parse_keys(
    keys: Option<Vec<ServiceKeyData>>,
) -> Result<Vec<rsa::RsaPublicKey>, ServiceKeyError> {
    let keys = keys.unwrap_or_default();
    if keys.len() > MAX_SERVICE_KEYS_PER_KIND {
        return Err(ServiceKeyError::TooManyKeys {
            count: keys.len(),
            limit: MAX_SERVICE_KEYS_PER_KIND,
        });
    }

    keys.into_iter()
        .map(|key| {
            if key.public_key.len() > MAX_SERVICE_KEY_ENCODED_BYTES {
                return Err(ServiceKeyError::EncodedKeyTooLarge {
                    size: key.public_key.len(),
                    limit: MAX_SERVICE_KEY_ENCODED_BYTES,
                });
            }
            let der = BASE64_STANDARD.decode(key.public_key)?;
            if der.len() > MAX_SERVICE_KEY_DER_BYTES {
                return Err(ServiceKeyError::DecodedKeyTooLarge {
                    size: der.len(),
                    limit: MAX_SERVICE_KEY_DER_BYTES,
                });
            }
            public_key_from_bytes(&der).map_err(ServiceKeyError::from)
        })
        .collect()
}

fn failure_delay(failure_count: u32) -> Duration {
    let exponent = failure_count.min(MAX_BACKOFF_EXPONENT);
    BASE_FAILURE_INTERVAL.saturating_mul(1 << exponent)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use base64::{Engine as _, prelude::BASE64_STANDARD};
    use foton_crypto::{generate_key_pair, public_key_to_bytes};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        time::sleep,
    };

    use super::{
        BASE_FAILURE_INTERVAL, MAX_BACKOFF_EXPONENT, MAX_SERVICE_KEY_ENCODED_BYTES,
        MAX_SERVICE_KEYS_PER_KIND, MAX_SERVICE_KEYS_RESPONSE_BYTES, ServiceKeyData,
        ServiceKeyError, ServiceKeyResponse, ServiceKeyStore, failure_delay, profile_key_validator,
    };

    async fn read_request(socket: &mut TcpStream) {
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
    }

    #[test]
    fn parses_player_certificate_keys() {
        let (_, public_key) = generate_key_pair().expect("test RSA key should generate");
        let der = public_key_to_bytes(&public_key).expect("test RSA key should encode");
        let response = serde_json::from_value::<ServiceKeyResponse>(serde_json::json!({
            "profilePropertyKeys": [],
            "playerCertificateKeys": [{ "publicKey": BASE64_STANDARD.encode(der) }],
        }))
        .expect("Minecraft services response should deserialize");

        assert!(
            profile_key_validator(response)
                .expect("valid response should parse")
                .is_some()
        );
    }

    #[test]
    fn empty_player_certificate_keys_disable_validation() {
        let response = ServiceKeyResponse {
            profile_property_keys: None,
            player_certificate_keys: None,
        };

        assert!(
            profile_key_validator(response)
                .expect("missing keys should be accepted")
                .is_none()
        );
    }

    #[test]
    fn malformed_player_certificate_key_rejects_snapshot() {
        let response = ServiceKeyResponse {
            profile_property_keys: None,
            player_certificate_keys: Some(vec![ServiceKeyData {
                public_key: "not base64".to_owned(),
            }]),
        };

        assert!(profile_key_validator(response).is_err());
    }

    #[test]
    fn malformed_profile_property_key_rejects_snapshot() {
        let (_, public_key) = generate_key_pair().expect("test RSA key should generate");
        let der = public_key_to_bytes(&public_key).expect("test RSA key should encode");
        let response = ServiceKeyResponse {
            profile_property_keys: Some(vec![ServiceKeyData {
                public_key: "not base64".to_owned(),
            }]),
            player_certificate_keys: Some(vec![ServiceKeyData {
                public_key: BASE64_STANDARD.encode(der),
            }]),
        };

        assert!(profile_key_validator(response).is_err());
    }

    #[test]
    fn rejects_more_keys_than_one_snapshot_may_contain() {
        let response = ServiceKeyResponse {
            profile_property_keys: None,
            player_certificate_keys: Some(
                (0..=MAX_SERVICE_KEYS_PER_KIND)
                    .map(|_| ServiceKeyData {
                        public_key: String::new(),
                    })
                    .collect(),
            ),
        };

        assert!(matches!(
            profile_key_validator(response),
            Err(ServiceKeyError::TooManyKeys { .. })
        ));
    }

    #[test]
    fn rejects_an_oversized_encoded_key_before_decoding_it() {
        let response = ServiceKeyResponse {
            profile_property_keys: None,
            player_certificate_keys: Some(vec![ServiceKeyData {
                public_key: "A".repeat(MAX_SERVICE_KEY_ENCODED_BYTES + 1),
            }]),
        };

        assert!(matches!(
            profile_key_validator(response),
            Err(ServiceKeyError::EncodedKeyTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn absolute_deadline_stops_a_drip_fed_initial_refresh() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("client should connect");
            read_request(&mut socket).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n")
                .await
                .expect("headers should write");
            for _ in 0..100 {
                if socket.write_all(b" ").await.is_err() {
                    break;
                }
                sleep(Duration::from_millis(25)).await;
            }
        });
        let store = ServiceKeyStore::new(Some(&format!("http://{address}/publickeys")))
            .expect("loopback endpoint should configure");

        let result = store.refresh_with_timeout(Duration::from_millis(100)).await;

        assert!(matches!(
            result,
            Err(ServiceKeyError::RequestDeadlineExceeded { .. })
        ));
        server.await.expect("test server should stop cleanly");
    }

    #[tokio::test]
    async fn oversized_refresh_preserves_the_last_good_snapshot() {
        let (_, public_key) = generate_key_pair().expect("test RSA key should generate");
        let der = public_key_to_bytes(&public_key).expect("test RSA key should encode");
        let valid_body = serde_json::to_vec(&serde_json::json!({
            "profilePropertyKeys": [],
            "playerCertificateKeys": [{ "publicKey": BASE64_STANDARD.encode(der) }],
        }))
        .expect("test response should serialize");
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let server = tokio::spawn(async move {
            let (mut first, _) = listener
                .accept()
                .await
                .expect("first client should connect");
            read_request(&mut first).await;
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                valid_body.len()
            );
            first
                .write_all(headers.as_bytes())
                .await
                .expect("first headers should write");
            first
                .write_all(&valid_body)
                .await
                .expect("valid response should write");

            let (mut second, _) = listener
                .accept()
                .await
                .expect("second client should connect");
            read_request(&mut second).await;
            second
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                .await
                .expect("second headers should write");
            let oversized_body = vec![b' '; MAX_SERVICE_KEYS_RESPONSE_BYTES + 1];
            let _ = second.write_all(&oversized_body).await;
        });
        let store = ServiceKeyStore::new(Some(&format!("http://{address}/publickeys")))
            .expect("loopback endpoint should configure");
        store
            .refresh_with_timeout(Duration::from_secs(1))
            .await
            .expect("first snapshot should load");
        assert!(store.profile_key_validator().is_some());

        let result = store.refresh_with_timeout(Duration::from_secs(1)).await;

        assert!(
            matches!(result, Err(ServiceKeyError::ResponseTooLarge { .. })),
            "expected an oversized response error, got {result:?}"
        );
        assert!(store.profile_key_validator().is_some());
        server.await.expect("test server should stop cleanly");
    }

    #[test]
    fn initial_failures_use_authlib_backoff_cap() {
        assert_eq!(failure_delay(0), BASE_FAILURE_INTERVAL);
        assert_eq!(failure_delay(1), BASE_FAILURE_INTERVAL * 2);
        assert_eq!(
            failure_delay(MAX_BACKOFF_EXPONENT + 1),
            BASE_FAILURE_INTERVAL * (1 << MAX_BACKOFF_EXPONENT)
        );
    }
}
