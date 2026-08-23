//! The knowledge book.

use steel_macros::item_behavior;
use steel_registry::REGISTRY;
use steel_registry::data_components::vanilla_components::RECIPES;

use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};

/// Grants the recipes it lists, then disappears.
///
/// Vanilla parity: `KnowledgeBookItem.use`, including its ordering quirk -- the
/// book is consumed before the list is checked, so an empty or broken one is
/// still eaten.
///
/// Steel gap: Steel has no recipe book. There is no per-player set of known
/// recipes, no persistence for one, and none of the eight recipe-book packets
/// exist beyond their generated ids, so nothing can be unlocked or told to the
/// client. Everything up to that point is ported: the component is read, the
/// stack is consumed, every listed recipe is resolved against the registry, and
/// an unresolvable one fails the use exactly as Vanilla's does.
#[item_behavior]
pub struct KnowledgeBookItem;

impl ItemBehavior for KnowledgeBookItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let recipes = context
            .inv
            .with_item(|item| item.get(RECIPES).map(|recipes| recipes.keys().to_vec()));

        // Vanilla parity: `ItemStack.consume(1, player)`, which spares creative.
        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        let Some(recipes) = recipes.filter(|keys| !keys.is_empty()) else {
            return InteractionResult::Fail;
        };

        for key in &recipes {
            if !REGISTRY.recipes.contains_key(key) {
                log::error!("Invalid recipe: {key}");
                return InteractionResult::Fail;
            }
        }

        // Vanilla awards the recipes here through `Player.awardRecipes`.
        InteractionResult::Success
    }
}
