//! Stonecutter menu.
//!
//! Vanilla parity: `StonecutterMenu`. Two slots and one number: what went in,
//! what comes out, and which of the input's many cuts the player picked. The
//! number is the whole difference from a crafting table -- a block of stone
//! offers a dozen results and nothing but the player's choice separates them.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::item_stack::ItemStack;
use foton_registry::{vanilla_blocks, vanilla_menu_types};
use foton_utils::BlockPos;
use foton_utils::locks::{IntoShared, Shared};

use crate::inventory::container::{ResultContainer, SimpleContainer};
#[cfg(test)]
use crate::inventory::lock::ContainerId;
use crate::inventory::prelude::*;
use crate::inventory::slots::{NO_SELECTION, StonecutterHandler};
use crate::player::player_inventory::PlayerInventory;
use crate::world::LevelReader as _;

/// Builds the stonecutter menu.
#[must_use]
pub fn stonecutter(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    block_pos: BlockPos,
) -> Menu {
    let input_container = SimpleContainer::new(1).into_shared();
    let result_container = ResultContainer::new().into_shared();
    let selected = Arc::new(AtomicI32::new(NO_SELECTION));

    let handler = StonecutterHandler::new(
        input_container.clone(),
        result_container.clone(),
        Arc::clone(&selected),
    );

    let mut builder = MenuBuilder::new(&vanilla_menu_types::STONECUTTER, container_id);
    let input = builder.section_all(&input_container);
    let result = builder.result_slot(handler.clone());
    let player = builder.player_inventory(&inventory);

    // Vanilla parity: the single `selectedRecipeIndex` data slot, which is how
    // the client knows which button to draw pressed.
    let selected_slot = builder.data_slot(i16::try_from(NO_SELECTION).unwrap_or(-1));

    builder.route(result, player.all(), FillDirection::Backward);
    builder.route(input, player.all(), FillDirection::Forward);
    builder.route(player.all(), input, FillDirection::Forward);
    builder.drain(input);

    builder.build(StonecutterKind {
        handler,
        selected,
        selected_slot,
        result,
        block_pos,
        last_input: ItemStack::empty(),
    })
}

/// Per-menu stonecutter state.
pub struct StonecutterKind {
    handler: StonecutterHandler,
    /// The chosen recipe, shared with the handler.
    selected: Arc<AtomicI32>,
    /// The same number, mirrored to the client.
    selected_slot: DataSlot,
    /// The result section, so pickup-all can be kept out of it.
    result: Section,
    block_pos: BlockPos,
    /// What was in the input last time it was looked at.
    ///
    /// Vanilla parity: the `input` field of `StonecutterMenu`, which exists so
    /// that stacking one more block into the slot does not throw away the
    /// choice the player already made.
    last_input: ItemStack,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for StonecutterKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/stonecutter");
}

impl StonecutterKind {
    /// The container the input sits in.
    #[cfg(test)]
    pub(crate) fn input_id(&self) -> ContainerId {
        self.handler.input_id_for_tests()
    }

    /// The container the cut appears in.
    #[cfg(test)]
    pub(crate) fn result_id(&self) -> ContainerId {
        self.handler.result_id_for_tests()
    }

    /// Writes the selection to the client and rebuilds the result.
    fn apply_selection(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        index: i32,
    ) {
        self.selected.store(index, Ordering::Relaxed);
        self.selected_slot
            .set(behavior, i16::try_from(index).unwrap_or(-1));
        self.handler.update_result(guard);
    }
}

impl MenuKind for StonecutterKind {
    /// Vanilla parity: `StonecutterMenu.slotsChanged`, which only resets the
    /// choice when the *kind* of item in the slot changes. Adding a second
    /// block of the same stone keeps the cut the player picked.
    fn slots_changed(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        let input = self
            .handler
            .input_snapshot(guard)
            .unwrap_or_else(ItemStack::empty);

        if ItemStack::is_same_item(&input, &self.last_input) {
            return;
        }
        self.last_input = input;
        self.apply_selection(behavior, guard, NO_SELECTION);
    }

    /// Vanilla parity: `StonecutterMenu.clickMenuButton`. An out-of-range
    /// button is ignored rather than clearing the choice, so a click that races
    /// a slot change cannot wipe what the player picked.
    fn on_button_click(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
        button: i32,
    ) -> bool {
        if self.selected.load(Ordering::Relaxed) == button {
            return false;
        }
        let count = self.handler.recipe_count(guard);
        if button >= 0 && (button as usize) < count {
            self.apply_selection(behavior, guard, button);
        }
        true
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, slot_index: usize) -> bool {
        !self.result.contains(slot_index)
    }

    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let world = player.get_world();
        world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::STONECUTTER
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }
}

#[cfg(test)]
mod tests;
