//! Thrown bottle o' enchanting.
//!
//! Vanilla parity: `ThrownExperienceBottle`. It falls faster than a snowball
//! and breaks into experience wherever it lands, which is the only reason it
//! exists.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_entity_data::ExperienceBottleEntityData;
use steel_registry::{level_events, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use crate::entity::entities::ExperienceOrbEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileHit, RemovalReason, ThrowableItemProjectile, ThrowableProjectile,
};
use crate::world::World;

/// Vanilla parity: `ThrownExperienceBottle.getDefaultGravity`, heavier than the
/// throwables around it, which is why a bottle drops so much sooner.
const BOTTLE_GRAVITY: f64 = 0.07;

/// The experience a bottle is worth.
///
/// Vanilla parity: the `3 + random.nextInt(5) + random.nextInt(5)` of
/// `ThrownExperienceBottle.onHit`. Two rolls rather than one wide roll, so the
/// middle is far likelier than either end.
const BASE_EXPERIENCE: i32 = 3;
const EXPERIENCE_ROLL: i32 = 5;

/// Vanilla parity: the `-13083194` color the splash particles are tinted.
const SPLASH_COLOR: i32 = -13_083_194;

/// A thrown bottle o' enchanting.
#[entity_behavior(class = "ThrownExperienceBottle")]
pub struct ExperienceBottleEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<ExperienceBottleEntityData>,
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies the entity.
unsafe impl DowncastType for ExperienceBottleEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/experience_bottle");
}

impl ExperienceBottleEntity {
    /// Creates a thrown bottle.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(ExperienceBottleEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a thrown bottle from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(ExperienceBottleEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }
}

impl Entity for ExperienceBottleEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.throwable_projectile_tick();
    }

    fn get_default_gravity(&self) -> f64 {
        BOTTLE_GRAVITY
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, _nbt: &mut NbtCompound) {}

    fn load_additional(&self, _nbt: BorrowedNbtCompoundView<'_, '_>) {}
}

impl Projectile for ExperienceBottleEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `ThrownExperienceBottle.onHit`.
    ///
    /// Vanilla throws the orbs away from whatever the bottle hit; Steel has
    /// only the undirected `award`, so they spill straight out of the impact
    /// point instead of bouncing off the surface.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        if self.is_removed() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let position = self.position();
        world.level_event(
            level_events::PARTICLES_INSTANT_POTION_SPLASH,
            BlockPos::from(position),
            SPLASH_COLOR,
            None,
        );

        let experience = BASE_EXPERIENCE
            + (rand::random::<u32>() % EXPERIENCE_ROLL as u32) as i32
            + (rand::random::<u32>() % EXPERIENCE_ROLL as u32) as i32;
        ExperienceOrbEntity::award(&world, position, experience);

        self.set_removed(RemovalReason::Discarded);
    }
}

impl ThrowableProjectile for ExperienceBottleEntity {}

impl ThrowableItemProjectile for ExperienceBottleEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::EXPERIENCE_BOTTLE
    }

    fn set_item(&self, item: ItemStack) {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .set(item);
    }

    fn get_item(&self) -> ItemStack {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .get()
            .clone()
    }
}
