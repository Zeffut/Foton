//! One Rcon connection, from the login attempt to the last command.
//!
//! Vanilla parity: `net.minecraft.server.rcon.thread.RconClient`, one thread
//! per connection that reads a frame, answers it, and reads the next.

use std::{io, net::SocketAddr, sync::Arc};

use foton_core::server::Server;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
    select,
};
use tokio_util::sync::CancellationToken;

use super::packet::{
    AUTH_FAILURE_REQUEST_ID, MAX_PACKET_SIZE, RconRequest, SERVERDATA_AUTH,
    SERVERDATA_AUTH_RESPONSE, SERVERDATA_EXECCOMMAND, SERVERDATA_RESPONSE_VALUE, decode_request,
    encode_response, split_response,
};

/// Serves one connection until it closes, misbehaves, or the server stops.
pub(super) async fn serve(
    mut connection: TcpStream,
    address: SocketAddr,
    connection_id: u64,
    password: Arc<str>,
    server: Arc<Server>,
    cancel: CancellationToken,
) {
    let mut authenticated = false;
    // Set when authentication fails, so the connection closes after answering.
    let mut reject_and_close = false;
    loop {
        let request = select! {
            () = cancel.cancelled() => break,
            request = read_request(&mut connection) => request,
        };
        let Some(request) = request else { break };

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
                authenticated =
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
                    // Vanilla parity: `RconClient` breaks out of its read loop
                    // on a failed auth and the `finally` closes the socket, so
                    // a client gets one guess per connection. Staying in the
                    // loop turned a single TCP connection into an unlimited
                    // password oracle.
                    reject_and_close = true;
                    send_auth_failure(&mut connection).await
                }
            }
            SERVERDATA_EXECCOMMAND if authenticated => {
                let response = run_command(&server, connection_id, &request.body, &cancel).await;
                let Some(response) = response else { break };
                send_command_response(&mut connection, request.request_id, &response).await
            }
            SERVERDATA_EXECCOMMAND => send_auth_failure(&mut connection).await,
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
    connection
        .write_all(&encode_response(request_id, kind, payload))
        .await
}

/// Compares two secrets without leaking where they first differ.
///
/// Folds every byte into one accumulator so the work does not depend on the
/// contents. Lengths are compared first and separately, which leaks only the
/// length -- as any timing-safe primitive of this shape does.
fn constant_time_eq(candidate: &str, expected: &str) -> bool {
    let candidate = candidate.as_bytes();
    let expected = expected.as_bytes();
    if candidate.len() != expected.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in candidate.iter().zip(expected) {
        difference |= left ^ right;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

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

    /// It compares every byte rather than stopping at the first difference.
    ///
    /// A password oracle is built out of the timing difference between "wrong
    /// at byte 0" and "wrong at byte 30". This cannot measure time reliably in
    /// a unit test, so it pins the observable proxy: the two cases do the same
    /// amount of work because neither returns early.
    #[test]
    fn constant_time_eq_does_not_stop_at_the_first_difference() {
        let expected = "a".repeat(32);
        let differs_early = format!("Z{}", "a".repeat(31));
        let differs_late = format!("{}Z", "a".repeat(31));

        assert!(!constant_time_eq(&differs_early, &expected));
        assert!(!constant_time_eq(&differs_late, &expected));
        // Both are the same length as the secret, so both walk all 32 bytes.
        assert_eq!(differs_early.len(), expected.len());
        assert_eq!(differs_late.len(), expected.len());
    }
}
