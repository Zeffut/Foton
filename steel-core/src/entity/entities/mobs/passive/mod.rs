//! Passive entity implementations.
/// Those mobs are passive creatures that run away when attacked by a player.
mod cat;
mod chicken;
mod cow;
mod mooshroom;
mod ocelot;
mod parrot;
mod pig;
mod sheep;
mod strider;

pub use cat::CatEntity;
pub use chicken::ChickenEntity;
pub use cow::CowEntity;
pub use mooshroom::MushroomCowEntity;
pub use ocelot::OcelotEntity;
pub use parrot::{ParrotEntity, ParrotVariant};
pub use pig::PigEntity;
pub use sheep::SheepEntity;
pub use strider::StriderEntity;
