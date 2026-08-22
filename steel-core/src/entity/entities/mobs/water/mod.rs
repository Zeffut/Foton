//! Water mob implementations.
//!
//! Vanilla parity: `WaterAnimal` and its subclasses. These navigate by swimming
//! rather than walking, and drown in air instead of in water.

mod cod;
mod fish;
mod glow_squid;
mod salmon;
mod squid;
mod squid_common;

pub use cod::CodEntity;
pub use glow_squid::GlowSquidEntity;
pub use salmon::SalmonEntity;
pub use squid::SquidEntity;
