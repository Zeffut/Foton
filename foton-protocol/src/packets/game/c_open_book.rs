//! Clientbound packet that opens the written-book reader.

use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_OPEN_BOOK;
use foton_utils::types::InteractionHand;

/// Tells the client to open the book held in `hand`.
///
/// Vanilla parity: `ClientboundOpenBookPacket`.
#[derive(ClientPacket, WriteTo, Clone, Copy, Debug)]
#[packet_id(Play = C_OPEN_BOOK)]
pub struct COpenBook {
    /// The hand holding the written book.
    pub hand: InteractionHand,
}
