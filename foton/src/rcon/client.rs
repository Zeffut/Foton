//! One Rcon connection, from the login attempt to the last command.
//!
//! Vanilla parity: `net.minecraft.server.rcon.thread.RconClient`, one thread
//! per connection that reads a frame, answers it, and reads the next.

use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use foton_core::server::Server;
use subtle::ConstantTimeEq as _;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
    select,
    time::{Instant, timeout, timeout_at},
};
use tokio_util::sync::CancellationToken;

use super::packet::{
    AUTH_FAILURE_REQUEST_ID, MAX_PACKET_SIZE, RconRequest, SERVERDATA_AUTH,
    SERVERDATA_AUTH_RESPONSE, SERVERDATA_EXECCOMMAND, SERVERDATA_RESPONSE_VALUE, decode_request,
    encode_response, split_response,
};

const RCON_AUTH_TIMEOUT: Duration = Duration::from_secs(30);
const RCON_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Serves one connection until it closes, misbehaves, or the server stops.
pub(super) async fn serve(
    mut connection: TcpStream,
    address: SocketAddr,
    connection_id: u64,
    password: Arc<str>,
    server: Arc<Server>,
    cancel: CancellationToken,
) {
    if !authenticate(
        &mut connection,
        address,
        &password,
        &cancel,
        RCON_AUTH_TIMEOUT,
    )
    .await
    {
        log::debug!("Rcon client {address} disconnected");
        return;
    }

    loop {
        let request = select! {
            () = cancel.cancelled() => break,
            request = read_request(&mut connection) => request,
        };
        let Some(request) = request else { break };

        let mut reject_and_close = false;
        let sent = match request.kind {
            SERVERDATA_AUTH => {
                // Vanilla parity: an empty password never authenticates, even
                // against an empty configured one -- though Foton refuses to
                // start with one, so that case cannot arise here.
                //
                // The comparison is constant-time. Vanilla uses `String.equals`,
                // which returns on the first differing byte and so leaks the
                // length of the correct prefix; RCON is admin tooling rather
                // than gameplay, so this is one of the places Foton is allowed
                // to be stricter than vanilla.
                let authenticated =
                    !request.body.is_empty() && constant_time_eq(&request.body, &password);
                if authenticated {
                    log::info!("Rcon client {address} authenticated");
                    send(
                        &mut connection,
                        request.request_id,
                        SERVERDATA_AUTH_RESPONSE,
                        "",
                    )
                    .await
                } else {
                    log::warn!("Rcon client {address} failed to authenticate");
                    // Security hardening divergence: Vanilla keeps the socket
                    // open after replying with request id -1. Foton closes it
                    // so one connection cannot become an unlimited password
                    // oracle; RCON is administrative rather than gameplay.
                    reject_and_close = true;
                    send_auth_failure(&mut connection).await
                }
            }
            SERVERDATA_EXECCOMMAND => {
                let response = run_command(&server, connection_id, &request.body, &cancel).await;
                let Some(response) = response else { break };
                send_command_response(&mut connection, request.request_id, &response).await
            }
            unknown => {
                // Vanilla parity: `String.format("Unknown request %s",
                // Integer.toHexString(cmd))`, which prints the kind unsigned.
                let message = format!("Unknown request {:x}", unknown as u32);
                send_command_response(&mut connection, request.request_id, &message).await
            }
        };
        if sent.is_err() || reject_and_close {
            break;
        }
    }
    log::debug!("Rcon client {address} disconnected");
}

/// Reads and answers the first request under one absolute deadline.
///
/// A client receives the response appropriate to the request it actually
/// sent, but only a successful authentication keeps the socket open. This
/// prevents unauthenticated clients from occupying a connection slot by
/// periodically sending commands or unknown request kinds.
async fn authenticate(
    connection: &mut TcpStream,
    address: SocketAddr,
    password: &str,
    cancel: &CancellationToken,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    select! {
        () = cancel.cancelled() => false,
        result = timeout_at(deadline, authenticate_before_deadline(connection, address, password)) => {
            result.unwrap_or(false)
        }
    }
}

async fn authenticate_before_deadline(
    connection: &mut TcpStream,
    address: SocketAddr,
    password: &str,
) -> bool {
    let Some(request) = read_request(connection).await else {
        return false;
    };

    match request.kind {
        SERVERDATA_AUTH => {
            let authenticated =
                !request.body.is_empty() && constant_time_eq(&request.body, password);
            if authenticated {
                log::info!("Rcon client {address} authenticated");
                send(connection, request.request_id, SERVERDATA_AUTH_RESPONSE, "")
                    .await
                    .is_ok()
            } else {
                log::warn!("Rcon client {address} failed to authenticate");
                let _ = send_auth_failure(connection).await;
                false
            }
        }
        SERVERDATA_EXECCOMMAND => {
            let _ = send_auth_failure(connection).await;
            false
        }
        unknown => {
            let message = format!("Unknown request {:x}", unknown as u32);
            let _ = send_command_response(connection, request.request_id, &message).await;
            false
        }
    }
}

/// Reads one frame, or `None` once the connection ends or breaks its own rules.
async fn read_request(connection: &mut TcpStream) -> Option<RconRequest> {
    let mut length = [0_u8; 4];
    connection.read_exact(&mut length).await.ok()?;
    let length = i32::from_le_bytes(length);

    // Vanilla reads one 1460-byte chunk and hangs up unless the frame fills it
    // exactly, which makes a client that coalesces two frames into one segment
    // look malformed. Reading the declared length instead accepts that client;
    // a well-formed frame is byte-for-byte the same either way. The upper
    // bound is still vanilla's, so an absurd length is refused rather than
    // allocated.
    let length = usize::try_from(length).ok()?;
    if !(8..=MAX_PACKET_SIZE - 4).contains(&length) {
        return None;
    }

    let mut contents = vec![0_u8; length];
    connection.read_exact(&mut contents).await.ok()?;
    decode_request(&contents)
}

/// Runs one command and waits for everything it prints.
///
/// `None` means the server is shutting down and the connection should go with
/// it. A command that never answers cannot happen: the output sink replies
/// when its last handle is dropped, and every way an execution can end drops
/// it.
async fn run_command(
    server: &Arc<Server>,
    connection_id: u64,
    command: &str,
    cancel: &CancellationToken,
) -> Option<String> {
    let Ok(reply) = server.submit_rcon_command(connection_id, command.to_owned()) else {
        // Vanilla parity: the shape of `RconClient.run`'s catch, which hands
        // the client the failure as ordinary command output.
        return Some(format!(
            "Error executing: {command} (the command queue is full)"
        ));
    };
    select! {
        () = cancel.cancelled() => None,
        response = reply => Some(response.unwrap_or_default()),
    }
}

async fn send_command_response(
    connection: &mut TcpStream,
    request_id: i32,
    response: &str,
) -> Result<(), io::Error> {
    for chunk in split_response(response) {
        send(connection, request_id, SERVERDATA_RESPONSE_VALUE, chunk).await?;
    }
    Ok(())
}

async fn send_auth_failure(connection: &mut TcpStream) -> Result<(), io::Error> {
    send(
        connection,
        AUTH_FAILURE_REQUEST_ID,
        SERVERDATA_AUTH_RESPONSE,
        "",
    )
    .await
}

async fn send(
    connection: &mut TcpStream,
    request_id: i32,
    kind: i32,
    payload: &str,
) -> Result<(), io::Error> {
    timeout(
        RCON_WRITE_TIMEOUT,
        connection.write_all(&encode_response(request_id, kind, payload)),
    )
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Rcon write timed out"))?
}

/// Compares two secrets without leaking where they first differ.
///
/// Lengths are compared first and separately, which leaks only the length --
/// as any timing-safe primitive of this shape does. Equal-length contents use
/// `subtle`'s optimization-resistant primitive rather than a handwritten fold.
fn constant_time_eq(candidate: &str, expected: &str) -> bool {
    let candidate = candidate.as_bytes();
    let expected = expected.as_bytes();
    if candidate.len() != expected.len() {
        return false;
    }
    bool::from(candidate.ct_eq(expected))
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, time::Duration};

    use tokio::{
        io::{AsyncReadExt as _, AsyncWriteExt as _},
        net::{TcpListener, TcpStream},
        time::{sleep, timeout},
    };
    use tokio_util::sync::CancellationToken;

    use crate::rcon::packet::{
        AUTH_FAILURE_REQUEST_ID, SERVERDATA_AUTH, SERVERDATA_AUTH_RESPONSE, SERVERDATA_EXECCOMMAND,
        SERVERDATA_RESPONSE_VALUE, encode_response,
    };

    use super::{authenticate, constant_time_eq};

    async fn connected_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should have an address");
        let client = TcpStream::connect(address);
        let (accepted, client) = tokio::join!(listener.accept(), client);
        (
            accepted.expect("test connection should be accepted").0,
            client.expect("test client should connect"),
        )
    }

    #[tokio::test]
    async fn unauthenticated_trickle_cannot_extend_the_absolute_deadline() {
        let (mut server_side, mut client_side) = connected_pair().await;
        let address = server_side
            .peer_addr()
            .expect("test connection should have a peer address");
        let authentication = tokio::spawn(async move {
            authenticate(
                &mut server_side,
                address,
                "secret",
                &CancellationToken::new(),
                Duration::from_millis(80),
            )
            .await
        });

        let frame = encode_response(7, SERVERDATA_AUTH, "secret");
        let trickle = tokio::spawn(async move {
            for byte in frame {
                if client_side.write_all(&[byte]).await.is_err() {
                    break;
                }
                sleep(Duration::from_millis(20)).await;
            }
        });

        let authenticated = timeout(Duration::from_millis(200), authentication)
            .await
            .expect("absolute authentication deadline should expire")
            .expect("authentication task should not panic");
        assert!(!authenticated);
        trickle.abort();
    }

    #[tokio::test]
    async fn first_unauthenticated_request_is_answered_then_closed() {
        for (kind, body, expected) in [
            (
                SERVERDATA_AUTH,
                "wrong",
                encode_response(AUTH_FAILURE_REQUEST_ID, SERVERDATA_AUTH_RESPONSE, ""),
            ),
            (
                SERVERDATA_EXECCOMMAND,
                "payload",
                encode_response(AUTH_FAILURE_REQUEST_ID, SERVERDATA_AUTH_RESPONSE, ""),
            ),
            (
                0x123,
                "payload",
                encode_response(7, SERVERDATA_RESPONSE_VALUE, "Unknown request 123"),
            ),
        ] {
            let (mut server_side, mut client_side) = connected_pair().await;
            let address = server_side
                .peer_addr()
                .expect("test connection should have a peer address");
            let authentication = tokio::spawn(async move {
                authenticate(
                    &mut server_side,
                    address,
                    "secret",
                    &CancellationToken::new(),
                    Duration::from_secs(1),
                )
                .await
            });

            client_side
                .write_all(&encode_response(7, kind, body))
                .await
                .expect("test request should be written");
            let mut response = Vec::new();
            client_side
                .read_to_end(&mut response)
                .await
                .expect("server should close after its response");

            assert_eq!(response, expected);
            assert!(
                !authentication
                    .await
                    .expect("authentication task should not panic")
            );
        }
    }

    #[tokio::test]
    async fn valid_first_authentication_keeps_the_session_eligible() {
        let (mut server_side, mut client_side) = connected_pair().await;
        let address = server_side
            .peer_addr()
            .expect("test connection should have a peer address");
        let authentication = tokio::spawn(async move {
            authenticate(
                &mut server_side,
                address,
                "secret",
                &CancellationToken::new(),
                Duration::from_secs(1),
            )
            .await
        });

        client_side
            .write_all(&encode_response(7, SERVERDATA_AUTH, "secret"))
            .await
            .expect("test authentication should be written");
        let mut response = Vec::new();
        client_side
            .read_to_end(&mut response)
            .await
            .expect("test authentication response should be readable");

        assert_eq!(response, encode_response(7, SERVERDATA_AUTH_RESPONSE, ""));
        assert!(
            authentication
                .await
                .expect("authentication task should not panic")
        );
    }

    /// The comparison still answers correctly; being timing-safe is not an
    /// excuse for being wrong.
    #[test]
    fn constant_time_eq_matches_ordinary_equality() {
        for (candidate, expected) in [
            ("hunter2", "hunter2"),
            ("hunter2", "hunter3"),
            ("hunter2", "hunter22"),
            ("hunter2", "hunter"),
            ("", ""),
            ("", "x"),
            ("x", ""),
            // Multi-byte characters must not be treated as single units.
            ("clé", "clé"),
            ("clé", "cle"),
        ] {
            assert_eq!(
                constant_time_eq(candidate, expected),
                candidate == expected,
                "constant_time_eq({candidate:?}, {expected:?}) disagreed with =="
            );
        }
    }
}
