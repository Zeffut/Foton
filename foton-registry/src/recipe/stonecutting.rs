//! Stonecutting recipes.
//!
//! Vanilla parity: `StonecutterRecipe`. One ingredient in, one stack out, and
//! no shape or timing at all -- the interesting part is that a single input
//! usually matches many recipes and the player picks which one, which is why
//! the registry hands back every match rather than the first.

use foton_utils::Identifier;

use crate::item_stack::ItemStack;

use super::{Ingredient, RecipeResult};

/// A stonecutter recipe.
#[derive(Debug)]
pub struct StonecuttingRecipe {
    pub id: Identifier,
    pub ingredient: Ingredient,
    pub result: RecipeResult,
}

impl StonecuttingRecipe {
    /// Returns whether this recipe accepts `input`.
    #[must_use]
    pub fn matches(&self, input: &ItemStack) -> bool {
        self.ingredient.test(input)
    }
}

#[cfg(test)]
mod tests {
    use foton_utils::Identifier;

    use crate::recipe::{Ingredient, RecipeResult};
    use crate::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};

    use super::*;

    #[test]
    fn a_stonecutting_recipe_matches_only_its_ingredient() {
        init_vanilla_registry();
        let recipe = StonecuttingRecipe {
            id: Identifier::vanilla_static("test"),
            ingredient: Ingredient::Item(&vanilla_items::ANDESITE),
            result: RecipeResult {
                item: &vanilla_items::ANDESITE_SLAB,
                count: 2,
            },
        };

        assert!(recipe.matches(&ItemStack::new(&vanilla_items::ANDESITE)));
        assert!(!recipe.matches(&ItemStack::new(&vanilla_items::DIORITE)));
    }
}
