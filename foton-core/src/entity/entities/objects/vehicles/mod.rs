//! Vehicle entity implementations.

mod boat;
mod boat_common;
mod chest_boat;
mod chest_minecart;
mod command_block_minecart;
mod furnace_minecart;
mod hopper_minecart;
mod minecart;
mod minecart_common;
mod spawner_minecart;
mod tnt_minecart;

pub use boat::{BoatEntity, RaftEntity};
pub use chest_boat::{ChestBoatEntity, ChestRaftEntity};
pub use chest_minecart::ChestMinecartEntity;
pub use command_block_minecart::MinecartCommandBlockEntity;
pub use furnace_minecart::FurnaceMinecartEntity;
pub use hopper_minecart::HopperMinecartEntity;
pub use minecart::MinecartEntity;
pub use spawner_minecart::SpawnerMinecartEntity;
pub use tnt_minecart::TntMinecartEntity;
