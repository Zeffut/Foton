//! Clientbound item cooldown packet.

use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_COOLDOWN;
use foton_utils::Identifier;

/// Starts or clears a client-side item cooldown group.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_COOLDOWN)]
pub struct CCooldown {
    pub cooldown_group: Identifier,
    #[write(as = VarInt)]
    pub duration: i32,
}
