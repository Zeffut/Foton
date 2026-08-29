//! Ambient mob implementations.
//!
//! Vanilla parity: `AmbientCreature`. Mobs that decorate a place without taking
//! part in anything: they cannot be leashed, and they carry no goals.

mod bat;

pub use bat::BatEntity;
