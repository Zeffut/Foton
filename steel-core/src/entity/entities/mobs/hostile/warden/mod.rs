//! The warden, its anger bookkeeping and the brain that acts on it.

mod anger;
mod behaviors;
mod entity;
mod spawn_tracker;
#[cfg(test)]
mod tests;
mod warden_ai;

pub use anger::{AngerLevel, AngerManagement};
pub use entity::WardenEntity;
pub use spawn_tracker::{MAX_WARNING_LEVEL, WardenSpawnTracker, try_warn};
