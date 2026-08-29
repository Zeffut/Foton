//! Dispenser and dropper menu.
//!
//! Vanilla parity: `DispenserMenu`. Three rows of three, then the player
//! inventory. Layout:
//! - Slots 0 to 8: the dispenser
//! - Slots 9 to 35: main inventory (27)
//! - Slots 36 to 44: hotbar (9)

use foton_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Slots in a dispenser or dropper.
///
/// Vanilla parity: `DispenserMenu.CONTAINER_SIZE`.
const DISPENSER_SLOTS: usize = 9;

/// Builds the dispenser menu, shared by the dropper.
#[must_use]
pub fn dispenser(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::GENERIC_3X3, container_id);
    let dispenser = builder.section(&container, DISPENSER_SLOTS);
    let player = builder.player_inventory(&inventory);

    builder.route(dispenser, player.all(), FillDirection::Backward);
    builder.route(player.all(), dispenser, FillDirection::Forward);

    builder.build(DispenserKind { container })
}

/// Per-menu dispenser state: the backing container for the validity check.
pub struct DispenserKind {
    container: ContainerRef,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for DispenserKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/dispenser");
}

impl MenuKind for DispenserKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}

#[cfg(test)]
mod tests {
    use foton_utils::locks::IntoShared as _;

    use super::*;
    use crate::inventory::container::SimpleContainer;

    #[test]
    fn dispenser_menu_exposes_nine_slots_plus_the_player_inventory() {
        let inventory = PlayerInventory::new().into_shared();
        let container = SimpleContainer::new(DISPENSER_SLOTS).into_shared();

        let menu = dispenser(inventory, 1, container);

        assert_eq!(menu.behavior().slot_count(), DISPENSER_SLOTS + 36);
    }
}
