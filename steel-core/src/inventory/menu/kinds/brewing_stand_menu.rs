//! Brewing stand menu.
//!
//! Three bottle slots, an ingredient slot and a fuel slot, plus the player
//! inventory, and two data slots carrying the brew timer and the fuel gauge to
//! the client.

use std::sync::Arc;

use steel_registry::{potion_brewing, vanilla_items, vanilla_menu_types};

use crate::block_entity::entities::BrewingStandDataSlots;
use crate::inventory::menu::builder::{DataSlot, SectionKind};
use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds the brewing stand menu.
///
/// Vanilla parity: `BrewingStandMenu`.
#[must_use]
pub fn brewing_stand(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    data: Arc<BrewingStandDataSlots>,
) -> Menu {
    let container = container.into();
    let mut builder = MenuBuilder::new(&vanilla_menu_types::BREWING_STAND, container_id);

    // Vanilla parity: `BrewingStandMenu.PotionSlot`, which takes a filled potion
    // of any kind or an empty glass bottle and nothing else.
    let bottles = builder.section_with(
        &container,
        3,
        SectionKind::restricted(|_slot, stack| {
            stack.is(&vanilla_items::POTION)
                || stack.is(&vanilla_items::SPLASH_POTION)
                || stack.is(&vanilla_items::LINGERING_POTION)
                || stack.is(&vanilla_items::GLASS_BOTTLE)
        }),
    );
    let ingredient = builder.section_with(
        &container,
        1,
        SectionKind::restricted(|_slot, stack| potion_brewing::is_ingredient(stack)),
    );
    let fuel = builder.section_with(
        &container,
        1,
        SectionKind::restricted(|_slot, stack| potion_brewing::is_brewing_fuel(stack)),
    );
    let player = builder.player_inventory(&inventory);

    let data_slots = [builder.data_slot(0), builder.data_slot(0)];

    builder.route(
        [bottles, ingredient, fuel],
        player.all(),
        FillDirection::Backward,
    );
    // TODO: mirror BrewingStandMenu.quickMoveStack, which sends a bottle to the
    // bottle row, blaze powder to the fuel slot and anything brewable to the
    // ingredient slot instead of filling in order.
    builder.route(
        player.all(),
        [bottles, ingredient, fuel],
        FillDirection::Forward,
    );

    builder.build(BrewingStandKind {
        container,
        data,
        data_slots,
    })
}

/// Per-menu brewing stand state.
pub struct BrewingStandKind {
    /// The backing container.
    container: ContainerRef,
    /// Progress published by the block entity each tick.
    data: Arc<BrewingStandDataSlots>,
    /// Handles to the two synced values.
    data_slots: [DataSlot; 2],
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for BrewingStandKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/brewing_stand");
}

impl MenuKind for BrewingStandKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }

    /// Pushes the block entity's progress into the synced data slots.
    ///
    /// As with the furnace, vanilla hands the menu the block entity's own
    /// `ContainerData`; Steel republishes it here so the menu never has to take
    /// the block entity's lock.
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
