//! The behaviors a raid drives on the village it besieges.
//!
//! Vanilla parity: the raid half of `net.minecraft.world.entity.ai.behavior` --
//! `SetRaidStatus`, `ResetRaidStatus`, `ReactToBell`, `RingBell`,
//! `LocateHidingPlace`, `SetHiddenState` and `MoveToSkySeeingSpot`.
//!
//! Vanilla types all seven on `LivingEntity` rather than on `Villager`, and
//! none of them reads anything only a villager has, so they sit with the rest
//! of the framework; the activity packages that schedule them are the
//! villager's own. `CelebrateVillagersSurvivedRaid` is the exception -- vanilla
//! types that one on `Villager` too -- and lives beside those packages instead.
//!
//! # How a village learns it is under attack
//!
//! [`SetRaidStatus`] runs in the core package, so it runs whatever the villager
//! is doing: once a raid covers the villager's block it forces `PRE_RAID` while
//! the countdown runs and RAID once the first wave is on the ground, and makes
//! that the *default* activity so the schedule cannot pull the villager back to
//! bed. [`ResetRaidStatus`] sits at the bottom of both of those packages and
//! hands the day back when the raid stops or the village loses.
//!
//! The bell is the other way in. [`ReactToBell`] reads the `HEARD_BELL_TIME` a
//! rung bell writes on everything nearby and sends a villager that is *not*
//! already in a raid into HIDE; [`RingBell`] is how a villager that has been
//! herded to the meeting point by `PRE_RAID` rings it in the first place.

mod bell;
mod hide;
mod raid_status;
mod sky;

pub use bell::{ReactToBell, RingBell};
pub use hide::{LocateHidingPlace, SetHiddenState};
pub use raid_status::{ResetRaidStatus, SetRaidStatus};
pub use sky::{MoveToSkySeeingSpot, has_no_blocks_above};
