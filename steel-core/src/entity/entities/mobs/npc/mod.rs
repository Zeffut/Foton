//! Villagers and the mobs that trade like them.
//!
//! Vanilla parity: `net.minecraft.world.entity.npc`.
//!
//! # What a Steel villager does, and what it does not
//!
//! It has a biome variant, a profession and a level; it claims a workstation
//! and takes its profession from it; it claims a bed; it rolls the trades its
//! profession and level call for out of the `villager_trade` data registry;
//! it opens the trading screen, banks experience, levels up, restocks twice a
//! day, and remembers who cured it, traded with it or hit it. All of that is
//! ported from `Villager` and `AbstractVillager`.
//!
//! What it does not have is a **day**. Vanilla drives a villager from a
//! `Brain` with a `Schedule`, which switches it between the CORE, WORK, MEET,
//! REST, IDLE, PLAY, PANIC, PRE-RAID, RAID and HIDE activities as the clock
//! turns, and each of those is a package of behaviors -- roughly two dozen of
//! them -- built by `VillagerGoalPackages`. Steel's `Brain` has the machinery
//! but not those packages, and the schedule itself hangs off an
//! `EnvironmentAttribute<Activity>` that Steel does not model at all. So a
//! Steel villager does not walk to its workstation to work, does not go to bed
//! at dusk, does not gather at the bell, does not flee a raid, and does not
//! breed on its own: `Villager.canBreed` and `getBreedOffspring` are ported and
//! correct, but nothing calls them, because the `VillagerMakeLove` behavior
//! that would is part of the missing MEET package.
//!
//! Two consequences worth naming, because they are what a player would notice:
//! a villager claims its workstation from wherever it is standing rather than
//! walking over to it, and a village will not grow on its own.

pub mod merchant_state;
pub mod poi_links;
mod villager;
mod zombie_villager;

pub use merchant_state::MerchantState;
pub use poi_links::{PoiAcquisition, VillagerPoiLinks};
pub use villager::VillagerEntity;
pub use zombie_villager::ZombieVillagerEntity;

/// Saving and loading a merchant's offers, which lives with the offer types.
pub use steel_registry::trading::offer_nbt;
