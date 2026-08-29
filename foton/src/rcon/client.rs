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
                authenticated = !request.body.is_empty() && request.body == *password;
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
        if sent.is_err() {
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
