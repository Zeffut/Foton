//! Thrown snowball.
//!
//! Vanilla parity: `Snowball`. The simplest thing on the
//! `Projectile → ThrowableProjectile → ThrowableItemProjectile` stack: it flies,
//! it hits, it breaks. The only thing it hurts is a blaze, and even then only
//! for three -- against everything else it deals zero damage and exists to
//! knock things about and to trip a target block.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::items::ItemRef;
use foton_registry::vanilla_entity_data::SnowballEntityData;
use foton_registry::{vanilla_damage_types, vanilla_entities, vanilla_items};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileHit, RemovalReason, SharedEntity, ThrowableItemProjectile, ThrowableProjectile,
};
use crate::world::World;

/// What a snowball does to a blaze.
///
/// Vanilla parity: the `entity instanceof Blaze ? 3 : 0` of
/// `Snowball.onHitEntity`.
const BLAZE_DAMAGE: f32 = 3.0;

/// A thrown snowball.
#[entity_behavior(class = "Snowball")]
pub struct SnowballEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<SnowballEntityData>,
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies `SnowballEntity`.
unsafe impl DowncastType for SnowballEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/snowball");
}

impl SnowballEntity {
    /// Creates a snowball.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(SnowballEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a snowball from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(SnowballEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }
}

impl Entity for SnowballEntity {
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

impl Projectile for SnowballEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `Snowball.onHitEntity`.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let mut damage =
            DamageSource::environment(&vanilla_damage_types::THROWN).with_direct_entity(self.id());
        if let Some(owner) = self.get_owner() {
            damage = damage.with_causing_entity(owner.id());
        }

        let amount = if entity.entity_type() == &vanilla_entities::BLAZE {
            BLAZE_DAMAGE
        } else {
            0.0
        };
        if let Some(world) = entity.level() {
            entity.hurt(&world, &damage, amount);
        }
    }

    /// Vanilla parity: `Snowball.onHit`, which breaks the snowball wherever it
    /// landed. The entity event is what puts the puff of particles on screen.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        if self.is_removed() {
            return;
        }
        self.broadcast_entity_event(EntityStatus::Death);
        self.set_removed(RemovalReason::Discarded);
    }
}

impl ThrowableProjectile for SnowballEntity {}

impl ThrowableItemProjectile for SnowballEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::SNOWBALL
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
