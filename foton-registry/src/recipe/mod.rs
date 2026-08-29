//! Recipe system for crafting and other recipe types.
//!
//! This module provides the data structures and matching logic for Minecraft recipes.
//! Supports crafting (shaped and shapeless), cooking, stonecutting and
//! smithing transformations.

mod cooking;
mod crafting;
mod ingredient;
mod registry;
mod smithing;
mod stonecutting;

pub use cooking::{CookingKind, SmeltingRecipe};
pub use crafting::{
    CraftingCategory, CraftingInput, CraftingRecipe, PositionedCraftingInput, RecipeResult,
    ShapedRecipe, ShapelessRecipe,
};
pub use ingredient::Ingredient;
pub use registry::RecipeRegistry;
pub use smithing::SmithingTransformRecipe;
pub use stonecutting::StonecuttingRecipe;
