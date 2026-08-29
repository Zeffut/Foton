//! Village raids.
//!
//! Vanilla parity: the `net.minecraft.world.entity.raid` package.
//!
//! A raid is the only vanilla event that pulls five systems together at once:
//! it lives in the point-of-interest index (a village is an occupied village
//! POI), in saved data (one `Raids` per loaded world), on the boss bar, in the
//! wave spawner and in the mobs themselves. [`Raid`] is the state machine,
//! [`Raids`] is the per-world manager that ticks and persists it, and
//! [`crate::entity::raider`] is the seam the mobs read it through.
//!
//! The two entry points are the Raid Omen effect running out on a player
//! standing in a village -- see [`crate::entity::living_base`] -- and the
//! `/raid start` command.

#[expect(
    clippy::module_inception,
    reason = "the raid module mirrors vanilla's Raid class and groups its implementation"
)]
mod raid;
mod raids;
mod wave;

pub use raid::{DEFAULT_MAX_RAID_OMEN_LEVEL, DEFAULT_PRE_RAID_TICKS, Raid, RaidPhase};
pub use raids::Raids;
pub use wave::{RaiderType, num_groups};

pub(crate) use raid::VALID_RAID_RADIUS_SQR;
pub(crate) use raids::PersistentRaids;

#[cfg(test)]
mod tests;
