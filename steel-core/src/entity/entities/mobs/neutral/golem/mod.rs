//! The built golems.
//!
//! Vanilla parity: the `net.minecraft.world.entity.animal.golem` package. All
//! three are put together out of blocks by a carved pumpkin rather than spawned
//! by the world, and all three share `AbstractGolem`'s refusal to despawn.

mod copper_golem;
mod iron_golem;
mod snow_golem;

pub use copper_golem::{CopperGolemEntity, CopperGolemState, EQUIPMENT_SLOT_ANTENNA};
pub use iron_golem::IronGolemEntity;
pub use snow_golem::SnowGolemEntity;

/// Ticks between ambient sounds for a golem.
///
/// Vanilla parity: `AbstractGolem.getAmbientSoundInterval`.
pub(super) const AMBIENT_SOUND_INTERVAL: i32 = 120;
