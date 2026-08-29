//! Crafter menu.
//!
//! Vanilla parity: `CrafterMenu`. A 3x3 grid, the player inventory, and one
//! slot on the right showing what the recipe would make -- a preview only: it
//! takes nothing and gives nothing, because the crafter builds its result on a
//! redstone pulse rather than when a player clicks.
//!
//! The nine grid slots can each be switched off from the client, which is the
//! one thing this menu does that no other does.

use std::array;
use std::sync::Arc;

use foton_registry::REGISTRY;
use foton_registry::item_stack::ItemStack;
use foton_registry::recipe::CraftingInput;
use foton_registry::vanilla_menu_types;
use foton_utils::locks::IntoShared as _;

use crate::block_entity::entities::{
    CRAFTER_DATA_SLOTS, CRAFTER_HEIGHT, CRAFTER_SLOTS, CRAFTER_WIDTH, CrafterContainer,
    CrafterDataSlots,
};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::menu::builder::{DataSlot, SectionKind};
use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds the crafter menu.
#[must_use]
pub fn crafter(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    data: Arc<CrafterDataSlots>,
) -> Menu {
    let container = container.into();
    let preview: ContainerRef = SimpleContainer::new(1).into_shared().into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::CRAFTER_3X3, container_id);
    let grid = builder.section(&container, CRAFTER_SLOTS);
    let player = builder.player_inventory(&inventory);
    // Vanilla parity: `NonInteractiveResultSlot` -- neither half of a click
    // does anything to it.
    let result = builder.section_with(
        preview.clone(),
        1,
        SectionKind::guarded(
            |_slot, _stack| false,
            |_slot, _stack, _guard, _player| false,
        ),
    );

    let data_slots: [DataSlot; CRAFTER_DATA_SLOTS] = array::from_fn(|_| builder.data_slot(0));

    builder.route(grid, player.all(), FillDirection::Backward);
    builder.route(player.all(), grid, FillDirection::Forward);
    // The preview holds nothing a shift-click could move, but leaving it out of
    // the routes keeps it from ever being picked as a target.
    let _ = result;

    builder.build(CrafterKind {
        container,
        preview,
        data,
        data_slots,
    })
}

/// Per-menu crafter state.
pub struct CrafterKind {
    /// The nine grid slots, owned by the block entity.
    container: ContainerRef,
    /// The preview slot's own one-slot container.
    preview: ContainerRef,
    /// Slot states and the redstone flag, shared with the block entity.
    data: Arc<CrafterDataSlots>,
    /// Handles to the ten synced values.
    data_slots: [DataSlot; CRAFTER_DATA_SLOTS],
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for CrafterKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/crafter");
}

impl CrafterKind {
    /// Recomputes what the grid would make.
    ///
    /// Vanilla parity: `CrafterMenu.refreshRecipeResult`.
    ///
    /// Both containers are reached through the guard rather than locked
    /// directly: the guard already holds every container this menu covers, and
    /// the preview is one of them.
    fn refresh_preview(&self, guard: &mut ContainerLockGuard) {
        let Some(grid) = guard.get_typed::<CrafterContainer>(self.container.container_id()) else {
            return;
        };
        let input = CraftingInput::new(CRAFTER_WIDTH, CRAFTER_HEIGHT, grid.items().to_vec());
        let result = REGISTRY
            .recipes
            .find_crafting_recipe(&input)
            .map_or_else(ItemStack::empty, |recipe| recipe.assemble());

        if let Some(preview) = guard.get_mut(self.preview.container_id()) {
            preview.set_item(0, result);
            preview.set_changed();
        }
    }
}

impl MenuKind for CrafterKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }

    fn on_open(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.refresh_preview(guard);
    }

    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.refresh_preview(guard);
    }

    /// Switches one grid slot on or off.
    ///
    /// Vanilla parity: `CrafterMenu.setSlotState`. A slot that holds something
    /// cannot be switched off, which the block entity enforces; the preview is
    /// recomputed either way because a disabled slot is not an empty one as far
    /// as the recipe is concerned.
    fn on_slot_state_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
        slot: usize,
        enabled: bool,
    ) {
        if slot >= CRAFTER_SLOTS {
            return;
        }
        let holds_something = guard
            .get_typed::<CrafterContainer>(self.container.container_id())
            .is_some_and(|grid| !grid.get_item(slot).is_empty());
        if holds_something {
            return;
        }

        self.data.set_slot_disabled(slot, !enabled);
        if let Some(owner) = self.container.owner_block_entity() {
            owner.set_changed();
        }
        self.refresh_preview(guard);
    }

    /// Pushes the slot states and the redstone flag into the synced data slots.
    ///
    /// Vanilla hands the menu the block entity's own `ContainerData`; Foton
    /// republishes it here for the same reason the furnace menu does.
    fn on_tick(
        &mut self,
        behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        let values = self.data.snapshot();
        for (slot, value) in self.data_slots.iter().zip(values) {
            slot.set(behavior, value);
        }
    }
}

#[cfg(test)]
mod tests;
