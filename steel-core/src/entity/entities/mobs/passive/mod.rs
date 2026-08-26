//! Passive entity implementations.
/// Those mobs are passive creatures that run away when attacked by a player.
mod allay;
mod armadillo;
mod axolotl;
mod bee;
mod cat;
mod chicken;
mod cow;
pub mod equine;
mod fox;
mod frog;
mod goat;
mod happy_ghast;
mod mooshroom;
mod ocelot;
mod panda;
mod parrot;
mod pig;
mod polar_bear;
mod rabbit;
mod sheep;
mod sniffer;
mod strider;
mod turtle;

pub use allay::{AllayEntity, spawn_allay};
pub use armadillo::ArmadilloEntity;
pub use axolotl::AxolotlEntity;
pub use bee::BeeEntity;
pub use cat::CatEntity;
pub use chicken::ChickenEntity;
pub use cow::CowEntity;
pub use equine::{
    CamelEntity, CamelHuskEntity, DonkeyEntity, HorseEntity, HorseMarkings, HorseVariant,
    LlamaEntity, MuleEntity, SkeletonHorseEntity, TraderLlamaEntity, ZombieHorseEntity,
};
pub use fox::{FoxEntity, FoxVariant};
pub use frog::FrogEntity;
pub use goat::GoatEntity;
pub use happy_ghast::HappyGhastEntity;
pub use mooshroom::MushroomCowEntity;
pub use ocelot::OcelotEntity;
pub use panda::{PandaEntity, PandaGene};
pub use parrot::{ParrotEntity, ParrotVariant};
pub use pig::PigEntity;
pub use polar_bear::PolarBearEntity;
pub use rabbit::{RabbitEntity, RabbitVariant};
pub use sheep::SheepEntity;
pub use sniffer::{SnifferEntity, hatch_sniffer_from_egg};
pub use strider::StriderEntity;
pub use turtle::TurtleEntity;
