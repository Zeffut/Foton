//! Decoration entities: the ones a player places to be looked at.
//!
//! They are not in `objects` because that module is for non-living entities,
//! and an armor stand is a `LivingEntity` with no AI -- the only one in Steel
//! that is neither a mob nor a player.

mod armor_stand;

pub use armor_stand::ArmorStandEntity;
