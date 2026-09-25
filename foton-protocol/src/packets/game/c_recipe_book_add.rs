//! Clientbound packet adding recipes to the recipe book.

use std::io::{Result, Write};

use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_RECIPE_BOOK_ADD;
use foton_registry::recipe::RecipeDisplayEntry;
use foton_utils::serial::WriteTo;

/// One recipe display handed to the recipe book.
///
/// Vanilla parity: `ClientboundRecipeBookAddPacket.Entry`, whose two booleans
/// travel as one flag byte.
#[derive(Debug, Clone)]
pub struct RecipeBookAddEntry {
    pub contents: RecipeDisplayEntry,
    /// Whether the client pops an unlock toast for it.
    pub notification: bool,
    /// Whether the book marks it new until the player looks at it.
    pub highlight: bool,
}

impl RecipeBookAddEntry {
    const FLAG_NOTIFICATION: u8 = 1;
    const FLAG_HIGHLIGHT: u8 = 2;
}

impl WriteTo for RecipeBookAddEntry {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.contents.write(writer)?;
        let mut flags = 0;
        if self.notification {
            flags |= Self::FLAG_NOTIFICATION;
        }
        if self.highlight {
            flags |= Self::FLAG_HIGHLIGHT;
        }
        flags.write(writer)
    }
}

/// Adds recipe displays to the client's recipe book.
///
/// Vanilla parity: `ClientboundRecipeBookAddPacket`. With `replace` the client
/// first forgets every recipe it knew, which is how the whole book is sent on
/// joining; without it the entries are unlocks, and the ones flagged for it
/// pop a toast.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_RECIPE_BOOK_ADD)]
pub struct CRecipeBookAdd {
    pub entries: Vec<RecipeBookAddEntry>,
    pub replace: bool,
}

#[cfg(test)]
mod tests {
    use foton_registry::recipe::RecipeDisplayEntry;
    use foton_registry::{REGISTRY, init_vanilla_registry};
    use foton_utils::Identifier;

    use super::*;

    /// `ClientboundRecipeBookAddPacket.STREAM_CODEC` run by vanilla 26.2 over
    /// `minecraft:cake` (display 0, ungrouped, toast and highlight) and
    /// `minecraft:iron_ingot_from_smelting_raw_iron` (display 1, group 0,
    /// highlight only), each built from its datapack JSON with vanilla's own
    /// recipe classes.
    const VANILLA: &str = concat!(
        "0200010303090a01090496080590080100000a01090496080590080100000a01090496080590080100000a01",
        "04d908060e6d696e6563726166743a656767730a0104d9080a0104d4070a0104d4070a0104d40705da080100",
        "0004e8020003010902960802960802960802d908000e6d696e6563726166743a6567677302d90802d40702d4",
        "0702d4070301020a0104a3070105a40701000004ea02c8013f3333330106010102a3070200",
    );

    fn entry(key: &'static str, id: i32, group: Option<i32>) -> RecipeDisplayEntry {
        let recipe = REGISTRY
            .recipes
            .book_recipe(&Identifier::vanilla_static(key))
            .unwrap_or_else(|| panic!("{key} is a vanilla recipe"));
        let [display] = <[RecipeDisplayEntry; 1]>::try_from(recipe.displays)
            .unwrap_or_else(|_| panic!("{key} has exactly one display"));
        RecipeDisplayEntry {
            id,
            group,
            ..display
        }
    }

    /// The whole chain in one: the recipe's own data, vanilla's rules for
    /// turning it into a display (the milk bucket's remainder, the tag kept as
    /// a tag, the furnace's fuel and category), and the wire shape of every
    /// field down to the optional group being a shifted VarInt.
    #[test]
    fn encodes_vanilla_recipes_byte_for_byte_like_vanilla() {
        init_vanilla_registry();
        let packet = CRecipeBookAdd {
            entries: vec![
                RecipeBookAddEntry {
                    contents: entry("cake", 0, None),
                    notification: true,
                    highlight: true,
                },
                RecipeBookAddEntry {
                    contents: entry("iron_ingot_from_smelting_raw_iron", 1, Some(0)),
                    notification: false,
                    highlight: true,
                },
            ],
            replace: false,
        };

        let mut bytes = Vec::new();
        packet.write(&mut bytes).expect("packet encodes");

        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(hex, VANILLA);
    }
}
