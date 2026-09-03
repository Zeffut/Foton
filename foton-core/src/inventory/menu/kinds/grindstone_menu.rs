//! Grindstone menu.
//!
//! Vanilla parity: `GrindstoneMenu`. Two slots in, one out, and the experience
//! the stripped enchantments were worth thrown back at the player when they
//! take it -- which is the only reason to grind an item rather than throw it
//! away.

use std::sync::Arc;

use glam::DVec3;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::item_stack::ItemStack;
use foton_registry::{level_events, vanilla_blocks, vanilla_menu_types};
use foton_utils::BlockPos;
use foton_utils::locks::{IntoShared, Shared};

use crate::entity::Entity;
use crate::entity::entities::ExperienceOrbEntity;
use crate::event::PrepareGrindstoneEvent;
use crate::inventory::container::{ResultContainer, SimpleContainer};
use crate::inventory::prelude::*;
use crate::inventory::slots::GrindstoneHandler;
use crate::player::player_inventory::PlayerInventory;
use crate::world::{LevelReader as _, World};

/// Where the result appears.
const RESULT_SLOT: usize = 2;

/// Builds the grindstone menu.
#[must_use]
pub fn grindstone(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    block_pos: BlockPos,
    world: &Arc<World>,
) -> Menu {
    let input_container = SimpleContainer::new(2).into_shared();
    let result_container = ResultContainer::new().into_shared();
    let handler = GrindstoneHandler::new(input_container.clone(), result_container.clone());

    let mut builder = MenuBuilder::new(&vanilla_menu_types::GRINDSTONE, container_id);
    let inputs = builder.section_all(&input_container);
    let result = builder.result_slot(handler.clone());
    let player = builder.player_inventory(&inventory);

    builder.route(result, player.all(), FillDirection::Backward);
    builder.route(inputs, player.all(), FillDirection::Forward);
    builder.route(player.all(), inputs, FillDirection::Forward);
    builder.drain(inputs);

    builder.build(GrindstoneKind {
        handler,
        result,
        block_pos,
        world: Arc::clone(world),
    })
}

/// Per-menu grindstone state.
pub struct GrindstoneKind {
    handler: GrindstoneHandler,
    /// The result section, so pickup-all can be kept out of it.
    result: Section,
    block_pos: BlockPos,
    world: Arc<World>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for GrindstoneKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/grindstone");
}

impl MenuKind for GrindstoneKind {
    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.handler.update_result(guard);
        let Some((upper, lower)) = self.handler.input_snapshot(guard) else {
            return;
        };
        let result = self.handler.result_snapshot(guard);
        let mut event = PrepareGrindstoneEvent::new(_player.uuid(), upper, lower, result);
        _player.fire_event(&mut event);
        self.handler.apply_snapshot(
            guard,
            event.upper().clone(),
            event.lower().clone(),
            event.result().clone(),
        );
    }

    /// Vanilla parity: the `onTake` of the result slot. The experience is read
    /// before the inputs are cleared, because clearing them is what the take
    /// costs and there would be nothing left to price afterwards.
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

        let experience = self.handler.experience(guard);
        if experience > 0 {
            let center = DVec3::new(
                f64::from(self.block_pos.x()) + 0.5,
                f64::from(self.block_pos.y()) + 0.5,
                f64::from(self.block_pos.z()) + 0.5,
            );
            ExperienceOrbEntity::award(&self.world, center, experience);
        }
        self.world
            .level_event(level_events::SOUND_GRINDSTONE_USED, self.block_pos, 0, None);

        ClickOutcome::Fallthrough
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, slot_index: usize) -> bool {
        !self.result.contains(slot_index)
    }

    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let world = player.get_world();
        world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::GRINDSTONE
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }
}
