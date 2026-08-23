//! Smithing recipes.
//!
//! Vanilla parity: `SmithingTransformRecipe`. Three slots in, one out, and the
//! result keeps everything the base had -- its enchantments, its damage, the
//! name somebody gave it on an anvil. That is the whole point: upgrading a
//! diamond pickaxe to netherite must not cost the Fortune III on it.
//!
//! Vanilla's other smithing recipe, `SmithingTrimRecipe`, is not here. Armor
//! trims need trim pattern and material registries and a `TRIM` component that
//! Steel does not have, so the build script still skips those eighteen
//! recipes.

use steel_utils::Identifier;

use crate::item_stack::ItemStack;

use super::{Ingredient, RecipeResult};

/// A smithing table transformation.
#[derive(Debug)]
pub struct SmithingTransformRecipe {
    pub id: Identifier,
    /// The template in the left slot; `Empty` when the recipe wants none.
    pub template: Ingredient,
    /// The item being upgraded, in the middle.
    pub base: Ingredient,
    /// What it is upgraded with, on the right.
    pub addition: Ingredient,
    pub result: RecipeResult,
}

impl SmithingTransformRecipe {
    /// Returns whether these three slots make this recipe.
    #[must_use]
    pub fn matches(&self, template: &ItemStack, base: &ItemStack, addition: &ItemStack) -> bool {
        self.template.test(template) && self.base.test(base) && self.addition.test(addition)
    }

    /// Builds the upgraded item from `base`.
    ///
    /// Vanilla parity: `TransmuteRecipe.createWithOriginalComponents` -- the
    /// base is changed into the result item and keeps its own components,
    /// rather than a fresh result being made from nothing.
    #[must_use]
    pub fn assemble(&self, base: &ItemStack) -> ItemStack {
        let mut upgraded = base.clone();
        upgraded.set_item(&self.result.item.key);
        upgraded.set_count(self.result.count);
        upgraded
    }
}

#[cfg(test)]
mod tests {
    use steel_utils::Identifier;

    use crate::recipe::{Ingredient, RecipeResult};
    use crate::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};

    use super::*;

    fn netherite_pickaxe_recipe() -> SmithingTransformRecipe {
        SmithingTransformRecipe {
            id: Identifier::vanilla_static("test"),
            template: Ingredient::Item(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
            base: Ingredient::Item(&vanilla_items::DIAMOND_PICKAXE),
            addition: Ingredient::Item(&vanilla_items::NETHERITE_INGOT),
            result: RecipeResult {
                item: &vanilla_items::NETHERITE_PICKAXE,
                count: 1,
            },
        }
    }

    #[test]
    fn all_three_slots_have_to_match() {
        init_vanilla_registry();
        let recipe = netherite_pickaxe_recipe();

        assert!(recipe.matches(
            &ItemStack::new(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
            &ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
            &ItemStack::new(&vanilla_items::NETHERITE_INGOT),
        ));

        assert!(
            !recipe.matches(
                &ItemStack::empty(),
                &ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
                &ItemStack::new(&vanilla_items::NETHERITE_INGOT),
            ),
            "no template, no upgrade"
        );
        assert!(
            !recipe.matches(
                &ItemStack::new(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
                &ItemStack::new(&vanilla_items::IRON_PICKAXE),
                &ItemStack::new(&vanilla_items::NETHERITE_INGOT),
            ),
            "an iron pickaxe does not upgrade to netherite"
        );
    }

    /// The upgrade keeps what was on the base.
    ///
    /// This is the reason smithing exists rather than crafting: losing a
    /// tool's enchantments to upgrade it would make the upgrade worthless.
    #[test]
    fn the_upgrade_keeps_the_enchantments() {
        init_vanilla_registry();
        let recipe = netherite_pickaxe_recipe();

        let mut base = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        base.set_enchantments(&[(Identifier::vanilla_static("fortune"), 3)], false);

        let upgraded = recipe.assemble(&base);

        assert!(upgraded.is(&vanilla_items::NETHERITE_PICKAXE));
        assert_eq!(
            upgraded
                .get_enchantments_for_crafting()
                .map_or(0, |enchantments| enchantments
                    .get_level(&Identifier::vanilla_static("fortune"))),
            3,
            "Fortune III should survive the upgrade"
        );
    }
}
