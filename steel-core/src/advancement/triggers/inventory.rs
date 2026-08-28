//! Triggers that fire from what a player is carrying.

use steel_registry::advancement::TriggerInstance;
use steel_registry::advancement::predicate::ItemPredicate;
use steel_registry::advancement::trigger::SlotsPredicate;
use steel_registry::item_stack::ItemStack;

use super::fire;
use crate::advancement::predicate::item_matches;
use crate::player::Player;

/// Fired once for every player-inventory slot whose stack changed.
///
/// Vanilla parity: `CriteriaTriggers.INVENTORY_CHANGED`, which `ServerPlayer`'s
/// container listener invokes from `AbstractContainerMenu.broadcastChanges`,
/// once per changed slot and carrying the stack that landed in it.
///
/// Which stack changed is not a detail: a criterion naming exactly one item
/// predicate tests *that stack* and nothing else, so an item already sitting
/// in the inventory never re-awards one. `story/root` is such a criterion.
pub fn inventory_changed(player: &Player) {
    let Some(change) = player.take_inventory_change() else {
        return;
    };

    for changed in &change.changed {
        fire(player, "minecraft:inventory_changed", |instance| {
            let TriggerInstance::InventoryChanged { slots, items, .. } = instance else {
                return false;
            };
            matches(slots, items, &change.items, changed)
        });
    }
}

/// Vanilla parity: `InventoryChangeTrigger.TriggerInstance.matches`.
fn matches(
    slots: &SlotsPredicate,
    items: &[ItemPredicate],
    inventory: &[ItemStack],
    changed: &ItemStack,
) -> bool {
    if !slots_match(slots, inventory) {
        return false;
    }
    if items.is_empty() {
        return true;
    }
    // The one-predicate case reads the changed stack and only it. Falling
    // through to the inventory sweep below would award `story/root` to anyone
    // who already had a crafting table the moment any other slot moved.
    let [only] = items else {
        return every_predicate_is_satisfied(items, inventory);
    };
    !changed.is_empty() && item_matches(only, changed)
}

/// Vanilla parity: the multi-predicate half of `matches`, which drops every
/// predicate a stack satisfies as it walks the inventory. One stack can
/// therefore answer several predicates at once, which is how
/// `husbandry/balanced_diet` counts.
fn every_predicate_is_satisfied(items: &[ItemPredicate], inventory: &[ItemStack]) -> bool {
    let mut unmet: Vec<&ItemPredicate> = items.iter().collect();
    for stack in inventory {
        if unmet.is_empty() {
            return true;
        }
        if stack.is_empty() {
            continue;
        }
        unmet.retain(|predicate| !item_matches(predicate, stack));
    }
    unmet.is_empty()
}

/// Vanilla parity: `InventoryChangeTrigger.TriggerInstance.Slots.matches`,
/// whose three counts are taken over the whole inventory container.
fn slots_match(slots: &SlotsPredicate, inventory: &[ItemStack]) -> bool {
    if slots.occupied.is_any() && slots.full.is_any() && slots.empty.is_any() {
        return true;
    }

    let (mut full, mut empty, mut occupied) = (0, 0, 0);
    for stack in inventory {
        if stack.is_empty() {
            empty += 1;
            continue;
        }
        occupied += 1;
        if stack.count() >= stack.max_stack_size() {
            full += 1;
        }
    }

    slots.full.matches(full) && slots.empty.matches(empty) && slots.occupied.matches(occupied)
}

#[cfg(test)]
mod tests {
    use steel_registry::advancement::predicate::{IntBounds, RegistrySet};
    use steel_registry::{init_vanilla_registry, vanilla_items};
    use steel_utils::Identifier;

    use super::{ItemPredicate, ItemStack, SlotsPredicate, matches};

    static COBBLESTONE: &[Identifier] = &[Identifier::vanilla_static("cobblestone")];
    static CRAFTING_TABLE: &[Identifier] = &[Identifier::vanilla_static("crafting_table")];

    fn wanting(entries: &'static [Identifier]) -> ItemPredicate {
        ItemPredicate {
            items: Some(RegistrySet::Entries(entries)),
            ..ItemPredicate::ANY
        }
    }

    /// The trap this whole function exists for. Vanilla's one-predicate branch
    /// tests the stack that *changed* and nothing else, so an item already
    /// sitting in the inventory does not re-award when some other slot moves.
    /// Falling through to the multi-predicate sweep -- which reads the same
    /// inventory and would say yes -- is the tidy-up that breaks it.
    #[test]
    fn one_predicate_reads_the_changed_stack_and_not_the_inventory() {
        init_vanilla_registry();

        let items = [wanting(COBBLESTONE)];
        let inventory = [
            ItemStack::new(&vanilla_items::COBBLESTONE),
            ItemStack::new(&vanilla_items::CRAFTING_TABLE),
        ];

        assert!(
            matches(
                &SlotsPredicate::ANY,
                &items,
                &inventory,
                &ItemStack::new(&vanilla_items::COBBLESTONE)
            ),
            "the cobblestone that just landed is what the criterion asked for"
        );
        assert!(
            !matches(
                &SlotsPredicate::ANY,
                &items,
                &inventory,
                &ItemStack::new(&vanilla_items::CRAFTING_TABLE)
            ),
            "a different slot moving must not re-award cobblestone the player already had"
        );
        assert!(
            !matches(
                &SlotsPredicate::ANY,
                &items,
                &inventory,
                &ItemStack::empty()
            ),
            "a slot emptied out is `!changedItem.isEmpty()` in vanilla, so it awards nothing"
        );
    }

    /// The many-predicate branch is the other half: it sweeps the inventory and
    /// ignores what changed, and one stack can answer several predicates at
    /// once because vanilla drops every predicate a stack satisfies.
    #[test]
    fn many_predicates_sweep_the_inventory_and_one_stack_can_answer_several() {
        init_vanilla_registry();

        let items = [wanting(COBBLESTONE), wanting(CRAFTING_TABLE)];
        let both = [
            ItemStack::new(&vanilla_items::COBBLESTONE),
            ItemStack::new(&vanilla_items::CRAFTING_TABLE),
        ];
        let only_one = [ItemStack::new(&vanilla_items::COBBLESTONE)];

        // The changed stack is deliberately empty: the sweep must not read it.
        assert!(matches(
            &SlotsPredicate::ANY,
            &items,
            &both,
            &ItemStack::empty()
        ));
        assert!(!matches(
            &SlotsPredicate::ANY,
            &items,
            &only_one,
            &ItemStack::empty()
        ));

        // Two predicates the same stack satisfies are both dropped by it.
        let twice = [wanting(COBBLESTONE), wanting(COBBLESTONE)];
        assert!(matches(
            &SlotsPredicate::ANY,
            &twice,
            &only_one,
            &ItemStack::empty()
        ));
    }

    /// The slot counts are taken over the whole container, and `full` means a
    /// stack at its own maximum size rather than any non-empty slot.
    #[test]
    fn the_slot_counts_separate_occupied_from_full() {
        init_vanilla_registry();

        let inventory = [
            ItemStack::with_count(&vanilla_items::COBBLESTONE, 64),
            ItemStack::new(&vanilla_items::COBBLESTONE),
            ItemStack::empty(),
        ];
        let exactly = |min, max| IntBounds {
            min: Some(min),
            max: Some(max),
        };

        let counts = SlotsPredicate {
            occupied: exactly(2, 2),
            full: exactly(1, 1),
            empty: exactly(1, 1),
        };
        assert!(matches(&counts, &[], &inventory, &ItemStack::empty()));

        let wrong_full = SlotsPredicate {
            full: exactly(2, 2),
            ..counts
        };
        assert!(!matches(&wrong_full, &[], &inventory, &ItemStack::empty()));
    }
}
