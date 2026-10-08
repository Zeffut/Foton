//! A player's resource packs: what the client last said about them, and what
//! becomes of a decline.
//!
//! The server's own pack is sent during configuration, before a `Player`
//! exists; `foton-login` hands that exchange over once it has built one.

use std::mem;

use foton_protocol::packets::common::{ResourcePackAction, SResourcePack};
use foton_utils::translations;
use uuid::Uuid;

use crate::event::PlayerResourcePackStatusEvent;

use super::Player;

/// What the client has said about resource packs.
#[derive(Debug, Default)]
pub(super) struct ResourcePackState {
    /// The last answer, which is what Bukkit's `getResourcePackStatus` reports.
    status: Option<ResourcePackAction>,
    /// Answers given during configuration, waiting for a player to name.
    configuration_answers: Vec<(Uuid, ResourcePackAction)>,
}

impl Player {
    /// The last thing the client reported about a pack, if it reported anything.
    #[must_use]
    pub fn resource_pack_status(&self) -> Option<ResourcePackAction> {
        self.resource_packs.lock().status
    }

    /// Takes in what the client said about packs while it was configuring.
    ///
    /// It counts as the player's status straight away; listeners hear it once
    /// the player has joined, see [`Self::announce_configuration_resource_pack`].
    pub fn adopt_configuration_resource_pack(&self, answers: Vec<(Uuid, ResourcePackAction)>) {
        let mut state = self.resource_packs.lock();
        if let Some((_, action)) = answers.last() {
            state.status = Some(*action);
        }
        state.configuration_answers = answers;
    }

    /// Tells listeners what the client said about packs during configuration.
    pub fn announce_configuration_resource_pack(&self) {
        let answers = mem::take(&mut self.resource_packs.lock().configuration_answers);
        let Some(player) = self.shared() else {
            return;
        };
        for (pack, action) in answers {
            self.fire_event(&mut PlayerResourcePackStatusEvent::new(
                player.clone(),
                pack,
                action,
            ));
        }
    }

    /// Handles the client's answer to a pack push once it is in the world.
    ///
    /// Vanilla parity: `ServerCommonPacketListenerImpl.handleResourcePackResponse`.
    /// Only a decline of the server's own required pack costs the player their
    /// place: a pack a plugin pushed is the plugin's business.
    pub fn handle_resource_pack(&self, packet: SResourcePack) {
        if packet.action == ResourcePackAction::Declined
            && self
                .config
                .resource_pack
                .as_ref()
                .is_some_and(|pack| pack.required)
        {
            log::info!(
                "Disconnecting {} due to resource pack {} rejection",
                self.gameprofile.name,
                packet.id
            );
            self.disconnect(translations::MULTIPLAYER_REQUIRED_TEXTURE_PROMPT_DISCONNECT.msg());
        }

        self.resource_packs.lock().status = Some(packet.action);
        if let Some(player) = self.shared() {
            self.fire_event(&mut PlayerResourcePackStatusEvent::new(
                player,
                packet.id,
                packet.action,
            ));
        }
    }
}
