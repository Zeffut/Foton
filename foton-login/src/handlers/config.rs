//! Configuration state packet handlers.

use std::mem;
use std::sync::Arc;

use foton_core::entity::next_entity_id;
use foton_core::event::PlayerLoginEvent;
use foton_core::player::PlayerConnection;
use foton_core::player::connection::JavaConnection;
use foton_core::player::{ClientInformation, Player};
use foton_protocol::packets::common::CCustomPayload;
use foton_protocol::packets::common::{
    ResourcePackAction, SClientInformation, SCustomPayload, SResourcePack,
};
use foton_protocol::packets::config::CFinishConfiguration;
use foton_protocol::packets::config::CSelectKnownPacks;
use foton_protocol::packets::config::SSelectKnownPacks;
use foton_protocol::packets::shared_implementation::KnownPack;
use foton_protocol::utils::ConnectionProtocol;
use foton_utils::{Identifier, translations};

use crate::tcp_client::{ConnectionAction, ConnectionUpdate, JavaTcpClient};
use foton_core::event::AsyncPlayerPreLoginEvent;

/// The `minecraft:brand` payload: a protocol string, its `VarInt` length then
/// the UTF-8 bytes. Vanilla parity: `BrandPayload` writes it with `writeUtf`,
/// and a client reading a bare name fails the whole connection.
const BRAND_PAYLOAD: [u8; 6] = *b"\x05Foton";

/// How many answers about packs a client in configuration is remembered for.
///
/// A client sends a handful per pack; the cap only stops one that never
/// stops from growing the list.
const MAX_RESOURCE_PACK_ANSWERS: usize = 32;

impl JavaTcpClient {
    /// Handles a custom payload packet during the configuration state.
    #[expect(clippy::unused_self, reason = "this is an api function")]
    pub fn handle_config_custom_payload(&self, packet: SCustomPayload) {
        log::debug!("Custom payload packet: {packet:?}");
    }

    /// Handles the client information packet during the configuration state.
    pub async fn handle_client_information(&self, packet: SClientInformation) {
        log::debug!("Client information packet: {packet:?}");

        // Convert packet to our ClientInformation struct and store it
        let info = ClientInformation {
            language: packet.language,
            view_distance: packet
                .view_distance
                .clamp(2, i32::from(self.server.config.view_distance).max(2))
                as u8,
            chat_visibility: packet.chat_visibility,
            chat_colors: packet.chat_colors,
            model_customization: packet.model_customization,
            main_hand: packet.main_hand,
            text_filtering_enabled: packet.text_filtering_enabled,
            allows_listing: packet.allows_listing,
            particle_status: packet.particle_status,
        };

        *self.client_information.lock().await = info;
    }

    /// Starts the configuration process by sending initial packets.
    pub async fn start_configuration(&self) {
        self.send_bare_packet_now(CCustomPayload::new(
            Identifier::vanilla_static("brand"),
            Box::new(BRAND_PAYLOAD),
        ))
        .await;

        // Send server links if enabled and configured
        if let Some(server_links) = self.server.config.server_links_packet() {
            self.send_bare_packet_now(server_links).await;
        }

        self.send_bare_packet_now(CSelectKnownPacks::new(vec![KnownPack::new(
            "minecraft".to_string(),
            "core".to_string(),
            foton_utils::MC_VERSION.to_string(),
        )]))
        .await;
    }

    /// Handles the select known packs packet during the configuration state.
    pub async fn handle_select_known_packs(&self, packet: SSelectKnownPacks) {
        let resource_pack = self.server.config.resource_pack.as_ref();
        let sequence_result = self
            .pre_play_state
            .lock()
            .select_known_packs(resource_pack.is_some());
        if let Err(error) = sequence_result {
            self.reject_unexpected_packet(error).await;
            return;
        }
        log::debug!("Select known packs packet: {packet:?}");

        let registry_cache = self.server.registry_cache.registry_packets.clone();
        for encoded_packet in registry_cache.iter() {
            self.send_packet_now(encoded_packet).await;
        }

        // Send the packet for tags
        self.send_packet_now(&self.server.registry_cache.tags_packet)
            .await;

        // Vanilla parity: `ServerResourcePackConfigurationTask` runs between
        // registry synchronization and the end of configuration, and the client's
        // verdict on the pack is what lets configuration go on.
        match resource_pack {
            Some(pack) => self.send_bare_packet_now(pack.push_packet()).await,
            None => self.send_bare_packet_now(CFinishConfiguration {}).await,
        }
    }

    /// Handles the client's answer to a resource pack during configuration.
    ///
    /// Vanilla parity: `ServerConfigurationPacketListenerImpl.handleResourcePackResponse`.
    /// Progress reports are fine at any time; an answer that ends the exchange
    /// finishes the pack task, so it is only valid while one is running.
    pub(crate) async fn handle_resource_pack_response(
        &self,
        packet: SResourcePack,
    ) -> ConnectionAction {
        if packet.action == ResourcePackAction::Declined
            && self
                .server
                .config
                .resource_pack
                .as_ref()
                .is_some_and(|pack| pack.required)
        {
            log::info!(
                "Disconnecting client {} due to resource pack {} rejection",
                self.id,
                packet.id
            );
            self.kick(
                translations::MULTIPLAYER_REQUIRED_TEXTURE_PROMPT_DISCONNECT
                    .msg()
                    .into(),
            )
            .await;
            return ConnectionAction::none();
        }

        {
            let mut answers = self.resource_pack_answers.lock();
            if answers.len() < MAX_RESOURCE_PACK_ANSWERS {
                answers.push((packet.id, packet.action));
            }
        }
        if !packet.action.is_terminal() {
            return ConnectionAction::none();
        }
        let sequence_result = self.pre_play_state.lock().finish_resource_pack();
        if let Err(error) = sequence_result {
            return self.reject_unexpected_packet(error).await;
        }
        self.send_bare_packet_now(CFinishConfiguration {}).await;
        ConnectionAction::none()
    }

    /// Finishes the configuration process and transitions to the play state.
    pub(crate) async fn finish_configuration(&self) -> ConnectionAction {
        let sequence_result = self.pre_play_state.lock().finish_configuration();
        let gameprofile = match sequence_result {
            Ok(gameprofile) => gameprofile,
            Err(error) => return self.reject_unexpected_packet(error).await,
        };
        let mut pre_login =
            AsyncPlayerPreLoginEvent::new(gameprofile.id, gameprofile.name.clone(), self.address);
        self.server.events().fire(&mut pre_login);
        if let Some(message) = pre_login.kick_message() {
            self.kick(message.to_owned().into()).await;
            return ConnectionAction::none();
        }
        self.protocol.store(ConnectionProtocol::Play);

        let client_info = self.client_information.lock().await.clone();
        let resource_pack_answers = mem::take(&mut *self.resource_pack_answers.lock());

        let world = self.server.overworld().clone();
        let entity_id = next_entity_id();

        let player = Arc::new_cyclic(|player_weak| {
            let java_connection = JavaConnection::new(
                self.outgoing_queue.clone(),
                self.cancel_token.clone(),
                self.compression.load(),
                self.network_writer.clone(),
                self.id,
                self.address,
                player_weak.clone(),
                self.translation.clone(),
                Some(self.translated_serverbound.clone()),
            );
            let connection = Arc::new(PlayerConnection::Java(java_connection));

            Player::new(
                gameprofile,
                connection,
                world,
                Arc::downgrade(&self.server),
                self.server.config.clone(),
                entity_id,
                client_info,
                player_weak,
            )
        });

        player.adopt_configuration_resource_pack(resource_pack_answers);

        if let Some(tap) = self.server.packet_taps.current() {
            tap.playing(self.id, player.gameprofile.id, entity_id);
        }

        let connection = Arc::clone(&player.connection);
        if self
            .connection_updates
            .send(ConnectionUpdate::Upgrade(Arc::clone(&connection)))
            .is_err()
        {
            self.kick("Failed to update connection state".into()).await;
            return ConnectionAction::none();
        }

        tokio::select! {
            () = self.connection_updated.notified() => {}
            () = self.cancel_token.cancelled() => return ConnectionAction::none(),
        }

        let mut login = PlayerLoginEvent::new(Arc::clone(&player));
        self.server.events().fire(&mut login);
        if let Some(message) = login.kick_message() {
            self.server.abort_player_login(&player);
            self.kick(message.clone()).await;
            return ConnectionAction::none();
        }
        self.server.queue_player_join(player);

        ConnectionAction::upgrade(connection)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_utils::codec::VarInt;
    use foton_utils::serial::PrefixedRead as _;

    use super::BRAND_PAYLOAD;

    #[test]
    fn brand_payload_is_a_whole_protocol_string() {
        let mut data = Cursor::new(&BRAND_PAYLOAD[..]);
        let brand = String::read_prefixed::<VarInt>(&mut data).expect("brand string");
        assert_eq!(brand, "Foton");
        assert_eq!(data.position() as usize, BRAND_PAYLOAD.len());
    }
}
