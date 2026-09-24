//! Status state packet handlers (server list ping).

use foton_core::config::RuntimeConfig;
use foton_protocol::packets::{
    common::{CPongResponse, SPingRequest},
    status::{CStatusResponse, Players, Sample, Status, Version},
};
use foton_registry::packets::CURRENT_MC_PROTOCOL;
use foton_utils::MC_VERSION;

use crate::tcp_client::JavaTcpClient;

impl JavaTcpClient {
    /// Handles a status request from the client.
    pub async fn handle_status_request(&self) {
        let res_packet = CStatusResponse::new(Status {
            description: self.server.config.motd.clone(),
            players: Some(Players {
                max: self.server.config.max_players.cast_signed(),
                online: self.server.player_count() as i32,
                sample: self
                    .server
                    .player_sample()
                    .into_iter()
                    .map(|(name, id)| Sample { name, id })
                    .collect(),
            }),
            enforces_secure_chat: self.server.enforces_secure_chat(),
            favicon: cached_favicon(&self.server.config),
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

fn cached_favicon(config: &RuntimeConfig) -> Option<String> {
    config.use_favicon.then(|| config.favicon.clone())
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs::{remove_file, write};

    use foton_core::config::RuntimeConfig;

    use super::cached_favicon;

    fn runtime_config(favicon: String) -> RuntimeConfig {
        RuntimeConfig {
            max_players: 20,
            view_distance: 10,
            simulation_distance: 10,
            max_chained_neighbor_updates: 1_000_000,
            online_mode: true,
            whitelist_enabled: false,
            auth_server: None,
            allow_insecure_auth_server: false,
            profile_server: None,
            services_server: None,
            encryption: true,
            allow_flight: false,
            motd: String::new(),
            use_favicon: true,
            favicon,
            enforce_secure_chat: false,
            chat_spam_threshold_seconds: 10,
            command_spam_threshold_seconds: 10,
            compression: None,
            server_links: None,
            packet_workers: None,
            chunk_generation_threads: None,
            chunk_encoding_threads: None,
            bug_report_webhook: None,
        }
    }

    #[test]
    fn deleting_the_source_after_startup_does_not_break_status_favicon() {
        let path = temp_dir().join(format!("foton-status-favicon-{}.png", uuid::Uuid::new_v4()));
        write(
            &path,
            include_bytes!("../../../package-content/favicon.png"),
        )
        .expect("write favicon fixture");
        let encoded = RuntimeConfig::load_favicon(&path).expect("load favicon during startup");
        remove_file(&path).expect("delete source after startup");
        let config = runtime_config(encoded.clone());

        assert_eq!(cached_favicon(&config), Some(encoded));
    }
}
