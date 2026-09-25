//! The player's recipe book.
//!
//! Vanilla parity: `ServerRecipeBook` and the `ServerPlayer` methods around
//! it -- `awardRecipes`, `resetRecipes`, `awardRecipesByKey` -- plus the two
//! `ServerGamePacketListenerImpl` handlers that feed it. The book is the set
//! of recipes a player knows, the subset still marked new, and how each of
//! the four screens left its book. What each recipe *looks like* in the book
//! is the recipe registry's business ([`foton_registry::recipe::BookRecipe`]).

use foton_protocol::packets::game::{
    CRecipeBookAdd, CRecipeBookRemove, CRecipeBookSettings, RecipeBookAddEntry,
    RecipeBookSettings, RecipeBookType, RecipeBookTypeSettings,
};
use foton_registry::REGISTRY;
use foton_utils::Identifier;
use foton_utils::codec::VarInt;
use rustc_hash::FxHashSet;

use super::Player;
use crate::advancement::triggers::recipe::recipe_unlocked;

/// The book's state, with no player attached.
///
/// Vanilla parity: `ServerRecipeBook.known`, `highlight` and `bookSettings`.
#[derive(Debug, Default)]
pub struct ServerRecipeBook {
    known: FxHashSet<Identifier>,
    highlight: FxHashSet<Identifier>,
    settings: RecipeBookSettings,
}

/// The recipe book as it is saved.
///
/// Vanilla parity: `ServerRecipeBook.Packed` -- the `recipes` and
/// `toBeDisplayed` lists and the eight `is...GuiOpen` / `is...FilteringCraftable`
/// flags of the `recipeBook` tag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersistentRecipeBook {
    /// `recipes`: every recipe the player knows.
    pub recipes: Vec<String>,
    /// `toBeDisplayed`: the ones the book still marks as new.
    pub to_be_displayed: Vec<String>,
    /// The four screens, in `RecipeBookType` order, open before filtering.
    pub settings: [bool; 8],
}

const BOOK_TYPES: [RecipeBookType; 4] = [
    RecipeBookType::Crafting,
    RecipeBookType::Furnace,
    RecipeBookType::BlastFurnace,
    RecipeBookType::Smoker,
];

impl ServerRecipeBook {
    /// Vanilla parity: `ServerRecipeBook.pack`. The two sets are sorted so the
    /// same book always saves to the same bytes; vanilla's identity sets have
    /// no order to preserve.
    #[must_use]
    pub fn pack(&self) -> PersistentRecipeBook {
        let sorted = |set: &FxHashSet<Identifier>| {
            let mut keys: Vec<String> = set.iter().map(ToString::to_string).collect();
            keys.sort_unstable();
            keys
        };
        let mut settings = [false; 8];
        for (index, book) in BOOK_TYPES.into_iter().enumerate() {
            let book = self.settings.get(book);
            settings[index * 2] = book.open;
            settings[index * 2 + 1] = book.filtering;
        }
        PersistentRecipeBook {
            recipes: sorted(&self.known),
            to_be_displayed: sorted(&self.highlight),
            settings,
        }
    }

    /// Replaces this book with a saved one, keeping only the recipes `exists`
    /// recognizes.
    ///
    /// Vanilla parity: `ServerRecipeBook.loadUntrusted` on a fresh book, which
    /// drops every key the recipe manager no longer has with an error rather
    /// than refusing the whole save.
    pub fn load(&mut self, saved: &PersistentRecipeBook, exists: impl Fn(&Identifier) -> bool) {
        *self = Self::default();
        for (index, book) in BOOK_TYPES.into_iter().enumerate() {
            self.settings.set(
                book,
                RecipeBookTypeSettings {
                    open: saved.settings[index * 2],
                    filtering: saved.settings[index * 2 + 1],
                },
            );
        }
        let restore = |keys: &[String], into: &mut FxHashSet<Identifier>| {
            for key in keys {
                match key.parse::<Identifier>() {
                    Ok(recipe) if exists(&recipe) => {
                        into.insert(recipe);
                    }
                    _ => log::error!("Tried to load unrecognized recipe: {key} removed now."),
                }
            }
        };
        restore(&saved.recipes, &mut self.known);
        restore(&saved.to_be_displayed, &mut self.highlight);
    }
}

impl Player {
    /// Unlocks recipes, telling the client about the ones that are new.
    ///
    /// Vanilla parity: `ServerPlayer.awardRecipesByKey` into
    /// `ServerRecipeBook.addRecipes`. A key naming no recipe is skipped, as
    /// `byKey` skips it; every newly known recipe is highlighted, fires
    /// `recipe_unlocked`, and goes out in one non-replacing add packet. Returns
    /// how many displays were added, which is vanilla's return value and the
    /// count Bukkit's `discoverRecipes` reports.
    pub fn award_recipes<'a>(&self, keys: impl IntoIterator<Item = &'a Identifier>) -> usize {
        let mut entries = Vec::new();
        let mut unlocked = Vec::new();
        {
            let mut book = self.recipe_book.lock();
            for key in keys {
                if book.known.contains(key) {
                    continue;
                }
                let Some(recipe) = REGISTRY.recipes.book_recipe(key) else {
                    continue;
                };
                book.known.insert(key.clone());
                book.highlight.insert(key.clone());
                entries.extend(recipe.displays.into_iter().map(|contents| RecipeBookAddEntry {
                    contents,
                    notification: recipe.show_notification,
                    highlight: true,
                }));
                unlocked.push(key.clone());
            }
        }
        for key in &unlocked {
            recipe_unlocked(self, key);
        }
        let added = entries.len();
        if added > 0 {
            self.send_packet(CRecipeBookAdd {
                entries,
                replace: false,
            });
        }
        added
    }

    /// Forgets recipes, taking the known ones out of the client's book.
    ///
    /// Vanilla parity: `ServerPlayer.resetRecipes` into
    /// `ServerRecipeBook.removeRecipes`. Returns how many displays were
    /// removed.
    pub fn reset_recipes<'a>(&self, keys: impl IntoIterator<Item = &'a Identifier>) -> usize {
        let mut removed = Vec::new();
        {
            let mut book = self.recipe_book.lock();
            for key in keys {
                if !book.known.remove(key) {
                    continue;
                }
                book.highlight.remove(key);
                if let Some(recipe) = REGISTRY.recipes.book_recipe(key) {
                    removed.extend(recipe.displays.iter().map(|display| VarInt(display.id)));
                }
            }
        }
        let count = removed.len();
        if count > 0 {
            self.send_packet(CRecipeBookRemove { recipes: removed });
        }
        count
    }

    /// Whether the player knows a recipe.
    ///
    /// Vanilla parity: `ServerRecipeBook.contains`.
    #[must_use]
    pub fn has_recipe(&self, key: &Identifier) -> bool {
        self.recipe_book.lock().known.contains(key)
    }

    /// Every recipe the player knows.
    #[must_use]
    pub fn known_recipes(&self) -> Vec<Identifier> {
        self.recipe_book.lock().known.iter().cloned().collect()
    }

    /// Sends the whole book: the screen settings, then every known recipe in
    /// one replacing add packet.
    ///
    /// Vanilla parity: `ServerRecipeBook.sendInitialRecipeBook`, sent from
    /// `PlayerList.placeNewPlayer`. Foton also sends it on arriving in another
    /// domain, because the book is saved per domain and the client would
    /// otherwise keep the one it left behind.
    pub fn send_initial_recipe_book(&self) {
        let (settings, entries) = {
            let book = self.recipe_book.lock();
            let mut entries = Vec::with_capacity(book.known.len());
            for key in &book.known {
                let Some(recipe) = REGISTRY.recipes.book_recipe(key) else {
                    continue;
                };
                let highlight = book.highlight.contains(key);
                entries.extend(recipe.displays.into_iter().map(|contents| RecipeBookAddEntry {
                    contents,
                    notification: false,
                    highlight,
                }));
            }
            (book.settings, entries)
        };
        self.send_packet(CRecipeBookSettings { settings });
        self.send_packet(CRecipeBookAdd {
            entries,
            replace: true,
        });
    }

    /// Remembers how the client left one screen's book.
    ///
    /// Vanilla parity: `handleRecipeBookChangeSettingsPacket` into
    /// `RecipeBook.setBookSetting`.
    pub fn handle_recipe_book_change_settings(
        &self,
        book_type: RecipeBookType,
        open: bool,
        filtering: bool,
    ) {
        self.recipe_book
            .lock()
            .settings
            .set(book_type, RecipeBookTypeSettings { open, filtering });
    }

    /// Stops highlighting a recipe the player has now looked at.
    ///
    /// Vanilla parity: `handleRecipeBookSeenRecipePacket`, which ignores an
    /// unknown display id.
    pub fn handle_recipe_book_seen_recipe(&self, display_id: i32) {
        let Some(key) = REGISTRY.recipes.recipe_for_display(display_id) else {
            return;
        };
        self.recipe_book.lock().highlight.remove(&key);
    }

    /// The book in the shape the save file keeps.
    #[must_use]
    pub fn saved_recipe_book(&self) -> PersistentRecipeBook {
        self.recipe_book.lock().pack()
    }

    /// Replaces the book with a saved one.
    ///
    /// Vanilla parity: the `recipeBook` half of `ServerPlayer.readAdditionalSaveData`,
    /// whose validator is `RecipeManager.byKey(id).isPresent()`.
    pub fn load_recipe_book(&self, saved: &PersistentRecipeBook) {
        self.recipe_book
            .lock()
            .load(saved, |key| REGISTRY.recipes.book_recipe(key).is_some());
    }

    /// Forgets every recipe and setting, for a player entering a domain they
    /// have never visited.
    pub fn reset_recipe_book(&self) {
        *self.recipe_book.lock() = ServerRecipeBook::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(path: &'static str) -> Identifier {
        Identifier::vanilla_static(path)
    }

    /// A save written by one book and read by another comes back whole, and a
    /// recipe the server no longer has is dropped rather than failing the load.
    #[test]
    fn a_saved_book_loads_back_without_the_recipes_that_are_gone() {
        let mut book = ServerRecipeBook::default();
        book.known.extend([key("cake"), key("oak_planks"), key("removed")]);
        book.highlight.insert(key("cake"));
        book.settings.set(
            RecipeBookType::BlastFurnace,
            RecipeBookTypeSettings {
                open: true,
                filtering: true,
            },
        );

        let saved = book.pack();
        assert_eq!(
            saved.recipes,
            ["minecraft:cake", "minecraft:oak_planks", "minecraft:removed"]
        );
        assert_eq!(
            saved.settings,
            [false, false, false, false, true, true, false, false]
        );

        let mut restored = ServerRecipeBook::default();
        restored.load(&saved, |recipe| recipe != &key("removed"));

        assert_eq!(restored.known, FxHashSet::from_iter([key("cake"), key("oak_planks")]));
        assert_eq!(restored.highlight, FxHashSet::from_iter([key("cake")]));
        assert_eq!(restored.settings, book.settings);
    }
}
