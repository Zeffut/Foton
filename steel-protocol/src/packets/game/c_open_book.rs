//! Clientbound packet that opens the written-book reader.

use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::C_OPEN_BOOK;
use steel_utils::types::InteractionHand;

/// Tells the client to open the book held in `hand`.
///
/// Vanilla parity: `ClientboundOpenBookPacket`.
#[derive(ClientPacket, WriteTo, Clone, Copy, Debug)]
#[packet_id(Play = C_OPEN_BOOK)]
pub struct COpenBook {
    /// The hand holding the written book.
    pub hand: InteractionHand,
}
