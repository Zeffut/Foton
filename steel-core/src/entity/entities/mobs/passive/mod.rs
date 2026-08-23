//! Passive entity implementations.
/// Those mobs are passive creatures that run away when attacked by a player.
mod chicken;
mod cow;
mod goat;
mod mooshroom;
mod pig;
mod polar_bear;
mod rabbit;
mod sheep;
mod strider;
mod turtle;

pub use chicken::ChickenEntity;
pub use cow::CowEntity;
pub use goat::GoatEntity;
pub use mooshroom::MushroomCowEntity;
pub use pig::PigEntity;
pub use polar_bear::PolarBearEntity;
pub use rabbit::{RabbitEntity, RabbitVariant};
pub use sheep::SheepEntity;
pub use strider::StriderEntity;
pub use turtle::TurtleEntity;
