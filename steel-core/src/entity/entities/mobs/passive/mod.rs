//! Passive entity implementations.
/// Those mobs are passive creatures that run away when attacked by a player.
mod chicken;
mod cow;
mod mooshroom;
mod pig;
mod rabbit;
mod sheep;
mod strider;
mod turtle;

pub use chicken::ChickenEntity;
pub use cow::CowEntity;
pub use mooshroom::MushroomCowEntity;
pub use pig::PigEntity;
pub use rabbit::{RabbitEntity, RabbitVariant};
pub use sheep::SheepEntity;
pub use strider::StriderEntity;
pub use turtle::TurtleEntity;
