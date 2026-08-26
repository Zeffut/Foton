//! Boss mob implementations.

pub mod ender_dragon;
mod wither;

pub use ender_dragon::{
    DragonPartIndex, EnderDragon, EnderDragonPart, EnderDragonPhase, EnderDragonPhaseManager,
};
pub use wither::{INVULNERABLE_TICKS, WitherBoss, can_destroy};
