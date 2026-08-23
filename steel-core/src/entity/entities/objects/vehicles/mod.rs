//! Vehicle entity implementations.

mod boat;
mod boat_common;
mod chest_boat;
mod chest_minecart;

pub use boat::{BoatEntity, RaftEntity};
pub use chest_boat::{ChestBoatEntity, ChestRaftEntity};
pub use chest_minecart::ChestMinecartEntity;
