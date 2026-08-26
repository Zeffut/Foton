//! The End, and the fight that runs in it.
//!
//! Vanilla parity: `net.minecraft.world.level.dimension.end`.

pub mod fight;
pub mod respawn_stage;

#[cfg(test)]
mod tests;

pub use fight::EnderDragonFight;
pub use respawn_stage::DragonRespawnStage;
