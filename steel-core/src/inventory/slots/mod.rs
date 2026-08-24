//! All the different types of Slots

mod anvil_slots;
mod armor_slot;
mod cartography_slots;
mod crafting_slots;
mod furnace_slots;
mod grindstone_slots;
mod loom_slots;
mod normal_slot;
mod restricted_slot;
mod result_handler;
mod result_slot;
pub mod slot;
mod smithing_slots;
mod stonecutter_slots;

pub use anvil_slots::*;
pub use armor_slot::ArmorSlot;
pub use cartography_slots::{
    CARTOGRAPHY_ADDITIONAL, CARTOGRAPHY_MAP, CartographyHandler, is_cartography_material,
    is_filled_map,
};
pub use crafting_slots::CraftingHandler;
pub use furnace_slots::FurnaceResultSlot;
pub use grindstone_slots::{
    GRINDSTONE_ADDITIONAL, GRINDSTONE_INPUT, GrindstoneHandler, grindstone_accepts,
    grindstone_result,
};
pub use loom_slots::{
    LOOM_BANNER, LOOM_DYE, LOOM_PATTERN, LoomHandler, PATTERN_NOT_SET, is_banner, is_dye_item,
    is_pattern_item, selectable_patterns,
};
pub use normal_slot::NormalSlot;
pub use restricted_slot::*;
pub use result_handler::ResultHandler;
pub use result_slot::*;
pub use slot::*;
pub use smithing_slots::{
    SMITHING_ADDITION, SMITHING_BASE, SMITHING_TEMPLATE, SmithingHandler, smithing_result,
};
pub use stonecutter_slots::{NO_SELECTION, StonecutterHandler};
