//! Cartography table menu.
//!
//! Vanilla parity: `CartographyTableMenu`. A map, a material and one result.

use std::sync::Arc;

use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::{sound_events, vanilla_blocks, vanilla_menu_types};
use foton_utils::BlockPos;
use foton_utils::locks::IntoShared as _;

use crate::inventory::container::{ResultContainer, SimpleContainer};
use crate::inventory::menu::builder::SectionKind;
use crate::inventory::prelude::*;
use crate::inventory::slots::{
    CARTOGRAPHY_ADDITIONAL, CARTOGRAPHY_MAP, CartographyHandler, is_cartography_material,
    is_filled_map,
};
use crate::map::storage::MapStorage;
use crate::player::player_inventory::PlayerInventory;
use crate::world::{LevelReader as _, World};

/// Slots in the cartography table's input container.
const CARTOGRAPHY_INPUT_SLOTS: usize = 2;
/// Where the result appears.
const RESULT_SLOT: usize = 2;

/// Builds the cartography table menu.
#[must_use]
pub fn cartography(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    block_pos: BlockPos,
    world: &Arc<World>,
    maps: Arc<MapStorage>,
) -> Menu {
    let input_container = SimpleContainer::new(CARTOGRAPHY_INPUT_SLOTS).into_shared();
    let result_container = ResultContainer::new().into_shared();
    let handler = CartographyHandler::new(input_container.clone(), result_container.clone(), maps);

    let mut builder = MenuBuilder::new(&vanilla_menu_types::CARTOGRAPHY_TABLE, container_id);
    let inputs = builder.section_with(
        &input_container,
        CARTOGRAPHY_INPUT_SLOTS,
        SectionKind::restricted(|slot, stack| match slot {
            CARTOGRAPHY_MAP => is_filled_map(stack),
            CARTOGRAPHY_ADDITIONAL => is_cartography_material(stack),
            _ => false,
        }),
    );
    let result = builder.result_slot(handler.clone());
    let player = builder.player_inventory(&inventory);

    builder.route([inputs, result], player.all(), FillDirection::Backward);
    builder.route(player.all(), inputs, FillDirection::Forward);
    // Vanilla parity: `CartographyTableMenu.removed`, which hands the inputs
    // back. The result is virtual and simply disappears.
    builder.drain(inputs);

    builder.build(CartographyKind {
        handler,
        result,
        block_pos,
        world: Arc::clone(world),
        last_sound_time: -1,
    })
}

/// Per-menu cartography table state.
pub struct CartographyKind {
    handler: CartographyHandler,
    /// The result section, so pick-all cannot drain it.
    result: Section,
    block_pos: BlockPos,
    world: Arc<World>,
    /// Vanilla parity: `CartographyTableMenu.lastSoundTime`, which keeps a
    /// single tick from playing the sound twice.
    last_sound_time: i64,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for CartographyKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/cartography_table");
}

impl MenuKind for CartographyKind {
    fn on_open(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.handler.update_result(guard);
    }

    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.handler.update_result(guard);
    }

    /// Vanilla parity: the sound half of the result slot's `onTake`. Consuming
    /// the inputs is the handler's job; only the sound needs the level.
    fn on_slot_clicked(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        click: Click,
        _player: &Player,
    ) -> ClickOutcome {
        if click.slot().is_none_or(|slot| slot != RESULT_SLOT) {
            return ClickOutcome::Fallthrough;
        }
        if self.handler.result_is_empty(guard) {
            return ClickOutcome::Fallthrough;
        }

        let game_time = self.world.game_time();
        if self.last_sound_time != game_time {
            self.world.play_sound_at(
                &sound_events::UI_CARTOGRAPHY_TABLE_TAKE_RESULT,
                SoundSource::Blocks,
                {
                    let (x, y, z) = self.block_pos.get_center();
                    glam::DVec3::new(x, y, z)
                },
                1.0,
                1.0,
                None,
            );
            self.last_sound_time = game_time;
        }

        ClickOutcome::Fallthrough
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, slot_index: usize) -> bool {
        !self.result.contains(slot_index)
    }

    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let world = player.get_world();
        world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::CARTOGRAPHY_TABLE
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }
}
