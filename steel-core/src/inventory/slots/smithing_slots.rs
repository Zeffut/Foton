//! The smithing table's three inputs and its result.
//!
//! Vanilla parity: the slots of `SmithingMenu`. Three things have to line up
//! at once -- a template, the thing being upgraded, and what it is upgraded
//! with -- and the result carries the base's own components across, which is
//! what makes upgrading an enchanted tool worth doing.

use steel_registry::REGISTRY;
use steel_registry::item_stack::ItemStack;
use steel_utils::locks::Shared;

use crate::inventory::container::{Container as _, ResultContainer, SimpleContainer};
use crate::inventory::lock::{ContainerId, ContainerLockGuard, ContainerRef};
use crate::inventory::slots::ResultHandler;
use crate::player::Player;

/// The template slot.
pub const SMITHING_TEMPLATE: usize = 0;
/// The slot holding the item being upgraded.
pub const SMITHING_BASE: usize = 1;
/// The slot holding what it is upgraded with.
pub const SMITHING_ADDITION: usize = 2;

/// Keeps a smithing table's result in step with its three inputs.
#[derive(Clone)]
pub struct SmithingHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
}

impl SmithingHandler {
    /// Creates a handler over the smithing table's containers.
    #[must_use]
    pub const fn new(
        input_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
    ) -> Self {
        Self {
            input_container,
            result_container,
        }
    }

    fn input_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.input_container)
    }

    fn result_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.result_container)
    }

    /// Returns the three inputs.
    fn inputs(&self, guard: &ContainerLockGuard) -> Option<(ItemStack, ItemStack, ItemStack)> {
        let container = guard.get(self.input_id())?;
        Some((
            container.get_item(SMITHING_TEMPLATE).clone(),
            container.get_item(SMITHING_BASE).clone(),
            container.get_item(SMITHING_ADDITION).clone(),
        ))
    }
}

/// Returns what a smithing table makes of these three slots.
#[must_use]
pub fn smithing_result(template: &ItemStack, base: &ItemStack, addition: &ItemStack) -> ItemStack {
    REGISTRY
        .recipes
        .smithing_recipe_for(template, base, addition)
        .map_or_else(ItemStack::empty, |recipe| recipe.assemble(base))
}

impl ResultHandler for SmithingHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, guard: &mut ContainerLockGuard) {
        let result = self
            .inputs(guard)
            .map_or_else(ItemStack::empty, |(template, base, addition)| {
                smithing_result(&template, &base, &addition)
            });

        let result_id = self.result_id();
        let Some(container) = guard.get_typed_mut::<ResultContainer>(result_id) else {
            return;
        };
        container.set_item(0, result);
        container.set_changed();
    }

    /// Vanilla parity: the `onTake` of `SmithingMenu`'s result slot, which
    /// spends one of each of the three inputs.
    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        let input_id = self.input_id();
        if let Some(container) = guard.get_mut(input_id) {
            for slot in [SMITHING_TEMPLATE, SMITHING_BASE, SMITHING_ADDITION] {
                container.get_item_mut(slot).shrink(1);
            }
            container.set_changed();
        }
        self.update_result(guard);
        None
    }

    fn is_result_valid(&self, guard: &ContainerLockGuard, _player: &Player) -> bool {
        self.inputs(guard)
            .is_some_and(|(template, base, addition)| {
                !smithing_result(&template, &base, &addition).is_empty()
            })
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};
    use steel_utils::Identifier;

    use super::smithing_result;

    /// The extracted data really carries smithing transformations.
    ///
    /// The build script skipped these outright until now, so without this the
    /// tests below would be checking an empty recipe list.
    #[test]
    fn a_diamond_pickaxe_upgrades_to_netherite() {
        init_vanilla_registry();
        let result = smithing_result(
            &ItemStack::new(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
            &ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
            &ItemStack::new(&vanilla_items::NETHERITE_INGOT),
        );
        assert!(
            result.is(&vanilla_items::NETHERITE_PICKAXE),
            "got {:?}",
            result.item().key
        );
    }

    /// The upgrade carries the base's enchantments across.
    #[test]
    fn the_upgrade_keeps_what_was_on_the_tool() {
        init_vanilla_registry();
        let mut base = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        base.set_enchantments(&[(Identifier::vanilla_static("efficiency"), 5)], false);

        let result = smithing_result(
            &ItemStack::new(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
            &base,
            &ItemStack::new(&vanilla_items::NETHERITE_INGOT),
        );

        assert_eq!(
            result
                .get_enchantments_for_crafting()
                .map_or(0, |enchantments| enchantments
                    .get_level(&Identifier::vanilla_static("efficiency"))),
            5,
            "Efficiency V should survive the upgrade"
        );
    }

    /// Without the template nothing happens.
    ///
    /// Vanilla parity: the template ingredient is part of the match, which is
    /// what makes netherite upgrade templates a currency rather than a
    /// formality.
    #[test]
    fn no_template_no_upgrade() {
        init_vanilla_registry();
        let result = smithing_result(
            &ItemStack::empty(),
            &ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
            &ItemStack::new(&vanilla_items::NETHERITE_INGOT),
        );
        assert!(result.is_empty());
    }

    /// The addition has to be netherite.
    #[test]
    fn the_wrong_addition_makes_nothing() {
        init_vanilla_registry();
        let result = smithing_result(
            &ItemStack::new(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
            &ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
            &ItemStack::new(&vanilla_items::IRON_INGOT),
        );
        assert!(result.is_empty());
    }

    /// An empty table makes nothing.
    #[test]
    fn nothing_in_makes_nothing_out() {
        init_vanilla_registry();
        assert!(
            smithing_result(
                &ItemStack::empty(),
                &ItemStack::empty(),
                &ItemStack::empty()
            )
            .is_empty()
        );
    }
}
