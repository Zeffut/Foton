//! Registry-independent animal used by isolated JNI lifecycle tests.

use std::sync::Weak;

use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_entities;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;

use super::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, LivingEntity,
    LivingEntityBase, Mob, MobBase,
};

/// A live animal entity that can exist before the vanilla registry is published.
///
/// This fixture exists only behind `test-support`. JNI lifecycle tests use its
/// UUID and animal trait identity; methods requiring full living state are not
/// part of that prepublication path.
pub struct PrepublicationTestAnimal {
    base: EntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    mob_flags: SyncMutex<i8>,
    health: SyncMutex<f32>,
    age_locked: SyncMutex<bool>,
    baby: SyncMutex<bool>,
}

impl PrepublicationTestAnimal {
    /// Creates a live animal without touching `REGISTRY`.
    #[must_use]
    pub fn new(id: i32) -> Self {
        Self {
            base: EntityBase::new(
                id,
                DVec3::ZERO,
                vanilla_entities::COW.dimensions,
                Weak::new(),
            ),
            mob_base: MobBase::new(),
            ageable_base: AgeableMobBase::new(),
            animal_base: AnimalBase::new(),
            mob_flags: SyncMutex::new(0),
            health: SyncMutex::new(10.0),
            age_locked: SyncMutex::new(false),
            baby: SyncMutex::new(false),
        }
    }
}

// SAFETY: This Foton-owned key uniquely identifies the test-support fixture.
unsafe impl DowncastType for PrepublicationTestAnimal {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:test/prepublication_animal");
}

impl Entity for PrepublicationTestAnimal {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        &vanilla_entities::COW
    }
}

impl LivingEntity for PrepublicationTestAnimal {
    #[expect(
        clippy::unreachable,
        reason = "the isolated lifecycle test needs animal identity but never living state"
    )]
    fn living_base(&self) -> &LivingEntityBase {
        unreachable!("the prepublication JNI path must not inspect living state")
    }

    fn get_health(&self) -> f32 {
        *self.health.lock()
    }

    fn set_health(&self, health: f32) {
        *self.health.lock() = health;
    }
}

impl Mob for PrepublicationTestAnimal {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.mob_flags.lock()
    }

    fn set_mob_flags(&self, flags: i8) {
        *self.mob_flags.lock() = flags;
    }
}

impl AgeableMob for PrepublicationTestAnimal {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.age_locked.lock()
    }

    fn set_age_locked(&self, age_locked: bool) {
        *self.age_locked.lock() = age_locked;
    }

    fn set_synced_baby(&self, baby: bool) {
        *self.baby.lock() = baby;
    }
}

impl Animal for PrepublicationTestAnimal {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }
}
