//! Block entity implementations.

mod banner;
mod barrel;
mod beacon;
mod beehive;
mod bell;
mod brewing_stand;
mod brushable;
mod campfire;
mod chest;
mod chiseled_bookshelf;
mod command_block;
mod comparator;
mod conduit;
mod copper_golem_statue;
mod crafter;
mod creaking_heart;
mod daylight_detector;
mod decorated_pot;
mod dispenser;
mod end_gateway;
mod end_portal;
mod furnace;
mod hopper;
mod jigsaw;
mod jukebox;
mod lectern;
mod piston_moving;
mod potent_sulfur;
mod raw;
mod sculk_catalyst;
mod sculk_sensor;
mod sculk_shrieker;
mod shelf;
mod shulker_box;
mod sign;
mod skull;
mod spawner;
mod structure_block;
mod trial_spawner;

pub use banner::BannerBlockEntity;
pub use barrel::{BARREL_SLOTS, BarrelBlockEntity};
pub use beacon::{
    BEACON_DATA_SLOTS, BeaconBlockEntity, BeaconDataSlots, LEVELS_NEEDED_FOR_SECONDARY, MAX_LEVELS,
    count_pyramid_levels, decode_effect, effect_duration_ticks, effect_from_holder_id,
    effect_range, encode_effect, required_levels_for, should_apply_this_tick, validate_effects,
};
pub use beehive::{
    BEEHIVE_MAX_OCCUPANTS, BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR,
    BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS, BEEHIVE_MIN_TICKS_BEFORE_REENTERING, BeeReleaseStatus,
    BeehiveBlockEntity,
};
pub use bell::{BellBlockEntity, EVENT_BELL_RING};
pub use brewing_stand::{
    BOTTLE_SLOTS, BREWING_STAND_SLOTS, BrewingStandBlockEntity, BrewingStandDataSlots,
    SLOT_FIRST_BOTTLE, SLOT_INGREDIENT,
};
pub use brushable::BrushableBlockEntity;
pub use campfire::{CAMPFIRE_SLOTS, CampfireBlockEntity};
pub use chest::{CHEST_SLOTS, ChestBlockEntity};
pub use chiseled_bookshelf::{CHISELED_BOOKSHELF_SLOTS, ChiseledBookShelfBlockEntity};
pub use command_block::{CommandBlockEntity, CommandBlockMode, is_command_block};
pub use comparator::ComparatorBlockEntity;
pub use conduit::ConduitBlockEntity;
pub use copper_golem_statue::CopperGolemStatueBlockEntity;
pub use crafter::{
    CRAFTER_DATA_SLOTS, CRAFTER_HEIGHT, CRAFTER_SLOTS, CRAFTER_WIDTH, CrafterBlockEntity,
    CrafterContainer, CrafterDataSlots,
};
pub use creaking_heart::CreakingHeartBlockEntity;
pub use daylight_detector::DaylightDetectorBlockEntity;
pub use decorated_pot::{
    DECORATED_POT_SLOTS, DecoratedPotBlockEntity, EVENT_POT_WOBBLES, WobbleStyle,
};
pub use dispenser::{DISPENSER_SLOTS, DispenserBlockEntity, DispenserContainer};
pub use end_gateway::EndGatewayBlockEntity;
pub use end_portal::EndPortalBlockEntity;
pub use furnace::{
    FURNACE_SLOTS, FurnaceBlockEntity, FurnaceDataSlots, SLOT_FUEL, SLOT_INPUT, SLOT_RESULT,
};
pub use hopper::{
    HOPPER_SLOTS, HopperBlockEntity, HopperContainer, MOVE_ITEM_SPEED, insert_into_containers_at,
};
pub(crate) use hopper::{attached_containers_at, suck_into_at};
pub use jigsaw::{JigsawBlockEntity, JigsawJointType, JigsawSettings, default_joint_type};
pub use jukebox::JukeboxBlockEntity;
pub use lectern::LecternBlockEntity;
pub use piston_moving::PistonMovingBlockEntity;
pub use potent_sulfur::PotentSulfurBlockEntity;
pub use raw::RawBlockEntity;
pub use sculk_catalyst::{CatalystListener, SculkCatalystBlockEntity};
pub use sculk_sensor::SculkSensorBlockEntity;
pub use sculk_shrieker::{SculkShriekerBlockEntity, with_shrieking_player};
pub use shelf::{SHELF_SLOTS, ShelfBlockEntity};
pub use shulker_box::{SHULKER_BOX_SLOTS, ShulkerBoxBlockEntity};
pub use sign::{SIGN_LINES, SignBlockEntity, SignText};
pub use skull::SkullBlockEntity;
pub use spawner::SpawnerBlockEntity;
pub use structure_block::{
    StructureBlockEntity, StructureMirror, StructureRotation,
    mode_from_ordinal as structure_mode_from_ordinal,
};
pub use trial_spawner::TrialSpawnerBlockEntity;
