//! The stonecutter's input and result slots.
//!
//! Vanilla parity: the anonymous result slot of `StonecutterMenu`. What makes
//! it different from a crafting result is that the recipe is not deduced from
//! the input -- one block usually cuts into a dozen things, so the player picks
//! which, and the pick has to survive between the button press and the take.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use foton_registry::REGISTRY;
use foton_registry::item_stack::ItemStack;
use foton_utils::locks::Shared;

use crate::inventory::container::{Container as _, ResultContainer, SimpleContainer};
use crate::inventory::lock::{ContainerId, ContainerLockGuard, ContainerRef};
use crate::inventory::slots::ResultHandler;
use crate::player::Player;

/// The value the selection holds when the player has not chosen.
///
/// Vanilla parity: the `-1` `StonecutterMenu.setupRecipeList` resets to.
pub const NO_SELECTION: i32 = -1;

/// Keeps a stonecutter's result in step with its input and the chosen recipe.
#[derive(Clone)]
pub struct StonecutterHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    /// Which of the input's recipes the player picked.
    ///
    /// Shared with the menu rather than owned by it: the button press that
    /// changes it and the take that reads it arrive as separate packets.
    selected: Arc<AtomicI32>,
}

impl StonecutterHandler {
    /// Creates a handler over the stonecutter's two containers.
    #[must_use]
    pub const fn new(
        input_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
        selected: Arc<AtomicI32>,
    ) -> Self {
        Self {
            input_container,
            result_container,
            selected,
        }
    }

    /// Returns how many recipes the current input offers.
    #[must_use]
    pub fn recipe_count(&self, guard: &ContainerLockGuard) -> usize {
        let Some(input) = self.input(guard) else {
            return 0;
        };
        if input.is_empty() {
            return 0;
        }
        REGISTRY.recipes.stonecutting_recipes_for(&input).len()
    }

    /// Returns a copy of the input stack, for the menu to compare against.
    #[must_use]
    pub fn input_snapshot(&self, guard: &ContainerLockGuard) -> Option<ItemStack> {
        self.input(guard)
    }

    /// Returns a copy of the input stack.
    fn input(&self, guard: &ContainerLockGuard) -> Option<ItemStack> {
        let container = guard.get(self.input_id())?;
        Some(container.get_item(0).clone())
    }

    /// The input container's id, for tests that read it directly.
    #[cfg(test)]
    pub(crate) fn input_id_for_tests(&self) -> ContainerId {
        self.input_id()
    }

    /// The result container's id, for tests that read it directly.
    #[cfg(test)]
    pub(crate) fn result_id_for_tests(&self) -> ContainerId {
        self.result_id()
    }

    fn input_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.input_container)
    }

    fn result_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.result_container)
    }

    /// Returns what the chosen recipe makes from the current input.
    fn chosen_result(&self, guard: &ContainerLockGuard) -> ItemStack {
        let Some(input) = self.input(guard) else {
            return ItemStack::empty();
        };
        if input.is_empty() {
            return ItemStack::empty();
        }

        let index = self.selected.load(Ordering::Relaxed);
        if index < 0 {
            return ItemStack::empty();
        }

        let recipes = REGISTRY.recipes.stonecutting_recipes_for(&input);
        recipes
            .get(index as usize)
            .map_or_else(ItemStack::empty, |recipe| recipe.result.to_item_stack())
    }
}

impl ResultHandler for StonecutterHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, guard: &mut ContainerLockGuard) {
        let result = self.chosen_result(guard);
        let result_id = self.result_id();
        let Some(container) = guard.get_typed_mut::<ResultContainer>(result_id) else {
            return;
        };
        container.set_item(0, result);
        container.set_changed();
    }

    /// Vanilla parity: the `onTake` of the result slot -- one input is spent
    /// per take, and the result is rebuilt so the player can keep taking.
    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        let input_id = self.input_id();
        if let Some(container) = guard.get_mut(input_id) {
            container.get_item_mut(0).shrink(1);
            container.set_changed();
        }
        self.update_result(guard);
        None
    }

    /// Vanilla parity: a stonecutter result is only real while the input still
    /// matches the chosen recipe, which stops a stale result being taken after
    /// the input was swapped underneath it.
    fn is_result_valid(&self, guard: &ContainerLockGuard, _player: &Player) -> bool {
        !self.chosen_result(guard).is_empty()
    }
}
