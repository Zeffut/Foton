//! Status state packet handlers (server list ping).

use foton_core::config::RuntimeConfig;
use foton_core::event::ServerListPingEvent;
use text_components::TextComponent;
use foton_protocol::packets::{
    common::{CPongResponse, SPingRequest},
    status::{CStatusResponse, Players, Sample, Status, Version},
};
use foton_registry::packets::CURRENT_MC_PROTOCOL;
use foton_utils::MC_VERSION;

use crate::tcp_client::JavaTcpClient;

impl JavaTcpClient {
    /// Handles a status request from the client.
    ///
    /// Bukkit parity: `ServerListPingEvent` decides the MOTD and the player
    /// limit shown.
    pub async fn handle_status_request(&self) {
        let online = i32::try_from(self.server.player_count()).unwrap_or(i32::MAX);
        let mut ping = ServerListPingEvent::new(
            self.address.ip(),
            TextComponent::from(self.server.config.motd.clone()),
            online,
            self.server.config.max_players.cast_signed(),
        );
        self.server.events().fire(&mut ping);
        let res_packet = CStatusResponse::new(Status {
            description: ping.motd().clone(),
            players: Some(Players {
                max: ping.max_players(),
                online,
                sample: self
                    .server
                    .player_sample()
                    .into_iter()
                    .map(|(name, id)| Sample { name, id })
                    .collect(),
            }),
            enforces_secure_chat: self.server.enforces_secure_chat(),
            favicon: load_favicon(&self.server.config),
            version: Some(Version {
                name: MC_VERSION,
                protocol: CURRENT_MC_PROTOCOL,
            }),
        });
        self.send_bare_packet_now(res_packet).await;
    }

    /// Handles a ping request from the client.
    pub async fn handle_ping_request(&self, packet: SPingRequest) {
        self.send_bare_packet_now(CPongResponse::new(packet.time))
            .await;
        self.close();
    }
}

/// Loads the favicon from config.
fn load_favicon(config: &RuntimeConfig) -> Option<String> {
    use base64::{Engine, prelude::BASE64_STANDARD};
    use std::fs;
    use std::path::Path;

    const ICON_PREFIX: &str = "data:image/png;base64,";

    if !config.use_favicon {
        return None;
    }

    let path = Path::new(&config.favicon);
    let Ok(icon) = fs::read(path) else {
        return None;
    };

    let cap = ICON_PREFIX.len() + icon.len().div_ceil(3) * 4;
    let mut base64 = String::with_capacity(cap);
    base64 += ICON_PREFIX;
    BASE64_STANDARD.encode_string(icon, &mut base64);
    Some(base64)
}
