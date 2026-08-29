//! Wind charge.
//!
//! Vanilla parity: `WindCharge` on top of `AbstractWindCharge` and
//! `AbstractHurtingProjectile`. Thrown by hand, it flies dead straight -- no
//! gravity, no drag, no acceleration -- and bursts on the first thing it
//! touches, shoving whatever stands nearby without breaking a block or
//! lighting a fire.
//!
//! Everything it shares with a breeze's charge lives in
//! [`super::wind_charge_common`]; what is here is the hand-thrown charge's own
//! burst and the few ticks after a throw during which nothing can deflect it.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;
use foton_registry::vanilla_entity_data::WindChargeEntityData;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use super::wind_charge_common::{AbstractWindCharge, AbstractWindChargeBase};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileDeflection, ProjectileHit, RemovalReason, SharedEntity,
};
use crate::world::{ClipHitResult, World};

/// How much harder than an ordinary blast a wind charge shoves.
///
/// Vanilla parity: the `1.22F` knockback multiplier of `WindCharge.explode`.
const BURST_KNOCKBACK: f64 = 1.22;

/// Vanilla parity: `WindCharge.RADIUS`, the `1.2F` handed to `Level.explode`.
const BURST_RADIUS: f32 = 1.2;

/// How long after being thrown a wind charge refuses to be deflected.
///
/// Vanilla parity: `WindCharge.noDeflectTicks`, which is what stops a charge
/// thrown point blank from bouncing straight back off its own thrower.
const NO_DEFLECT_TICKS: i32 = 5;

/// A thrown wind charge.
#[entity_behavior(class = "WindCharge")]
pub struct WindChargeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<WindChargeEntityData>,
    projectile_base: ProjectileBase,
    wind_charge_base: AbstractWindChargeBase,
    /// Vanilla parity: `WindCharge.noDeflectTicks`. Not persisted: vanilla
    /// keeps it in a plain field, so a reloaded charge starts the count over.
    no_deflect_ticks: SyncMutex<i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `WindChargeEntity`.
unsafe impl DowncastType for WindChargeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/wind_charge");
}

impl WindChargeEntity {
    /// Creates a wind charge.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(WindChargeEntityData::new()),
            projectile_base: ProjectileBase::new(),
            wind_charge_base: AbstractWindChargeBase::new(),
            no_deflect_ticks: SyncMutex::new(NO_DEFLECT_TICKS),
        }
    }

    /// Creates a wind charge from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(WindChargeEntityData::new()),
            projectile_base: ProjectileBase::new(),
            wind_charge_base: AbstractWindChargeBase::new(),
            no_deflect_ticks: SyncMutex::new(NO_DEFLECT_TICKS),
        }
    }

    /// Returns how many ticks of deflection immunity are left.
    ///
    /// Vanilla parity: `WindCharge.noDeflectTicks`.
    #[must_use]
    pub fn no_deflect_ticks(&self) -> i32 {
        *self.no_deflect_ticks.lock()
    }
}

impl Entity for WindChargeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `WindCharge.tick`, whose own contribution is the
    /// deflection countdown that runs after the flight.
    fn tick(&self) {
        self.abstract_wind_charge_tick();

        let mut no_deflect_ticks = self.no_deflect_ticks.lock();
        if *no_deflect_ticks > 0 {
            *no_deflect_ticks -= 1;
        }
    }

    /// Vanilla parity: `AbstractWindCharge.canCollideWith`.
    fn can_collide_with(&self, other: &dyn Entity) -> bool {
        self.wind_charge_can_collide_with(other)
    }

    /// Vanilla parity: `AbstractWindCharge.push`, an empty override -- nothing
    /// shoves a wind charge off course, not even another charge's burst.
    ///
    /// Foton's explosion assigns knockback with `Entity::set_velocity` rather
    /// than through this hook, so a nearby burst still moves a wind charge that
    /// vanilla would leave alone.
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

impl Projectile for WindChargeEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `AbstractWindCharge.canHitEntity`.
    fn can_hit_entity(&self, entity: &dyn Entity) -> bool {
        self.wind_charge_can_hit_entity(entity)
    }

    /// Vanilla parity: `WindCharge.deflect`, which ignores every deflection for
    /// the first few ticks of flight.
    fn deflect(
        &self,
        deflection: ProjectileDeflection,
        deflecting_entity: Option<&dyn Entity>,
        new_owner_uuid: Option<Uuid>,
        new_owner_entity: Option<&SharedEntity>,
        by_attack: bool,
    ) -> bool {
        if self.no_deflect_ticks() > 0 {
            return false;
        }
        self.projectile_deflect(
            deflection,
            deflecting_entity,
            new_owner_uuid,
            new_owner_entity,
            by_attack,
        )
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

    /// Vanilla parity: `AbstractWindCharge.onHit`, which discards the charge
    /// whatever it ran into. A block hit has already discarded itself by the
    /// time this runs, exactly as in vanilla.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        self.set_removed(RemovalReason::Discarded);
    }
}

impl AbstractWindCharge for WindChargeEntity {
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
        &sound_events::ENTITY_WIND_CHARGE_WIND_BURST
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
    use foton_utils::types::UpdateFlags;
    use foton_utils::{BlockPos, ChunkPos};
    use simdnbt::borrow::read_compound as read_borrowed_compound;

    use super::super::wind_charge_common::{
        DEFLECTED_ACCELERATION_POWER, MAX_Y_OVERSHOOT, block_burst_center, inertia_applied,
        is_wind_charge,
    };
    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::{EndCrystalEntity, PigEntity};
    use crate::entity::next_entity_id;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn charge_at(position: DVec3, world: &Arc<World>) -> Arc<WindChargeEntity> {
        Arc::new(WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ))
    }

    #[test]
    fn a_wind_charge_holds_its_speed_and_heading_for_as_long_as_it_flies() {
        let velocity = DVec3::new(0.3, -0.2, 1.4);

        assert_eq!(inertia_applied(velocity, 0.0), velocity);
    }

    #[test]
    fn a_charge_an_attack_has_deflected_picks_up_speed_along_its_heading() {
        let accelerated = inertia_applied(DVec3::new(0.0, 0.0, 2.0), DEFLECTED_ACCELERATION_POWER);

        assert!((accelerated.z - 2.1).abs() < 1.0e-9);
        assert!(accelerated.x.abs() < 1.0e-12);
        assert!(accelerated.y.abs() < 1.0e-12);
    }

    #[test]
    fn a_burst_against_a_wall_sits_a_quarter_block_out_from_the_face_it_struck() {
        assert_eq!(
            block_burst_center(DVec3::new(4.0, 65.0, 8.0), DVec3::new(-1.0, 0.0, 0.0)),
            DVec3::new(3.75, 65.0, 8.0)
        );
    }

    #[test]
    fn a_charge_in_flight_points_the_way_vanilla_points_it() {
        init_vanilla_registry();
        let charge = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        charge.set_velocity(DVec3::new(0.0, -1.0, 0.0));
        charge.rotate_towards_movement();
        let (_, pitch) = charge.rotation();
        assert!(
            (pitch - 90.0).abs() < 1.0e-4,
            "diving charge should look down"
        );

        // Vanilla's `rotateTowardsMovement` builds yaw from `atan2(z, x) + 90`,
        // which lands half a turn away from the `atan2(x, z)` the rest of the
        // game uses: movement toward positive Z reads as 180, not 0. Ported as
        // it stands, since only a client rendering a round projectile sees it.
        charge.set_velocity(DVec3::new(0.0, 0.0, 1.0));
        charge.rotate_towards_movement();
        let (yaw, _) = charge.rotation();
        assert!((yaw - 180.0).abs() < 1.0e-4);
    }

    #[test]
    fn one_wind_charge_will_not_shoot_another_out_of_the_air() {
        init_vanilla_registry();
        let charge = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        let other = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            2,
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        assert!(!charge.can_hit_entity(&other));
        assert!(is_wind_charge(&other));
    }

    #[test]
    fn a_wind_charge_passes_straight_through_an_end_crystal() {
        init_vanilla_registry();
        let charge = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            2,
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        assert!(
            crystal.is_pickable(),
            "an end crystal is otherwise hittable"
        );
        assert!(!charge.can_hit_entity(&crystal));
    }

    #[test]
    fn a_charge_just_thrown_refuses_every_deflection_until_its_grace_period_runs_out() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("wind_charge_deflection_grace");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let charge = charge_at(DVec3::new(8.5, 100.0, 8.5), &world);
        let shared: SharedEntity = charge.clone();
        world
            .try_add_entity(shared)
            .expect("wind charge should attach to the loaded test chunk");

        assert_eq!(charge.no_deflect_ticks(), NO_DEFLECT_TICKS);
        assert!(!charge.deflect(ProjectileDeflection::Reverse, None, None, None, true));
        assert_eq!(charge.acceleration_power().to_bits(), 0.0f64.to_bits());

        for _ in 0..NO_DEFLECT_TICKS {
            charge.tick();
        }

        assert_eq!(charge.no_deflect_ticks(), 0);
        assert!(charge.deflect(ProjectileDeflection::Reverse, None, None, None, true));
        assert_eq!(
            charge.acceleration_power().to_bits(),
            DEFLECTED_ACCELERATION_POWER.to_bits()
        );
    }

    #[test]
    fn a_burst_against_the_ground_leaves_the_ground_where_it_was() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("wind_charge_keeps_blocks");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let floor = BlockPos::new(8, 64, 8);
        assert!(world.set_block(
            floor,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));

        let charge = charge_at(DVec3::new(8.5, 67.0, 8.5), &world);
        let shared: SharedEntity = charge.clone();
        world
            .try_add_entity(shared)
            .expect("wind charge should attach to the loaded test chunk");
        charge.set_velocity(DVec3::new(0.0, -4.0, 0.0));

        charge.tick();

        assert_eq!(
            world.get_block_state(floor),
            vanilla_blocks::STONE.default_state(),
            "a wind charge burst must not break what it lands on"
        );
        assert!(charge.is_removed(), "the charge is spent by its own burst");
    }

    #[test]
    fn a_direct_hit_hurts_what_it_struck_and_shoves_it_away() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("wind_charge_direct_hit");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(9.0, 64.0, 8.0),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&pig))
            .expect("pig should attach to the loaded test chunk");
        let health_before = pig
            .as_living_entity()
            .expect("a pig is a living entity")
            .get_health();

        let charge = charge_at(DVec3::new(8.0, 64.0, 8.0), &world);
        let shared: SharedEntity = charge.clone();
        world
            .try_add_entity(shared)
            .expect("wind charge should attach to the loaded test chunk");

        charge.on_hit_entity(&pig, pig.position());

        let health_after = pig
            .as_living_entity()
            .expect("a pig is a living entity")
            .get_health();
        assert!(health_after < health_before);
        assert!(
            pig.velocity().x > 0.0,
            "the burst should throw the pig away from the charge"
        );
    }

    /// The whole point of a wind charge: the burst shoves a bystander and does
    /// not hurt it. A direct hit still hurts -- that damage comes from
    /// `onHitEntity`, not from the blast -- so the two have to be told apart.
    #[test]
    fn the_burst_shoves_a_bystander_without_hurting_it() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("wind_charge_burst_is_harmless");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(9.0, 64.0, 8.0),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&pig))
            .expect("pig should attach to the loaded test chunk");
        let health_before = pig
            .as_living_entity()
            .expect("a pig is a living entity")
            .get_health();

        let charge = charge_at(DVec3::new(8.0, 64.0, 8.0), &world);
        charge.explode(&world, DVec3::new(8.0, 64.0, 8.0));

        let living = pig.as_living_entity().expect("a pig is a living entity");
        assert!(
            (living.get_health() - health_before).abs() < f32::EPSILON,
            "the burst took health off a bystander"
        );
        assert!(
            pig.velocity().length_squared() > 0.0,
            "the burst did not move the bystander at all"
        );
    }

    #[test]
    fn a_charge_that_climbs_far_enough_above_the_world_bursts_instead_of_flying_on() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("wind_charge_above_world");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let ceiling = f64::from(world.get_max_y() + MAX_Y_OVERSHOOT + 1);
        let charge = charge_at(DVec3::new(8.5, ceiling, 8.5), &world);

        charge.tick();

        assert!(charge.is_removed());
    }

    #[test]
    fn a_deflected_charge_keeps_its_acceleration_and_its_owner_across_a_save() {
        init_vanilla_registry();
        let owner = Uuid::from_u128(0x0bad_c0de);
        let charge = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        charge.on_deflection(true);
        charge.set_owner_uuid(Some(owner));

        let mut nbt = NbtCompound::new();
        charge.save_additional(&mut nbt);
        assert_eq!(
            nbt.double("acceleration_power"),
            Some(DEFLECTED_ACCELERATION_POWER)
        );

        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test NBT should reborrow: {error}"));
        let loaded = WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            2,
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        loaded.load_additional((&borrowed).into());

        assert_eq!(
            loaded.acceleration_power().to_bits(),
            DEFLECTED_ACCELERATION_POWER.to_bits()
        );
        assert_eq!(loaded.owner_uuid(), Some(owner));
    }
}
