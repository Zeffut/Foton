//! Blocks that hold an unhatched animal: turtle eggs, frogspawn, sniffer eggs.
//!
//! Vanilla keeps all three in `net.minecraft.world.level.block` with no shared
//! superclass; what they share is a hatch countdown that ends in a mob spawn.

mod frogspawn_block;
mod sniffer_egg_block;
mod turtle_egg_block;

pub use frogspawn_block::FrogspawnBlock;
pub use sniffer_egg_block::SnifferEggBlock;
pub use turtle_egg_block::TurtleEggBlock;
