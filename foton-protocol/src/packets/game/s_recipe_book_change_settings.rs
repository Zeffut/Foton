//! Serverbound packet reporting a recipe book screen's settings.

use foton_macros::{ReadFrom, ServerPacket};

use super::RecipeBookType;

/// Sent when the player opens or closes a recipe book, or toggles its
/// craftable-only filter.
///
/// Vanilla parity: `ServerboundRecipeBookChangeSettingsPacket`: the book type
/// as an enum ordinal, then the two booleans.
#[derive(ReadFrom, ServerPacket, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SRecipeBookChangeSettings {
    pub book_type: RecipeBookType,
    pub is_open: bool,
    pub is_filtering: bool,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_utils::serial::ReadFrom as _;

    use super::*;

    #[test]
    fn reads_the_book_before_its_two_flags() {
        let buffer = [2u8, 1, 0];
        let packet = SRecipeBookChangeSettings::read(&mut Cursor::new(buffer.as_slice()))
            .expect("packet should parse");

        assert_eq!(
            packet,
            SRecipeBookChangeSettings {
                book_type: RecipeBookType::BlastFurnace,
                is_open: true,
                is_filtering: false,
            }
        );
    }

    /// Vanilla's `readEnum` throws on an ordinal past the end, which drops the
    /// connection; a fifth book must not be read as some other one.
    #[test]
    fn an_unknown_book_is_rejected() {
        let buffer = [4u8, 1, 1];
        assert!(SRecipeBookChangeSettings::read(&mut Cursor::new(buffer.as_slice())).is_err());
    }
}
