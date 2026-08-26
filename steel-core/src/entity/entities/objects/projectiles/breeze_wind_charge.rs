//! A breeze's wind charge.
//!
//! Vanilla parity: `BreezeWindCharge`. The same projectile a player throws by
//! hand, with a burst two and a half times as wide and no extra shove behind
//! it: `WindCharge` passes a `1.22F` knockback multiplier, this one passes the
//! `AbstractWindCharge` calculator whose multiplier is empty and so defaults to
//! one. It also has no deflection grace period -- a breeze fires from a
//! distance, so there is no point-blank throw to protect against.
//!
//! Everything else is [`super::wind_charge_common`].

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::BreezeWindChargeEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use super::wind_charge_common::{AbstractWindCharge, AbstractWindChargeBase};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileHit, RemovalReason, SharedEntity,
};
use crate::world::{ClipHitResult, World};

/// Vanilla parity: `BreezeWindCharge.RADIUS`, the `3.0F` handed to
/// `Level.explode`.
const BURST_RADIUS: f32 = 3.0;

/// Vanilla parity: the empty `knockbackMultiplier` of
/// `AbstractWindCharge.EXPLOSION_DAMAGE_CALCULATOR`, which
/// `SimpleExplosionDamageCalculator.getKnockbackMultiplier` resolves to the
/// `ExplosionDamageCalculator` default of one.
const BURST_KNOCKBACK: f64 = 1.0;

/// A wind charge a breeze fired.
#[entity_behavior(class = "BreezeWindCharge")]
pub struct BreezeWindChargeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<BreezeWindChargeEntityData>,
    projectile_base: ProjectileBase,
    wind_charge_base: AbstractWindChargeBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BreezeWindChargeEntity`.
unsafe impl DowncastType for BreezeWindChargeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/breeze_wind_charge");
}

impl BreezeWindChargeEntity {
    /// Creates a breeze wind charge.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(BreezeWindChargeEntityData::new()),
            projectile_base: ProjectileBase::new(),
            wind_charge_base: AbstractWindChargeBase::new(),
        }
    }

    /// Creates a breeze wind charge from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(BreezeWindChargeEntityData::new()),
            projectile_base: ProjectileBase::new(),
            wind_charge_base: AbstractWindChargeBase::new(),
        }
    }
}

impl Entity for BreezeWindChargeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `AbstractWindCharge.tick`. `BreezeWindCharge` adds
    /// nothing to it.
    fn tick(&self) {
        self.abstract_wind_charge_tick();
    }

    /// Vanilla parity: `AbstractWindCharge.canCollideWith`.
    fn can_collide_with(&self, other: &dyn Entity) -> bool {
        self.wind_charge_can_collide_with(other)
    }

    /// Vanilla parity: `AbstractWindCharge.push`, an empty override.
    fn push_impulse(&self, _impulse: DVec3) {}

    /// Vanilla parity: `AbstractHurtingProjectile.hurtServer`, which refuses
    /// every damage source outright.
    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        false
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn projectile_owner_uuid(&self) -> Option<Uuid> {
        self.owner_uuid()
    }

    fn projectile_owner(&self) -> Option<SharedEntity> {
        self.get_owner()
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_wind_charge(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_wind_charge(nbt);
    }
}

impl Projectile for BreezeWindChargeEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `AbstractWindCharge.canHitEntity`.
    fn can_hit_entity(&self, entity: &dyn Entity) -> bool {
        self.wind_charge_can_hit_entity(entity)
    }

    /// Vanilla parity: `AbstractHurtingProjectile.onDeflection`.
    fn on_deflection(&self, by_attack: bool) {
        self.wind_charge_on_deflection(by_attack);
    }

    /// Vanilla parity: `AbstractWindCharge.onHitEntity`.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        self.wind_charge_on_hit_entity(entity);
    }

    /// Vanilla parity: `AbstractWindCharge.onHitBlock`.
    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.wind_charge_on_hit_block(hit);
    }

    /// Vanilla parity: `AbstractWindCharge.onHit`.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        self.set_removed(RemovalReason::Discarded);
    }
}

impl AbstractWindCharge for BreezeWindChargeEntity {
    fn wind_charge_base(&self) -> &AbstractWindChargeBase {
        &self.wind_charge_base
    }

    fn burst_radius(&self) -> f32 {
        BURST_RADIUS
    }

    fn burst_knockback(&self) -> f64 {
        BURST_KNOCKBACK
    }

    fn burst_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_BREEZE_WIND_BURST
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use steel_registry::{init_vanilla_registry, vanilla_entities};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::{PigEntity, WindChargeEntity};
    use crate::entity::{ProjectileDeflection, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// Vanilla gives the breeze's charge a three-block burst against the
    /// player's 1.2, and takes away the 1.22 knockback multiplier that makes a
    /// hand-thrown charge shove harder than an ordinary blast. Swapping either
    /// of the two would turn a breeze fight into a very different fight.
    #[test]
    fn a_breeze_charge_bursts_wider_and_shoves_no_harder_than_an_ordinary_blast() {
        init_vanilla_registry();
        let breeze_charge = BreezeWindChargeEntity::new(
            &vanilla_entities::BREEZE_WIND_CHARGE,
            next_entity_id(),
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        let thrown = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            next_entity_id(),
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        assert!(breeze_charge.burst_radius() > thrown.burst_radius());
        assert!((breeze_charge.burst_radius() - 3.0).abs() < f32::EPSILON);
        assert!((breeze_charge.burst_knockback() - 1.0).abs() < f64::EPSILON);
        assert!(breeze_charge.burst_knockback() < thrown.burst_knockback());
    }

    /// A breeze standing in its own gust would otherwise shoot its own charges
    /// down, and two breezes in a trial chamber would shoot each other's.
    #[test]
    fn a_breeze_charge_will_not_shoot_a_hand_thrown_charge_out_of_the_air() {
        init_vanilla_registry();
        let breeze_charge = BreezeWindChargeEntity::new(
            &vanilla_entities::BREEZE_WIND_CHARGE,
            next_entity_id(),
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        let thrown = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            next_entity_id(),
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        assert!(!breeze_charge.can_hit_entity(&thrown));
    }

    /// Unlike the hand-thrown charge, a breeze's has no `noDeflectTicks`: a
    /// shield or a sword swing turns it around on the tick it arrives.
    #[test]
    fn a_breeze_charge_can_be_deflected_the_moment_it_is_fired() {
        init_vanilla_registry();
        let charge = BreezeWindChargeEntity::new(
            &vanilla_entities::BREEZE_WIND_CHARGE,
            next_entity_id(),
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        charge.set_velocity(DVec3::new(0.0, 0.0, 1.0));

        assert!(charge.deflect(ProjectileDeflection::Reverse, None, None, None, true));
    }

    /// The burst is the breeze's whole attack: it has to move a bystander and
    /// leave its health alone.
    #[test]
    fn the_burst_shoves_a_bystander_without_hurting_it() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("breeze_wind_charge_burst");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(9.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&pig))
            .expect("pig should attach to the loaded test chunk");
        let health_before = pig
            .as_living_entity()
            .expect("a pig is a living entity")
            .get_health();

        let charge = BreezeWindChargeEntity::new(
            &vanilla_entities::BREEZE_WIND_CHARGE,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        charge.explode(&world, DVec3::new(8.5, 64.0, 8.5));

        let living = pig.as_living_entity().expect("a pig is a living entity");
        assert!(
            (living.get_health() - health_before).abs() < f32::EPSILON,
            "a breeze's burst took health off a bystander"
        );
        assert!(
            pig.velocity().length_squared() > 0.0,
            "a breeze's burst did not move the bystander at all"
        );
    }
}
