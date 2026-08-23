//! Vehicle entity implementations.

mod boat;
mod boat_common;
mod chest_boat;
mod chest_minecart;
mod minecart;
mod minecart_common;

pub use boat::{BoatEntity, RaftEntity};
pub use chest_boat::{ChestBoatEntity, ChestRaftEntity};
pub use chest_minecart::ChestMinecartEntity;
pub use minecart::MinecartEntity;
