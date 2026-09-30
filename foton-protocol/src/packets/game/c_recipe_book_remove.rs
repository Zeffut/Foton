//! Clientbound packet removing recipes from the recipe book.

use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_RECIPE_BOOK_REMOVE;
use foton_utils::codec::VarInt;

/// Takes recipe displays out of the client's recipe book.
///
/// Vanilla parity: `ClientboundRecipeBookRemovePacket`, a list of the display
/// ids the add packet handed out.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_RECIPE_BOOK_REMOVE)]
pub struct CRecipeBookRemove {
    pub recipes: Vec<VarInt>,
}
