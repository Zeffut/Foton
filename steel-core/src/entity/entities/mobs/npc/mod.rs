//! Villagers and the mobs that trade like them.
//!
//! Vanilla parity: `net.minecraft.world.entity.npc`.
//!
//! # What a Steel villager does
//!
//! It has a biome variant, a profession and a level; it walks to an unclaimed
//! workstation and takes its trade from it; it claims a bed and a meeting
//! point; it rolls the trades its profession and level call for out of the
//! `villager_trade` data registry; it opens the trading screen, banks
//! experience, levels up, restocks twice a day, and remembers who cured it,
//! traded with it or hit it.
//!
//! It also has a **day**: the `villager_schedule` timeline steps it between the
//! IDLE, WORK, MEET, PLAY and REST activities as the clock turns, and it
//! panics when something dangerous comes near. [`villager_ai`] holds those
//! packages and lists, package by package, the behaviors that are still
//! missing -- chiefly farming, breeding, the bell, and everything a raid
//! touches.

pub mod merchant_state;
mod villager;
pub mod villager_ai;
mod wandering_trader;
mod zombie_villager;

pub use merchant_state::MerchantState;
pub use villager::VillagerEntity;
pub use wandering_trader::WanderingTraderEntity;
pub use zombie_villager::ZombieVillagerEntity;

/// Saving and loading a merchant's offers, which lives with the offer types.
pub use steel_registry::trading::offer_nbt;
