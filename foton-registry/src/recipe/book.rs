//! The recipe registry as the recipe book sees it.
//!
//! Vanilla parity: `RecipeManager.allDisplays` and `recipeToDisplay`, built by
//! `unpackRecipeInfo`. Every recipe display gets the next id, and every
//! non-empty group string the next group number, in the order recipes are
//! indexed. Both numbers are the server's own handles -- the client only ever
//! sends them back -- so any order works as long as it never changes while a
//! client holds them; this index only ever appends.

use foton_utils::Identifier;
use rustc_hash::FxHashMap;

use super::display::{DisplaySource, RecipeDisplayEntry};

/// One recipe's standing in the recipe book.
#[derive(Debug, Clone)]
pub struct BookRecipe {
    /// Vanilla parity: `Recipe.showNotification`, whether unlocking it pops
    /// a toast.
    pub show_notification: bool,
    /// The displays the book draws for it. Every Foton recipe has exactly one,
    /// except one whose result cannot be shown at all.
    pub displays: Vec<RecipeDisplayEntry>,
}

/// Recipe displays by id and by recipe.
#[derive(Debug, Default)]
pub struct RecipeDisplayIndex {
    /// The recipe each display id belongs to; the position is the id.
    display_owners: Vec<Identifier>,
    recipes: FxHashMap<Identifier, BookRecipe>,
    groups: FxHashMap<String, i32>,
}

impl RecipeDisplayIndex {
    /// Adds a recipe, numbering its display and its group. A recipe already
    /// present keeps the ids it was given.
    pub(crate) fn insert(&mut self, key: &Identifier, source: DisplaySource<'_>) {
        if self.recipes.contains_key(key) {
            return;
        }
        let group = if source.group.is_empty() {
            None
        } else {
            let next = i32::try_from(self.groups.len()).ok();
            match self.groups.get(source.group) {
                Some(&group) => Some(group),
                None => next.inspect(|&group| {
                    self.groups.insert(source.group.to_owned(), group);
                }),
            }
        };
        let mut displays = Vec::new();
        if let Some(display) = source.display
            && let Ok(id) = i32::try_from(self.display_owners.len())
        {
            self.display_owners.push(key.clone());
            displays.push(RecipeDisplayEntry {
                id,
                display,
                group,
                category: source.category,
                crafting_requirements: Some(source.crafting_requirements),
            });
        }
        self.recipes.insert(
            key.clone(),
            BookRecipe {
                show_notification: source.show_notification,
                displays,
            },
        );
    }

    /// The recipe book's view of one recipe.
    #[must_use]
    pub fn recipe(&self, key: &Identifier) -> Option<&BookRecipe> {
        self.recipes.get(key)
    }

    /// The recipe a display id belongs to.
    ///
    /// Vanilla parity: `RecipeManager.getRecipeFromDisplay`, which answers
    /// nothing for an id outside the list rather than failing.
    #[must_use]
    pub fn recipe_for_display(&self, id: i32) -> Option<&Identifier> {
        usize::try_from(id)
            .ok()
            .and_then(|index| self.display_owners.get(index))
    }
}
