//! Recipe registry for looking up recipes.

use foton_utils::Identifier;
use rustc_hash::FxHashMap;

use super::cooking::{CookingKind, SmeltingRecipe};
use super::crafting::{CraftingInput, CraftingRecipe, ShapedRecipe, ShapelessRecipe};
use super::smithing::SmithingTransformRecipe;
use super::stonecutting::StonecuttingRecipe;
use crate::item_stack::ItemStack;

/// Registry for all recipes.
pub struct RecipeRegistry {
    /// All recipes in registration order (unified storage for `RegistryExt`).
    recipes_by_id: Vec<&'static CraftingRecipe>,
    /// Map from recipe key to index in `recipes_by_id`.
    recipes_by_key: FxHashMap<Identifier, usize>,
    /// All shaped crafting recipes (for type-specific iteration).
    shaped_recipes: Vec<&'static ShapedRecipe>,
    /// All shapeless crafting recipes (for type-specific iteration).
    shapeless_recipes: Vec<&'static ShapelessRecipe>,
    /// Every smithing table transformation.
    smithing_recipes: Vec<&'static SmithingTransformRecipe>,

    /// Every stonecutter recipe, in registration order.
    ///
    /// Order matters here in a way it does not for the other kinds: the
    /// client picks a recipe by its index in the list it was shown, so the
    /// order the server hands them out in is the order the buttons appear.
    stonecutting_recipes: Vec<&'static StonecuttingRecipe>,

    /// All furnace smelting recipes.
    smelting_recipes: Vec<&'static SmeltingRecipe>,
    /// All blast furnace recipes.
    blasting_recipes: Vec<&'static SmeltingRecipe>,
    /// All smoker recipes.
    smoking_recipes: Vec<&'static SmeltingRecipe>,
    /// All campfire recipes.
    ///
    /// Kept apart from the three furnace families rather than folded into
    /// [`CookingKind`]: a campfire burns no fuel and awards no experience on
    /// collection, so it shares only the recipe shape, not the block.
    campfire_recipes: Vec<&'static SmeltingRecipe>,
    /// Whether registration is still allowed.
    allows_registering: bool,
}

impl Default for RecipeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RecipeRegistry {
    /// Creates a new empty recipe registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            recipes_by_id: Vec::new(),
            recipes_by_key: FxHashMap::default(),
            shaped_recipes: Vec::new(),
            shapeless_recipes: Vec::new(),
            smithing_recipes: Vec::new(),
            stonecutting_recipes: Vec::new(),
            smelting_recipes: Vec::new(),
            blasting_recipes: Vec::new(),
            smoking_recipes: Vec::new(),
            campfire_recipes: Vec::new(),
            allows_registering: true,
        }
    }

    /// Registers a shaped recipe.
    pub fn register_shaped(&mut self, recipe: &'static ShapedRecipe) {
        assert!(
            self.allows_registering,
            "Cannot register recipes after the registry has been frozen"
        );
        let id = self.recipes_by_id.len();
        self.recipes_by_key.insert(recipe.id.clone(), id);
        self.recipes_by_id
            .push(Box::leak(Box::new(CraftingRecipe::Shaped(recipe))));
        self.shaped_recipes.push(recipe);
    }

    /// Registers a shapeless recipe.
    pub fn register_shapeless(&mut self, recipe: &'static ShapelessRecipe) {
        assert!(
            self.allows_registering,
            "Cannot register recipes after the registry has been frozen"
        );
        let id = self.recipes_by_id.len();
        self.recipes_by_key.insert(recipe.id.clone(), id);
        self.recipes_by_id
            .push(Box::leak(Box::new(CraftingRecipe::Shapeless(recipe))));
        self.shapeless_recipes.push(recipe);
    }

    /// Registers a smithing table transformation.
    pub fn register_smithing(&mut self, recipe: &'static SmithingTransformRecipe) {
        self.smithing_recipes.push(recipe);
    }

    /// Returns the transformation these three slots make, if any.
    ///
    /// Vanilla parity: `RecipeManager.getRecipeFor(SMITHING, ...)`. Unlike a
    /// stonecutter, at most one smithing recipe can match, so this returns the
    /// first rather than the list.
    #[must_use]
    pub fn smithing_recipe_for(
        &self,
        template: &ItemStack,
        base: &ItemStack,
        addition: &ItemStack,
    ) -> Option<&'static SmithingTransformRecipe> {
        self.smithing_recipes
            .iter()
            .find(|recipe| recipe.matches(template, base, addition))
            .copied()
    }

    /// Registers a stonecutter recipe.
    pub fn register_stonecutting(&mut self, recipe: &'static StonecuttingRecipe) {
        self.stonecutting_recipes.push(recipe);
    }

    /// Returns every stonecutter recipe that accepts `input`, in order.
    ///
    /// Vanilla parity: `RecipeManager.getRecipesFor(STONECUTTING, ...)`. Every
    /// match is returned rather than the first, because a stonecutter shows the
    /// player all of them and lets them choose.
    #[must_use]
    pub fn stonecutting_recipes_for(&self, input: &ItemStack) -> Vec<&'static StonecuttingRecipe> {
        self.stonecutting_recipes
            .iter()
            .filter(|recipe| recipe.matches(input))
            .copied()
            .collect()
    }

    /// Registers a furnace smelting recipe.
    pub fn register_smelting(&mut self, recipe: &'static SmeltingRecipe) {
        assert!(
            self.allows_registering,
            "Cannot register recipes after the registry has been frozen"
        );
        self.smelting_recipes.push(recipe);
    }

    /// Registers a blast furnace recipe.
    pub fn register_blasting(&mut self, recipe: &'static SmeltingRecipe) {
        assert!(
            self.allows_registering,
            "Cannot register recipes after the registry has been frozen"
        );
        self.blasting_recipes.push(recipe);
    }

    /// Registers a smoker recipe.
    pub fn register_smoking(&mut self, recipe: &'static SmeltingRecipe) {
        assert!(
            self.allows_registering,
            "Cannot register recipes after the registry has been frozen"
        );
        self.smoking_recipes.push(recipe);
    }

    /// Registers a campfire cooking recipe.
    pub fn register_campfire(&mut self, recipe: &'static SmeltingRecipe) {
        assert!(
            self.allows_registering,
            "Cannot register recipes after the registry has been frozen"
        );
        self.campfire_recipes.push(recipe);
    }

    /// Returns the campfire recipe that accepts `input`.
    ///
    /// Vanilla parity: `RecipeManager.getRecipeFor(RecipeType.CAMPFIRE_COOKING, ...)`.
    /// Doubles as `RecipePropertySet.CAMPFIRE_INPUT.test`, which vanilla builds
    /// from exactly these recipes' ingredients.
    #[must_use]
    pub fn find_campfire_recipe(&self, input: &ItemStack) -> Option<&'static SmeltingRecipe> {
        self.campfire_recipes
            .iter()
            .find(|recipe| recipe.matches(input))
            .copied()
    }

    /// Returns how many campfire recipes are registered.
    #[must_use]
    pub const fn campfire_count(&self) -> usize {
        self.campfire_recipes.len()
    }

    /// Returns the recipes of one cooking family.
    const fn cooking_recipes(&self, kind: CookingKind) -> &Vec<&'static SmeltingRecipe> {
        match kind {
            CookingKind::Smelting => &self.smelting_recipes,
            CookingKind::Blasting => &self.blasting_recipes,
            CookingKind::Smoking => &self.smoking_recipes,
        }
    }

    /// Finds the recipe of `kind` that accepts `input`.
    ///
    /// Unlike [`Self::find_smelting_result`], this returns the recipe itself, which
    /// a furnace needs for its cooking time and experience reward.
    #[must_use]
    pub fn find_cooking_recipe(
        &self,
        kind: CookingKind,
        input: &ItemStack,
    ) -> Option<&'static SmeltingRecipe> {
        self.cooking_recipes(kind)
            .iter()
            .find(|recipe| recipe.matches(input))
            .copied()
    }

    /// Finds a cooking recipe by its identifier, across every family.
    ///
    /// A furnace stores the recipe ids it has cooked so it can award their
    /// experience later, and only has the id to go on at that point.
    /// Returns whether any recipe of any kind is registered under `key`.
    ///
    /// Vanilla parity: `RecipeManager.byKey(...).isPresent()`, which searches
    /// the one `RecipeMap` holding every recipe type. Foton indexes crafting
    /// recipes by key and keeps the other families in flat lists, so the
    /// remaining kinds are scanned; a knowledge book is used rarely enough that
    /// the scan is cheaper than a second index.
    #[must_use]
    pub fn contains_key(&self, key: &Identifier) -> bool {
        if self.recipes_by_key.contains_key(key) {
            return true;
        }
        if self.find_cooking_recipe_by_id(key).is_some() {
            return true;
        }
        self.stonecutting_recipes
            .iter()
            .any(|recipe| &recipe.id == key)
            || self.smithing_recipes.iter().any(|recipe| &recipe.id == key)
    }

    #[must_use]
    pub fn find_cooking_recipe_by_id(&self, id: &Identifier) -> Option<&'static SmeltingRecipe> {
        self.smelting_recipes
            .iter()
            .chain(&self.blasting_recipes)
            .chain(&self.smoking_recipes)
            .chain(&self.campfire_recipes)
            .find(|recipe| &recipe.id == id)
            .copied()
    }

    /// Returns the number of recipes in one cooking family.
    #[must_use]
    pub const fn cooking_count(&self, kind: CookingKind) -> usize {
        self.cooking_recipes(kind).len()
    }

    /// Finds a matching crafting recipe for the given positioned input.
    /// Returns the first matching recipe, or None if no recipe matches.
    #[must_use]
    pub fn find_crafting_recipe(&self, input: &CraftingInput) -> Option<CraftingRecipe> {
        // Try shaped recipes first (they're more specific)
        for recipe in &self.shaped_recipes {
            if recipe.matches(input) {
                return Some(CraftingRecipe::Shaped(recipe));
            }
        }

        // Then try shapeless
        for recipe in &self.shapeless_recipes {
            if recipe.matches(input) {
                return Some(CraftingRecipe::Shapeless(recipe));
            }
        }

        None
    }

    /// Finds a matching crafting recipe for a 2x2 grid.
    /// Only checks recipes that can fit in a 2x2 grid.
    #[must_use]
    pub fn find_crafting_recipe_2x2(&self, input: &CraftingInput) -> Option<CraftingRecipe> {
        // Try shaped recipes first (they're more specific)
        for recipe in &self.shaped_recipes {
            if recipe.fits_in_2x2() && recipe.matches(input) {
                return Some(CraftingRecipe::Shaped(recipe));
            }
        }

        // Then try shapeless
        for recipe in &self.shapeless_recipes {
            if recipe.fits_in_2x2() && recipe.matches(input) {
                return Some(CraftingRecipe::Shapeless(recipe));
            }
        }

        None
    }

    /// Gets a shaped recipe by its identifier.
    #[must_use]
    pub fn get_shaped(&self, id: &Identifier) -> Option<&'static ShapedRecipe> {
        self.shaped_recipes.iter().find(|r| &r.id == id).copied()
    }

    /// Gets a shapeless recipe by its identifier.
    #[must_use]
    pub fn get_shapeless(&self, id: &Identifier) -> Option<&'static ShapelessRecipe> {
        self.shapeless_recipes.iter().find(|r| &r.id == id).copied()
    }

    /// Finds the first furnace smelting result stack for `input`.
    #[must_use]
    pub fn find_smelting_result(
        &self,
        input: &ItemStack,
        use_input_count: bool,
    ) -> Option<ItemStack> {
        self.smelting_recipes
            .iter()
            .find(|recipe| recipe.matches(input))
            .map(|recipe| recipe.assemble_result(input.count(), use_input_count))
    }

    /// Returns the number of shaped recipes.
    #[must_use]
    pub const fn shaped_count(&self) -> usize {
        self.shaped_recipes.len()
    }

    /// Returns the number of shapeless recipes.
    #[must_use]
    pub const fn shapeless_count(&self) -> usize {
        self.shapeless_recipes.len()
    }

    /// Returns the number of furnace smelting recipes.
    #[must_use]
    pub const fn smelting_count(&self) -> usize {
        self.smelting_recipes.len()
    }

    /// Iterates over all shaped recipes.
    pub fn iter_shaped(&self) -> impl Iterator<Item = &'static ShapedRecipe> + '_ {
        self.shaped_recipes.iter().copied()
    }

    /// Iterates over all shapeless recipes.
    pub fn iter_shapeless(&self) -> impl Iterator<Item = &'static ShapelessRecipe> + '_ {
        self.shapeless_recipes.iter().copied()
    }

    /// Iterates over all furnace smelting recipes.
    pub fn iter_smelting(&self) -> impl Iterator<Item = &'static SmeltingRecipe> + '_ {
        self.smelting_recipes.iter().copied()
    }
}

impl crate::RegistryExt for RecipeRegistry {
    type Entry = CraftingRecipe;

    fn freeze(&mut self) {
        self.allows_registering = false;
    }

    fn by_id(&self, id: usize) -> Option<&'static CraftingRecipe> {
        self.recipes_by_id.get(id).copied()
    }

    fn by_key(&self, key: &Identifier) -> Option<&'static CraftingRecipe> {
        self.recipes_by_key
            .get(key)
            .and_then(|&id| self.recipes_by_id.get(id).copied())
    }

    fn id_from_key(&self, key: &Identifier) -> Option<usize> {
        self.recipes_by_key.get(key).copied()
    }

    fn len(&self) -> usize {
        self.recipes_by_id.len()
    }

    fn is_empty(&self) -> bool {
        self.recipes_by_id.is_empty()
    }
}

impl crate::RegistryEntry for CraftingRecipe {
    fn key(&self) -> &Identifier {
        self.id()
    }

    fn try_id(&self) -> Option<usize> {
        use crate::RegistryExt;
        crate::REGISTRY.recipes.id_from_key(self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{REGISTRY, init_vanilla_registry, vanilla_items};

    #[test]
    fn every_cooking_family_is_populated() {
        init_vanilla_registry();
        let recipes = &REGISTRY.recipes;
        assert!(recipes.cooking_count(CookingKind::Smelting) > 0);
        assert!(recipes.cooking_count(CookingKind::Blasting) > 0);
        assert!(recipes.cooking_count(CookingKind::Smoking) > 0);
    }

    #[test]
    fn blasting_is_twice_as_fast_as_smelting_for_the_same_ore() {
        init_vanilla_registry();
        let recipes = &REGISTRY.recipes;
        let raw_iron = ItemStack::new(&vanilla_items::RAW_IRON);

        let smelted = recipes
            .find_cooking_recipe(CookingKind::Smelting, &raw_iron)
            .expect("raw iron smelts in a furnace");
        let blasted = recipes
            .find_cooking_recipe(CookingKind::Blasting, &raw_iron)
            .expect("raw iron smelts in a blast furnace");

        assert_eq!(smelted.cooking_time, 200);
        assert_eq!(blasted.cooking_time, 100);
        assert_eq!(smelted.result.item.key, blasted.result.item.key);
    }

    #[test]
    fn a_smoker_cooks_food_but_never_ore() {
        init_vanilla_registry();
        let recipes = &REGISTRY.recipes;

        let beef = ItemStack::new(&vanilla_items::BEEF);
        assert!(
            recipes
                .find_cooking_recipe(CookingKind::Smoking, &beef)
                .is_some()
        );

        let raw_iron = ItemStack::new(&vanilla_items::RAW_IRON);
        assert!(
            recipes
                .find_cooking_recipe(CookingKind::Smoking, &raw_iron)
                .is_none(),
            "a smoker must refuse ore"
        );
    }

    /// The campfire family is loaded from the same datapack directory as the
    /// furnace ones, but through its own build-script arm. A typo there would
    /// leave the list silently empty and every campfire inert, so this asserts
    /// on the count as well as on one lookup.
    #[test]
    fn a_campfire_cooks_food_slowly_and_never_ore() {
        init_vanilla_registry();
        let recipes = &REGISTRY.recipes;

        assert_eq!(
            recipes.campfire_count(),
            9,
            "vanilla ships nine campfire recipes"
        );

        let porkchop = ItemStack::new(&vanilla_items::PORKCHOP);
        let cooked = recipes
            .find_campfire_recipe(&porkchop)
            .expect("a campfire cooks porkchop");
        assert_eq!(cooked.result.item.key, vanilla_items::COOKED_PORKCHOP.key);
        assert_eq!(
            cooked.cooking_time, 600,
            "a campfire is six times slower than a furnace"
        );

        let raw_iron = ItemStack::new(&vanilla_items::RAW_IRON);
        assert!(
            recipes.find_campfire_recipe(&raw_iron).is_none(),
            "a campfire must refuse ore"
        );
    }
}
