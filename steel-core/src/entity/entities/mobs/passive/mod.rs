//! Passive entity implementations.
/// Those mobs are passive creatures that run away when attacked by a player.
mod cat;
mod chicken;
mod cow;
mod fox;
mod goat;
mod mooshroom;
mod ocelot;
mod parrot;
mod pig;
mod polar_bear;
mod rabbit;
mod sheep;
mod strider;
mod turtle;

pub use cat::CatEntity;
pub use chicken::ChickenEntity;
pub use cow::CowEntity;
pub use fox::{FoxEntity, FoxVariant};
pub use goat::GoatEntity;
pub use mooshroom::MushroomCowEntity;
pub use ocelot::OcelotEntity;
pub use parrot::{ParrotEntity, ParrotVariant};
pub use pig::PigEntity;
pub use polar_bear::PolarBearEntity;
pub use rabbit::{RabbitEntity, RabbitVariant};
pub use sheep::SheepEntity;
pub use strider::StriderEntity;
pub use turtle::TurtleEntity;
