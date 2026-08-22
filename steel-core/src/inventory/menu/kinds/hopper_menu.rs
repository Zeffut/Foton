//! Hopper menu.
//!
//! Vanilla parity: `HopperMenu`. Five slots in a row, then the player
//! inventory. Layout:
//! - Slots 0 to 4: the hopper
//! - Slots 5 to 31: main inventory (27)
//! - Slots 32 to 40: hotbar (9)

use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Slots in a hopper.
///
/// Vanilla parity: `HopperMenu.CONTAINER_SIZE`.
const HOPPER_SLOTS: usize = 5;

/// Builds the hopper menu.
#[must_use]
pub fn hopper(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::HOPPER, container_id);
    let hopper = builder.section(&container, HOPPER_SLOTS);
    let player = builder.player_inventory(&inventory);

    builder.route(hopper, player.all(), FillDirection::Backward);
    builder.route(player.all(), hopper, FillDirection::Forward);

    builder.build(HopperKind { container })
}

/// Per-menu hopper state: the backing container for the validity check.
pub struct HopperKind {
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for HopperKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/hopper");
}

impl MenuKind for HopperKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}

#[cfg(test)]
mod tests {
    use steel_utils::locks::IntoShared as _;

    use super::*;
    use crate::inventory::container::SimpleContainer;

    #[test]
    fn hopper_menu_exposes_five_slots_plus_the_player_inventory() {
        let inventory = PlayerInventory::new().into_shared();
        let container = SimpleContainer::new(HOPPER_SLOTS).into_shared();

        let menu = hopper(inventory, 1, container);

        assert_eq!(menu.behavior().slot_count(), HOPPER_SLOTS + 36);
    }
}
