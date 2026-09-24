//! Source Rcon server.
//!
//! Vanilla parity: `net.minecraft.server.rcon.thread.RconThread`, which owns
//! the listening socket and gives every connection its own worker. Foton runs
//! those as Tokio tasks on the shared tracker rather than as threads, because
//! a connection spends nearly all of its life waiting.

mod client;
mod packet;

use std::{
    future::Future,
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use foton_core::server::Server;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::{OwnedSemaphorePermit, Semaphore},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

/// Rcon is administration traffic; a small fixed ceiling prevents an
/// unauthenticated socket farm from creating an unbounded number of tasks.
const MAX_RCON_CONNECTIONS: usize = 16;

/// A bound Rcon port waiting to be served.
pub struct RconListener {
    listener: TcpListener,
    password: Arc<str>,
    next_connection: u64,
    connection_slots: Arc<Semaphore>,
}

impl RconListener {
    /// Binds the Rcon port.
    ///
    /// Binding here rather than inside the accept loop is deliberate: a port
    /// already in use is a startup failure the operator can see, not a warning
    /// scrolling past while remote administration silently never works.
    pub async fn bind(
        bind_address: IpAddr,
        port: u16,
        password: Arc<str>,
    ) -> Result<Self, io::Error> {
        let address = SocketAddr::new(bind_address, port);
        let listener = TcpListener::bind(address).await?;
        log::info!("Rcon running on {address}; Source Rcon traffic is plaintext");
        Ok(Self {
            listener,
            password,
            next_connection: 0,
            connection_slots: Arc::new(Semaphore::new(MAX_RCON_CONNECTIONS)),
        })
    }

    /// Accepts connections until the server stops.
    pub async fn run(
        mut self,
        server: Arc<Server>,
        cancel: CancellationToken,
        task_tracker: TaskTracker,
    ) {
        loop {
            let accepted = select! {
                () = cancel.cancelled() => break,
                accepted = self.listener.accept() => accepted,
            };
            let Ok((connection, address)) = accepted else {
                continue;
            };
            let Ok(slot) = Arc::clone(&self.connection_slots).try_acquire_owned() else {
                log::warn!(
                    "Rejected Rcon connection from {address}: {MAX_RCON_CONNECTIONS} clients are already connected"
                );
                continue;
            };
            log::info!("Accepted Rcon connection from {address}");

            let connection_id = self.next_connection;
            self.next_connection = self.next_connection.wrapping_add(1);
            task_tracker.spawn(serve_with_slot(
                connection,
                address,
                connection_id,
                Arc::clone(&self.password),
                Arc::clone(&server),
                cancel.child_token(),
                slot,
            ));
        }
    }
}

async fn serve_with_slot(
    connection: TcpStream,
    address: SocketAddr,
    connection_id: u64,
    password: Arc<str>,
    server: Arc<Server>,
    cancel: CancellationToken,
    slot: OwnedSemaphorePermit,
) {
    retain_slot_until_complete(
        slot,
        client::serve(connection, address, connection_id, password, server, cancel),
    )
    .await;
}

async fn retain_slot_until_complete(
    _slot: OwnedSemaphorePermit,
    connection: impl Future<Output = ()>,
) {
    connection.await;
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, sync::Arc, time::Duration};

    use tokio::{
        io::{AsyncReadExt as _, AsyncWriteExt as _},
        net::{TcpListener, TcpStream},
        sync::Semaphore,
        task::yield_now,
        time::timeout,
    };

    use super::retain_slot_until_complete;

    #[tokio::test]
    async fn connection_slot_is_retained_until_the_network_task_finishes() {
        let slots = Arc::new(Semaphore::new(1));
        let slot = Arc::clone(&slots)
            .try_acquire_owned()
            .expect("the only test slot should be available");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should have an address");
        let client = TcpStream::connect(address);
        let (accepted, client) = tokio::join!(listener.accept(), client);
        let (mut server_side, _) = accepted.expect("test connection should be accepted");
        let mut client_side = client.expect("test client should connect");

        let connection = tokio::spawn(retain_slot_until_complete(slot, async move {
            let mut byte = [0_u8; 1];
            server_side
                .read_exact(&mut byte)
                .await
                .expect("network task should receive its final byte");
        }));
        yield_now().await;
        assert_eq!(slots.available_permits(), 0);

        client_side
            .write_all(&[1])
            .await
            .expect("test client should finish the network task");
        timeout(Duration::from_secs(1), connection)
            .await
            .expect("network task should finish")
            .expect("network task should not panic");
        assert_eq!(slots.available_permits(), 1);
    }
}
