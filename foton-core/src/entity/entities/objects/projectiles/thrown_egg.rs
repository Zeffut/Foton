//! Thrown egg.
//!
//! Vanilla parity: `ThrownEgg`. It hurts nothing -- zero damage, the same as a
//! snowball against anything that is not a blaze -- and one throw in eight
//! hatches, which is the only reason anybody throws one.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::items::ItemRef;
use foton_registry::vanilla_entity_data::EggEntityData;
use foton_registry::{vanilla_damage_types, vanilla_entities, vanilla_items};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::damage::DamageSource;
use crate::entity::entities::ChickenEntity;
use crate::entity::{
    AgeableMob, Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileHit, RemovalReason, SharedEntity, ThrowableItemProjectile, ThrowableProjectile,
    next_entity_id,
};
use crate::world::World;

/// One throw in this many hatches.
///
/// Vanilla parity: the `random.nextInt(8) == 0` of `ThrownEgg.onHit`.
const HATCH_CHANCE: u32 = 8;

/// And one hatch in this many gives four chicks instead of one.
const QUADRUPLE_CHANCE: u32 = 32;

/// How many chicks a lucky egg gives.
const QUADRUPLE_COUNT: u32 = 4;

/// How old a newly hatched chick is, in ticks before adulthood.
///
/// Vanilla parity: the `setAge(-24000)` of `ThrownEgg.onHit`.
const CHICK_AGE: i32 = -24_000;

/// A thrown egg.
#[entity_behavior(class = "ThrownEgg")]
pub struct ThrownEggEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<EggEntityData>,
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ThrownEggEntity`.
unsafe impl DowncastType for ThrownEggEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/thrown_egg");
}

impl ThrownEggEntity {
    /// Creates a thrown egg.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(EggEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a thrown egg from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(EggEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Hatches, sometimes.
    ///
    /// Vanilla parity: the hatch branch of `ThrownEgg.onHit`. The chick is put
    /// exactly where the egg broke; vanilla nudges it out of a wall first,
    /// which Foton has no equivalent for, so an egg thrown into a corner can
    /// hatch a chick inside the block.
    fn try_hatch(&self, world: &Arc<World>) {
        if !rand::random::<u32>().is_multiple_of(HATCH_CHANCE) {
            return;
        }

        let count = if rand::random::<u32>().is_multiple_of(QUADRUPLE_CHANCE) {
            QUADRUPLE_COUNT
        } else {
            1
        };
        let position = self.position();
        let (yaw, _) = self.rotation();

        for _ in 0..count {
            let chick = Arc::new(ChickenEntity::new(
                &vanilla_entities::CHICKEN,
                next_entity_id(),
                position,
                Arc::downgrade(world),
            ));
            chick.set_rotation((yaw, 0.0));
            AgeableMob::set_age(chick.as_ref(), CHICK_AGE);

            let chick: SharedEntity = chick;
            if world.try_add_entity(chick).is_err() {
                break;
            }
        }
    }
}

impl Entity for ThrownEggEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.throwable_projectile_tick();
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, _nbt: &mut NbtCompound) {}

    fn load_additional(&self, _nbt: BorrowedNbtCompoundView<'_, '_>) {}
}

impl Projectile for ThrownEggEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `ThrownEgg.onHitEntity`, zero damage. The hit still
    /// registers, which is what makes an egg knock a target block or anger a
    /// mob without hurting it.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let mut damage =
            DamageSource::environment(&vanilla_damage_types::THROWN).with_direct_entity(self.id());
        if let Some(owner) = self.get_owner() {
            damage = damage.with_causing_entity(owner.id());
        }
        if let Some(world) = entity.level() {
            entity.hurt(&world, &damage, 0.0);
        }
    }

    /// Vanilla parity: `ThrownEgg.onHit`.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        if self.is_removed() {
            return;
        }
        if let Some(world) = self.level() {
            self.try_hatch(&world);
        }
        self.broadcast_entity_event(EntityStatus::Death);
        self.set_removed(RemovalReason::Discarded);
    }
}

impl ThrowableProjectile for ThrownEggEntity {}

impl ThrowableItemProjectile for ThrownEggEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::EGG
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
