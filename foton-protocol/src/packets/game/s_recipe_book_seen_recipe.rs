//! Serverbound packet marking a recipe book entry as seen.

use std::io::{Cursor, Result};

use foton_macros::ServerPacket;
use foton_utils::codec::VarInt;
use foton_utils::serial::ReadFrom;

/// Sent when the player hovers a recipe the book was highlighting as new.
///
/// Vanilla parity: `ServerboundRecipeBookSeenRecipePacket`, whose only field
/// is the display id the add packet handed out.
#[derive(ServerPacket, Clone, Copy, Debug)]
pub struct SRecipeBookSeenRecipe {
    pub recipe: i32,
}

impl ReadFrom for SRecipeBookSeenRecipe {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self {
            recipe: VarInt::read(data)?.0,
        })
    }
}
