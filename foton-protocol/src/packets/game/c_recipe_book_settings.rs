//! Clientbound packet carrying the recipe book's per-screen settings.

use std::io::{Result, Write};

use foton_macros::{ClientPacket, ReadFrom, WriteTo};
use foton_registry::packets::play::C_RECIPE_BOOK_SETTINGS;
use foton_utils::serial::WriteTo;

/// The screens that carry a recipe book.
///
/// Vanilla parity: `RecipeBookType`. Its ordinal is the wire form of
/// `ServerboundRecipeBookChangeSettingsPacket`, so the declaration order is
/// protocol-observable.
#[derive(ReadFrom, Clone, Copy, Debug, PartialEq, Eq)]
#[read(as = VarInt)]
pub enum RecipeBookType {
    Crafting = 0,
    Furnace = 1,
    BlastFurnace = 2,
    Smoker = 3,
}

/// Whether one screen's book is open, and whether it shows only what the
/// player can craft right now.
///
/// Vanilla parity: `RecipeBookSettings.TypeSettings`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecipeBookTypeSettings {
    pub open: bool,
    pub filtering: bool,
}

/// The recipe book settings of all four screens.
///
/// Vanilla parity: `RecipeBookSettings`, whose stream codec is the four
/// screens in `RecipeBookType` order, open before filtering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecipeBookSettings {
    pub crafting: RecipeBookTypeSettings,
    pub furnace: RecipeBookTypeSettings,
    pub blast_furnace: RecipeBookTypeSettings,
    pub smoker: RecipeBookTypeSettings,
}

impl RecipeBookSettings {
    /// One screen's settings.
    #[must_use]
    pub const fn get(&self, book: RecipeBookType) -> RecipeBookTypeSettings {
        match book {
            RecipeBookType::Crafting => self.crafting,
            RecipeBookType::Furnace => self.furnace,
            RecipeBookType::BlastFurnace => self.blast_furnace,
            RecipeBookType::Smoker => self.smoker,
        }
    }

    /// Replaces one screen's settings.
    ///
    /// Vanilla parity: `RecipeBook.setBookSetting`.
    pub const fn set(&mut self, book: RecipeBookType, settings: RecipeBookTypeSettings) {
        match book {
            RecipeBookType::Crafting => self.crafting = settings,
            RecipeBookType::Furnace => self.furnace = settings,
            RecipeBookType::BlastFurnace => self.blast_furnace = settings,
            RecipeBookType::Smoker => self.smoker = settings,
        }
    }
}

impl WriteTo for RecipeBookSettings {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        for book in [self.crafting, self.furnace, self.blast_furnace, self.smoker] {
            book.open.write(writer)?;
            book.filtering.write(writer)?;
        }
        Ok(())
    }
}

/// Tells the client how each recipe book screen was last left.
///
/// Vanilla parity: `ClientboundRecipeBookSettingsPacket`, sent once, just
/// before the book's contents, on joining.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_RECIPE_BOOK_SETTINGS)]
pub struct CRecipeBookSettings {
    pub settings: RecipeBookSettings,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vanilla 26.2's `ClientboundRecipeBookSettingsPacket.STREAM_CODEC` over
    /// a crafting book left open, a furnace book filtering and a smoker book
    /// open. A mixed-up pair or screen order shows up as a moved byte.
    #[test]
    fn encodes_each_screen_in_vanilla_order() {
        let mut settings = RecipeBookSettings::default();
        settings.set(
            RecipeBookType::Crafting,
            RecipeBookTypeSettings {
                open: true,
                filtering: false,
            },
        );
        settings.set(
            RecipeBookType::Furnace,
            RecipeBookTypeSettings {
                open: false,
                filtering: true,
            },
        );
        settings.set(
            RecipeBookType::Smoker,
            RecipeBookTypeSettings {
                open: true,
                filtering: false,
            },
        );

        let mut bytes = Vec::new();
        CRecipeBookSettings { settings }
            .write(&mut bytes)
            .expect("packet encodes");

        assert_eq!(bytes, [1, 0, 0, 1, 0, 0, 1, 0]);
    }
}
