//! Water mob implementations.
//!
//! Vanilla parity: `WaterAnimal` and its subclasses. These navigate by swimming
//! rather than walking, and drown in air instead of in water.

mod cod;
mod fish;
mod salmon;
mod squid;

pub use cod::CodEntity;
pub use salmon::SalmonEntity;
pub use squid::SquidEntity;
