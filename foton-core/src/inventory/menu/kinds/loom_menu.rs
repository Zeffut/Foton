//! Loom menu.
//!
//! Vanilla parity: `LoomMenu`. A banner, a dye, an optional pattern item and
//! the result -- but the result is not decided by the inputs alone. The player
//! picks a pattern from the list on the left, which arrives as a button press,
//! and one data slot carries that choice back so the client can draw it.

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_blocks;
use foton_registry::vanilla_menu_types;
use foton_utils::BlockPos;
use foton_utils::locks::IntoShared as _;

use crate::inventory::container::{ResultContainer, SimpleContainer};
use crate::inventory::menu::builder::{DataSlot, SectionKind};
use crate::inventory::prelude::*;
use crate::inventory::slots::{
    LOOM_BANNER, LOOM_DYE, LOOM_PATTERN, LoomHandler, PATTERN_NOT_SET, is_banner, is_dye_item,
    is_pattern_item,
};
use crate::player::player_inventory::PlayerInventory;
use crate::world::LevelReader as _;

/// Slots in the loom's input container.
const LOOM_INPUT_SLOTS: usize = 3;

/// Builds the loom menu.
#[must_use]
pub fn loom(inventory: Shared<PlayerInventory>, container_id: u8, block_pos: BlockPos) -> Menu {
    let input_container = SimpleContainer::new(LOOM_INPUT_SLOTS).into_shared();
    let result_container = ResultContainer::new().into_shared();
    let handler = LoomHandler::new(input_container.clone(), result_container.clone());

    let mut builder = MenuBuilder::new(&vanilla_menu_types::LOOM, container_id);
    let inputs = builder.section_with(
        &input_container,
        LOOM_INPUT_SLOTS,
        SectionKind::restricted(|slot, stack| match slot {
            LOOM_BANNER => is_banner(stack),
            LOOM_DYE => is_dye_item(stack),
            LOOM_PATTERN => is_pattern_item(stack),
            _ => false,
        }),
    );
    let result = builder.result_slot(handler.clone());
    let player = builder.player_inventory(&inventory);
    let selected = builder.data_slot(PATTERN_NOT_SET as i16);

    builder.route([inputs, result], player.all(), FillDirection::Backward);
    builder.route(player.all(), inputs, FillDirection::Forward);
    // Vanilla parity: `LoomMenu.removed`, which hands the inputs back. The
    // result is virtual and simply disappears.
    builder.drain(inputs);

    builder.build(LoomKind {
        handler,
        result,
        selected,
        block_pos,
    })
}

/// Per-menu loom state.
pub struct LoomKind {
    handler: LoomHandler,
    /// The result slot, so pick-all cannot drain it.
    result: Section,
    /// The chosen pattern, mirrored to the client.
    selected: DataSlot,
    block_pos: BlockPos,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for LoomKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/loom");
}

impl LoomKind {
    /// Republishes the chosen pattern after the handler has had its say.
    ///
    /// The handler clamps the selection when the inputs change -- swapping the
    /// pattern item out from under a choice unselects it -- so the data slot is
    /// written from the handler rather than from whatever the player asked for.
    fn publish_selection(&self, behavior: &mut MenuBehavior) {
        self.selected.set(
            behavior,
            i16::try_from(self.handler.selected()).unwrap_or(-1),
        );
    }
}

impl MenuKind for LoomKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let world = player.get_world();
        world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::LOOM
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, slot_index: usize) -> bool {
        !self.result.contains(slot_index)
    }

    fn on_open(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.handler.update_result(guard);
        self.publish_selection(behavior);
    }

    fn slots_changed(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.handler.update_result(guard);
        self.publish_selection(behavior);
    }

    /// Vanilla parity: `LoomMenu.clickMenuButton`, where the button id is an
    /// index into the patterns on offer.
    fn on_button_click(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
        button: i32,
    ) -> bool {
        if button < 0 {
            return false;
        }
        self.handler.select(button);
        self.handler.update_result(guard);
        self.publish_selection(behavior);
        // An index the loom does not offer is refused, which is what the
        // handler says by unselecting it again.
        self.handler.selected() == button
    }
}

#[cfg(test)]
mod tests;
