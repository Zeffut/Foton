//! Smithing table menu.
//!
//! Vanilla parity: `SmithingMenu`. Three slots and a result, and no choices to
//! make -- unlike a stonecutter, at most one smithing recipe can match what is
//! laid out, so there is nothing for the player to pick.

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::item_stack::ItemStack;
use foton_registry::{vanilla_blocks, vanilla_menu_types};
use foton_utils::BlockPos;
use foton_utils::locks::{IntoShared, Shared};

use crate::inventory::container::{ResultContainer, SimpleContainer};
use crate::inventory::prelude::*;
use crate::inventory::slots::SmithingHandler;
use crate::player::player_inventory::PlayerInventory;
use crate::world::LevelReader as _;

/// How many slots the table itself owns.
const SMITHING_INPUTS: usize = 3;

/// Builds the smithing table menu.
#[must_use]
pub fn smithing(inventory: Shared<PlayerInventory>, container_id: u8, block_pos: BlockPos) -> Menu {
    let input_container = SimpleContainer::new(SMITHING_INPUTS).into_shared();
    let result_container = ResultContainer::new().into_shared();
    let handler = SmithingHandler::new(input_container.clone(), result_container.clone());

    let mut builder = MenuBuilder::new(&vanilla_menu_types::SMITHING, container_id);
    let inputs = builder.section_all(&input_container);
    let result = builder.result_slot(handler.clone());
    let player = builder.player_inventory(&inventory);

    builder.route(result, player.all(), FillDirection::Backward);
    builder.route(inputs, player.all(), FillDirection::Forward);
    builder.route(player.all(), inputs, FillDirection::Forward);
    builder.drain(inputs);

    builder.build(SmithingKind {
        handler,
        result,
        block_pos,
    })
}

/// Per-menu smithing table state.
pub struct SmithingKind {
    handler: SmithingHandler,
    /// The result section, so pickup-all can be kept out of it.
    result: Section,
    block_pos: BlockPos,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for SmithingKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/smithing");
}

impl MenuKind for SmithingKind {
    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.handler.update_result(guard);
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, slot_index: usize) -> bool {
        !self.result.contains(slot_index)
    }

    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let world = player.get_world();
        world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::SMITHING_TABLE
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }
}
