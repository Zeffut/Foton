//! The editable view of a bundle's contents.
//!
//! Vanilla keeps `BundleContents.Mutable` inside the component itself. Steel
//! cannot: deciding whether an item may go into a bundle asks the item's
//! behavior (`ItemBehavior::can_fit_inside_container_items`), which lives in
//! `steel-core`, while the component lives in `steel-registry`. The weight rule
//! therefore stays with the component and the editing lives here.

use steel_registry::ItemStackTemplate;
use steel_registry::data_components::{BundleContents, CheckedFraction};
use steel_registry::item_stack::ItemStack;

use crate::behavior::ITEM_BEHAVIORS;
use crate::inventory::lock::ContainerLockGuard;
use crate::inventory::slots::slot::Slot;
use crate::player::Player;

/// Returns whether `stack` is something a bundle will hold.
///
/// Vanilla parity: `BundleContents.canItemBeInBundle`.
#[must_use]
pub fn can_item_be_in_bundle(item_to_add: &ItemStack) -> bool {
    !item_to_add.is_empty()
        && ITEM_BEHAVIORS
            .get_behavior(item_to_add.item())
            .can_fit_inside_container_items()
}

/// A bundle's contents opened up for editing.
///
/// Vanilla parity: `BundleContents.Mutable`.
pub struct MutableBundleContents {
    items: Vec<ItemStack>,
    weight: CheckedFraction,
    selected_item: i32,
}

impl MutableBundleContents {
    /// Opens `contents` for editing.
    ///
    /// Vanilla parity: `BundleContents.Mutable(BundleContents)`. Contents whose
    /// weight cannot be computed open empty, which is how Vanilla lets a player
    /// recover a bundle that commands have stuffed past the arithmetic limits.
    #[must_use]
    pub fn new(contents: &BundleContents) -> Self {
        let Ok(weight) = contents.weight() else {
            return Self {
                items: Vec::new(),
                weight: CheckedFraction::ZERO,
                selected_item: BundleContents::NO_SELECTED_ITEM_INDEX,
            };
        };

        Self {
            items: contents
                .items()
                .iter()
                .map(ItemStackTemplate::create)
                .collect(),
            weight,
            selected_item: contents.selected_item_index(),
        }
    }

    /// Vanilla parity: `BundleContents.Mutable.weight`.
    #[must_use]
    pub const fn weight(&self) -> CheckedFraction {
        self.weight
    }

    /// Vanilla parity: `BundleContents.Mutable.findStackIndex`.
    fn find_stack_index(&self, items_to_add: &ItemStack) -> Option<usize> {
        if !items_to_add.is_stackable() {
            return None;
        }
        self.items
            .iter()
            .position(|item| ItemStack::is_same_item_same_components(item, items_to_add))
    }

    /// Vanilla parity: `BundleContents.Mutable.getMaxAmountToAdd`, whose
    /// `Math.max(.., 0)` is folded into the guards below.
    fn max_amount_to_add(&self, item_weight: CheckedFraction) -> i32 {
        if self.weight >= CheckedFraction::ONE {
            return 0;
        }
        let Ok(remaining) = CheckedFraction::ONE.subtract(self.weight) else {
            return 0;
        };
        remaining.divide_to_int(item_weight).unwrap_or(0)
    }

    /// Moves as much of `items_to_add` into the bundle as its weight allows,
    /// shrinking it by what was taken, and returns that count.
    ///
    /// Vanilla parity: `BundleContents.Mutable.tryInsert`.
    ///
    /// Steel deviation: the resulting stack is checked against the persistent
    /// item-template codec before it is committed, so [`Self::to_immutable`]
    /// can never lose an item. Vanilla builds that template unchecked and
    /// throws out of the click handler instead.
    pub fn try_insert(&mut self, items_to_add: &mut ItemStack) -> i32 {
        if !can_item_be_in_bundle(items_to_add) {
            return 0;
        }
        let Ok(item_weight) = BundleContents::stack_unit_weight(items_to_add) else {
            return 0;
        };

        let amount_to_add = items_to_add
            .count()
            .min(self.max_amount_to_add(item_weight));
        if amount_to_add <= 0 {
            return 0;
        }
        let Ok(new_weight) = item_weight
            .multiply(amount_to_add)
            .and_then(|added| self.weight.add(added))
        else {
            return 0;
        };

        if let Some(stack_index) = self.find_stack_index(items_to_add) {
            let existing = &self.items[stack_index];
            let merged = existing.copy_with_count(existing.count() + amount_to_add);
            if !is_persistable(&merged) {
                return 0;
            }
            self.items.remove(stack_index);
            items_to_add.shrink(amount_to_add);
            self.items.insert(0, merged);
        } else {
            let split = items_to_add.split(amount_to_add);
            if !is_persistable(&split) {
                // Nothing was committed yet, so hand the items straight back.
                items_to_add.grow(amount_to_add);
                return 0;
            }
            self.items.insert(0, split);
        }

        self.weight = new_weight;
        amount_to_add
    }

    /// Pulls what fits out of `slot` and into the bundle.
    ///
    /// Vanilla parity: `BundleContents.Mutable.tryTransfer`.
    pub fn try_transfer(
        &mut self,
        slot: &dyn Slot,
        guard: &mut ContainerLockGuard,
        player: &Player,
    ) -> i32 {
        let other = slot.get_item(guard).clone();
        let Ok(item_weight) = BundleContents::stack_unit_weight(&other) else {
            return 0;
        };
        let max_amount = self.max_amount_to_add(item_weight);
        if !can_item_be_in_bundle(&other) {
            return 0;
        }

        let mut taken = slot.safe_take(guard, other.count(), max_amount, player);
        self.try_insert(&mut taken)
    }

    /// Points the next extraction at `selected_item`, or clears the selection
    /// when it already points there or is out of range.
    ///
    /// Vanilla parity: `BundleContents.Mutable.toggleSelectedItem`.
    pub fn toggle_selected_item(&mut self, selected_item: i32) {
        let toggles_on =
            self.selected_item != selected_item && !self.index_is_outside_bounds(selected_item);
        self.selected_item = if toggles_on {
            selected_item
        } else {
            BundleContents::NO_SELECTED_ITEM_INDEX
        };
    }

    /// Vanilla parity: `BundleContents.Mutable.indexIsOutsideAllowedBounds`.
    fn index_is_outside_bounds(&self, selected_item: i32) -> bool {
        self.in_bounds_index(selected_item).is_none()
    }

    fn in_bounds_index(&self, selected_item: i32) -> Option<usize> {
        usize::try_from(selected_item)
            .ok()
            .filter(|index| *index < self.items.len())
    }

    /// Takes the selected stack out, or the most recently inserted one when
    /// nothing is selected.
    ///
    /// Vanilla parity: `BundleContents.Mutable.removeOne`.
    pub fn remove_one(&mut self) -> Option<ItemStack> {
        if self.items.is_empty() {
            return None;
        }

        let remove_index = self.in_bounds_index(self.selected_item).unwrap_or(0);
        let stack = self.items.remove(remove_index);

        match BundleContents::stack_unit_weight(&stack)
            .and_then(|unit| unit.multiply(stack.count()))
            .and_then(|removed| self.weight.subtract(removed))
        {
            Ok(weight) => self.weight = weight,
            // Unreachable: this stack was weighed on the way in.
            Err(error) => log::error!("Could not subtract removed bundle item weight: {error}"),
        }

        self.toggle_selected_item(BundleContents::NO_SELECTED_ITEM_INDEX);
        Some(stack)
    }

    /// Freezes these contents back into the component.
    ///
    /// Vanilla parity: `BundleContents.Mutable.toImmutable`. Every stack here
    /// either came from a template or was validated by [`Self::try_insert`], so
    /// the filter below never drops anything.
    #[must_use]
    pub fn to_immutable(&self) -> BundleContents {
        let items = self
            .items
            .iter()
            .filter_map(|item| match ItemStackTemplate::from_stack(item) {
                Ok(template) => Some(template),
                Err(error) => {
                    log::error!("Dropping unpersistable bundle item: {error}");
                    None
                }
            })
            .collect();
        BundleContents::with_selected_item(items, self.selected_item)
    }
}

fn is_persistable(stack: &ItemStack) -> bool {
    ItemStackTemplate::from_stack(stack).is_ok()
}
