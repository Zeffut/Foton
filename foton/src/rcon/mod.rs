//! Source Rcon server.
//!
//! Vanilla parity: `net.minecraft.server.rcon.thread.RconThread`, which owns
//! the listening socket and gives every connection its own worker. Foton runs
//! those as Tokio tasks on the shared tracker rather than as threads, because
//! a connection spends nearly all of its life waiting.

mod client;
mod packet;

use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use foton_core::server::Server;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::{OwnedSemaphorePermit, Semaphore},
    time::sleep,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(50);

struct AcceptedConnection {
    connection: TcpStream,
    address: SocketAddr,
    permit: OwnedSemaphorePermit,
}

/// A bound Rcon port waiting to be served.
pub struct RconListener {
    listener: TcpListener,
    password: Arc<str>,
    connection_permits: Arc<Semaphore>,
    next_connection: u64,
}

impl RconListener {
    /// Binds the Rcon port.
    ///
    /// Binding here rather than inside the accept loop is deliberate: a port
    /// already in use is a startup failure the operator can see, not a warning
    /// scrolling past while remote administration silently never works.
    pub async fn bind(
        bind: IpAddr,
        port: u16,
        password: Arc<str>,
        max_connections: usize,
    ) -> Result<Self, io::Error> {
        let address = SocketAddr::new(bind, port);
        let listener = TcpListener::bind(address).await?;
        log::info!("Rcon running on {address}");
        Ok(Self {
            listener,
            password,
            connection_permits: Arc::new(Semaphore::new(max_connections)),
            next_connection: 0,
        })
    }

    async fn accept_connection(&self, cancel: &CancellationToken) -> Option<AcceptedConnection> {
        let permit = select! {
            () = cancel.cancelled() => return None,
            permit = Arc::clone(&self.connection_permits).acquire_owned() => permit.ok()?,
        };
        let (connection, address) = select! {
            () = cancel.cancelled() => return None,
            accepted = self.listener.accept() => {
                accept_or_backoff(accepted, ACCEPT_ERROR_BACKOFF).await?
            }
        };
        Some(AcceptedConnection {
            connection,
            address,
            permit,
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
            let Some(accepted) = self.accept_connection(&cancel).await else {
                if cancel.is_cancelled() {
                    break;
                }
                continue;
            };
            let AcceptedConnection {
                connection,
                address,
                permit,
            } = accepted;
            log::info!("Accepted Rcon connection from {address}");

            let connection_id = self.next_connection;
            self.next_connection = self.next_connection.wrapping_add(1);
            task_tracker.spawn(client::serve(
                connection,
                address,
                connection_id,
                permit,
                Arc::clone(&self.password),
                Arc::clone(&server),
                cancel.child_token(),
            ));
        }
    }
}

async fn accept_or_backoff<T>(accepted: io::Result<T>, duration: Duration) -> Option<T> {
    match accepted {
        Ok(accepted) => Some(accepted),
        Err(error) => {
            log::warn!(
                "Failed to accept Rcon connection: {error}; retrying after {} ms",
                duration.as_millis()
            );
            sleep(duration).await;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        net::{IpAddr, Ipv4Addr},
        sync::Arc,
        time::Duration,
    };

    use tokio::{net::TcpStream, time::timeout};
    use tokio_util::sync::CancellationToken;

    use super::{RconListener, accept_or_backoff};

    #[tokio::test]
    async fn listener_binds_only_the_configured_loopback_address() {
        let listener = RconListener::bind(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
            Arc::from("test-password"),
            8,
        )
        .await
        .expect("loopback listener should bind");

        let address = listener
            .listener
            .local_addr()
            .expect("bound listener should have a local address");
        assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[tokio::test]
    async fn listener_does_not_accept_more_than_the_connection_limit() {
        let listener = RconListener::bind(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
            Arc::from("test-password"),
            1,
        )
        .await
        .expect("loopback listener should bind");
        let address = listener
            .listener
            .local_addr()
            .expect("bound listener should have a local address");
        let cancel = CancellationToken::new();

        let first_client = TcpStream::connect(address);
        let (first_connection, connected) =
            tokio::join!(listener.accept_connection(&cancel), first_client);
        connected.expect("first client should connect");
        let first_connection = first_connection.expect("first connection should be accepted");

        let second_client = TcpStream::connect(address)
            .await
            .expect("second client should reach the TCP backlog");
        assert!(
            timeout(
                Duration::from_millis(50),
                listener.accept_connection(&cancel)
            )
            .await
            .is_err(),
            "a second connection must wait for the first connection's permit"
        );

        drop(first_connection);
        let second_connection =
            timeout(Duration::from_secs(1), listener.accept_connection(&cancel))
                .await
                .expect("accept should resume after a permit is released")
                .expect("second connection should be accepted");
        drop(second_connection);
        drop(second_client);
    }

    #[tokio::test]
    async fn accept_failures_wait_before_retrying() {
        let retry = accept_or_backoff::<()>(
            Err(io::Error::other("synthetic accept failure")),
            Duration::from_millis(50),
        );

        assert!(timeout(Duration::from_millis(10), retry).await.is_err());
    }
}
