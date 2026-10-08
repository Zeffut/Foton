use std::sync::Arc;

use foton_protocol::packets::common::ResourcePackAction;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use super::Event;
use crate::player::Player;

/// Emitted when a client reports what it did with a resource pack.
///
/// Each push produces a run of these: accepted, downloaded, then loaded or
/// failed. A pack answered during configuration is reported once the player
/// has joined, because there is no player to name before that.
pub struct PlayerResourcePackStatusEvent {
    player: Arc<Player>,
    pack: Uuid,
    action: ResourcePackAction,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerResourcePackStatusEvent {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("foton:event/player_resource_pack_status");
}
impl Event for PlayerResourcePackStatusEvent {}
impl PlayerResourcePackStatusEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player: Arc<Player>, pack: Uuid, action: ResourcePackAction) -> Self {
        Self {
            player,
            pack,
            action,
        }
    }
    /// Who answered.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }
    /// Which pack the answer is about.
    #[must_use]
    pub const fn pack(&self) -> Uuid {
        self.pack
    }
    /// What the client did with it.
    #[must_use]
    pub const fn action(&self) -> ResourcePackAction {
        self.action
    }
}
