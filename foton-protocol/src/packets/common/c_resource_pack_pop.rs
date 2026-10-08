use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::config::C_RESOURCE_PACK_POP;
use foton_registry::packets::play::C_RESOURCE_PACK_POP as PLAY_C_RESOURCE_PACK_POP;
use uuid::Uuid;

/// Asks the client to drop a pack the server pushed, or all of them.
///
/// Vanilla parity: `ClientboundResourcePackPopPacket`; an absent id pops every
/// server pack.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Config = C_RESOURCE_PACK_POP, Play = PLAY_C_RESOURCE_PACK_POP)]
pub struct CResourcePackPop {
    /// The pack to remove; `None` removes them all.
    pub id: Option<Uuid>,
}
