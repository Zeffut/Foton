//! Furnace, smoker and blast furnace menu.
//!
//! Three container slots (input, fuel, result) plus the player inventory, and
//! four data slots carrying the burn and cooking progress to the client.

use std::sync::Arc;

use steel_registry::fuel;
use steel_registry::menu_type::MenuTypeRef;
use steel_registry::vanilla_items;

use crate::block_entity::entities::FurnaceDataSlots;
use crate::inventory::menu::builder::{DataSlot, SectionKind};
use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a furnace-style menu for `menu_type`.
///
/// Vanilla parity: `AbstractFurnaceMenu`. The same layout backs the furnace, the
/// smoker and the blast furnace; only the menu type and the recipe family differ.
#[must_use]
pub fn furnace(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    menu_type: MenuTypeRef,
    data: Arc<FurnaceDataSlots>,
) -> Menu {
    let container = container.into();
    let mut builder = MenuBuilder::new(menu_type, container_id);

    let input = builder.section(&container, 1);
    // Vanilla also refuses a second empty bucket while one already sits here; the
    // rule cannot see the slot's current contents from this predicate, so that
    // corner case is left to Container::can_place_item.
    let fuel = builder.section_with(
        &container,
        1,
        SectionKind::restricted(|_slot, stack| {
            fuel::is_fuel(stack) || stack.is(&vanilla_items::BUCKET)
        }),
    );
    let result = builder.section_with(&container, 1, SectionKind::take_only());
    let player = builder.player_inventory(&inventory);

    let data_slots = [
        builder.data_slot(0),
        builder.data_slot(0),
        builder.data_slot(0),
        builder.data_slot(0),
    ];

    builder.route([input, fuel, result], player.all(), FillDirection::Backward);
    // TODO: mirror AbstractFurnaceMenu.quickMoveStack, which sends a smeltable item
    // to the input slot and a fuel item to the fuel slot instead of filling in order.
    builder.route(player.all(), [input, fuel], FillDirection::Forward);

    builder.build(FurnaceKind {
        container,
        data,
        data_slots,
    })
}

/// Per-menu furnace state.
pub struct FurnaceKind {
    /// The backing container.
    container: ContainerRef,
    /// Progress published by the block entity each tick.
    data: Arc<FurnaceDataSlots>,
    /// Handles to the four synced values.
    data_slots: [DataSlot; 4],
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for FurnaceKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/furnace");
}

impl MenuKind for FurnaceKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }

    /// Pushes the block entity's progress into the synced data slots.
    ///
    /// Vanilla hands the menu the block entity's own `ContainerData`, so the two
    /// are the same object. Steel keeps the cooking state behind the block
    /// entity's lock and republishes it here, which is why the menu polls.
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
