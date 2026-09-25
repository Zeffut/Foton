//! Triggers that fire from the recipe book.

use foton_registry::advancement::TriggerInstance;
use foton_utils::Identifier;

use super::fire;
use crate::player::Player;

/// Vanilla parity: `CriteriaTriggers.RECIPE_UNLOCKED`, fired from
/// `ServerRecipeBook.addRecipes` for each recipe the book did not know. Every
/// vanilla `recipes/...` advancement listens for its own recipe here, which is
/// what stops it from granting that recipe again.
pub fn recipe_unlocked(player: &Player, key: &Identifier) {
    fire(player, "minecraft:recipe_unlocked", |instance| {
        matches!(instance, TriggerInstance::RecipeUnlocked { recipe, .. } if recipe == key)
    });
}
