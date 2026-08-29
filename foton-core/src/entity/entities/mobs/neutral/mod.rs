//! Neutral entity implementations.
//!
//! Those mobs are peaceful until provoked, and then stay angry at whoever
//! provoked them.

pub mod golem;
mod wolf;

pub use golem::{CopperGolemEntity, IronGolemEntity, SnowGolemEntity};
pub use wolf::WolfEntity;
