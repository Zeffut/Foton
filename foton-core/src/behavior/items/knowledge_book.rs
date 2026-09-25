//! The knowledge book.

use foton_macros::item_behavior;
use foton_registry::data_components::vanilla_components::RECIPES;
use foton_registry::{REGISTRY, vanilla_items, vanilla_stat_types};

use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};

/// Grants the recipes it lists, then disappears.
///
/// Vanilla parity: `KnowledgeBookItem.use`, including its ordering quirk -- the
/// book is consumed before the list is checked, so an empty or broken one is
/// still eaten, and one unknown recipe fails the whole book.
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

        context.player.award_recipes(&recipes);
        context
            .player
            .award_stat_for(&vanilla_stat_types::USED, &*vanilla_items::KNOWLEDGE_BOOK);
        InteractionResult::Success
    }
}
