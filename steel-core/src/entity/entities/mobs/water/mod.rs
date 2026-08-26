//! Water mob implementations.
//!
//! Vanilla parity: `WaterAnimal` and its subclasses. These navigate by swimming
//! rather than walking, and drown in air instead of in water.

mod cod;
mod dolphin;
mod fish;
mod glow_squid;
mod pufferfish;
mod salmon;
mod squid;
mod squid_common;
mod tadpole;
mod tropical_fish;

pub use cod::CodEntity;
pub use dolphin::DolphinEntity;
pub use glow_squid::GlowSquidEntity;
pub use pufferfish::PufferfishEntity;
pub use salmon::SalmonEntity;
pub use squid::SquidEntity;
pub use tadpole::{
    MAX_TADPOLES_SPAWN_EXCLUSIVE, MIN_TADPOLES_SPAWN, TICKS_TO_BE_FROG, TadpoleEntity,
    spawn_tadpoles_from_frogspawn,
};
pub use tropical_fish::{TropicalFishEntity, TropicalFishPattern, TropicalFishVariant};
