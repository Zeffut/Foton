//! Vanilla menu kind implementations.

mod anvil_menu;
mod basic_menu;
mod beacon_menu;
mod brewing_stand_menu;
mod cartography_menu;
mod chest_menu;
mod crafter_menu;
mod crafting_menu;
mod dispenser_menu;
mod enchantment_menu;
mod furnace_menu;
mod grindstone_menu;
mod hopper_menu;
mod inventory_menu;
mod lectern_menu;
mod loom_menu;
mod merchant_menu;
mod mount_menu;
mod smithing_menu;
mod stonecutter_menu;

pub use anvil_menu::{AnvilKind, anvil};
pub use basic_menu::BasicKind;
pub use beacon_menu::{BeaconKind, beacon};
pub use brewing_stand_menu::{BrewingStandKind, brewing_stand};
pub use cartography_menu::{CartographyKind, cartography};
pub use chest_menu::{ChestKind, chest, double_chest};
pub use crafter_menu::{CrafterKind, crafter};
pub use crafting_menu::{CraftingKind, crafting};
pub use dispenser_menu::{DispenserKind, dispenser};
pub use enchantment_menu::{EnchantmentKind, enchantment};
pub use furnace_menu::{FurnaceKind, furnace};
pub use grindstone_menu::{GrindstoneKind, grindstone};
pub use hopper_menu::{HopperKind, hopper};
pub use inventory_menu::{INVENTORY_MENU_CONTAINER_ID, InventoryKind, inventory_menu};
pub use lectern_menu::{LecternKind, lectern};
pub use loom_menu::{LoomKind, loom};
pub use merchant_menu::{MerchantKind, merchant_menu};
pub use mount_menu::{
    MountInventoryProbe, MountKind, horse_inventory, nautilus_inventory, open_mount_screen,
};
pub use smithing_menu::{SmithingKind, smithing};
pub use stonecutter_menu::{StonecutterKind, stonecutter};
