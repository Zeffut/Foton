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
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
};

use foton_core::server::Server;
use tokio::{net::TcpListener, select};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

/// A bound Rcon port waiting to be served.
pub struct RconListener {
    listener: TcpListener,
    password: Arc<str>,
    next_connection: u64,
}

impl RconListener {
    /// Binds the Rcon port.
    ///
    /// Binding here rather than inside the accept loop is deliberate: a port
    /// already in use is a startup failure the operator can see, not a warning
    /// scrolling past while remote administration silently never works.
    pub async fn bind(port: u16, password: Arc<str>) -> Result<Self, io::Error> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).await?;
        log::info!("Rcon running on port {port}");
        Ok(Self {
            listener,
            password,
            next_connection: 0,
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
            log::info!("Accepted Rcon connection from {address}");

            let connection_id = self.next_connection;
            self.next_connection = self.next_connection.wrapping_add(1);
            task_tracker.spawn(client::serve(
                connection,
                address,
                connection_id,
                Arc::clone(&self.password),
                Arc::clone(&server),
                cancel.child_token(),
            ));
        }
    }
}
