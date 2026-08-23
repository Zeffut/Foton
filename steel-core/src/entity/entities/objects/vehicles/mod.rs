//! Vehicle entity implementations.

mod boat;
mod boat_common;
mod chest_boat;
mod chest_minecart;
mod furnace_minecart;
mod minecart;
mod minecart_common;
mod tnt_minecart;

pub use boat::{BoatEntity, RaftEntity};
pub use chest_boat::{ChestBoatEntity, ChestRaftEntity};
pub use chest_minecart::ChestMinecartEntity;
pub use furnace_minecart::FurnaceMinecartEntity;
pub use minecart::MinecartEntity;
pub use tnt_minecart::TntMinecartEntity;
