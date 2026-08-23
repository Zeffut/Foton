//! Wind charge.
//!
//! Vanilla parity: `WindCharge` on top of `AbstractWindCharge` and
//! `AbstractHurtingProjectile`. Thrown by hand, it flies dead straight -- no
//! gravity, no drag, no acceleration -- and bursts on the first thing it
//! touches, shoving whatever stands nearby without breaking a block or
//! lighting a fire.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entity_data::WindChargeEntityData;
use steel_registry::{sound_events, vanilla_damage_types, vanilla_entities};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityEventSource, EntitySyncedData, Projectile,
    ProjectileBase, ProjectileDeflection, ProjectileHit, RemovalReason, SharedEntity,
};
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};
use crate::world::{ClipHitResult, World};

/// How far the burst reaches.
///
/// How much harder than an ordinary blast a wind charge shoves.
///
/// Vanilla parity: the `1.22F` knockback multiplier of `WindCharge.explode`.
const BURST_KNOCKBACK: f64 = 1.22;

/// Vanilla parity: `WindCharge.RADIUS`, the `1.2F` handed to `Level.explode`.
const BURST_RADIUS: f32 = 1.2;

/// What a direct hit costs the entity it lands on.
///
/// Vanilla parity: the `1.0F` of `AbstractWindCharge.onHitEntity`. In vanilla
/// this is the only damage a wind charge deals; the burst itself deals none.
const IMPACT_DAMAGE: f32 = 1.0;

/// How far off the struck face the burst is centered.
///
/// Vanilla parity: the `multiply(0.25, 0.25, 0.25)` applied to the hit normal
/// in `AbstractWindCharge.onHitBlock`, which lifts the burst out of the block
/// so it pushes along the surface rather than from inside it.
const BLOCK_HIT_NORMAL_OFFSET: f64 = 0.25;

/// How long after being thrown a wind charge refuses to be deflected.
///
/// Vanilla parity: `WindCharge.noDeflectTicks`, which is what stops a charge
/// thrown point blank from bouncing straight back off its own thrower.
const NO_DEFLECT_TICKS: i32 = 5;

/// How far above the world a wind charge may climb before it bursts on its own.
///
/// Vanilla parity: the `getBlockY() > getMaxY() + 30` of
/// `AbstractWindCharge.tick`.
const MAX_Y_OVERSHOOT: i32 = 30;

/// Vanilla parity: the `0.1` an `AbstractHurtingProjectile` is born with and
/// that `onDeflection` restores after a deflecting attack. Every
/// `AbstractWindCharge` constructor zeroes it, so a wind charge carries it only
/// once something has knocked the charge off course.
const DEFLECTED_ACCELERATION_POWER: f64 = 0.1;

/// Vanilla parity: `AbstractHurtingProjectile.DEFLECTION_SCALE`, the halving
/// `onDeflection` applies to a deflection that was not an attack.
const PASSIVE_DEFLECTION_SCALE: f64 = 0.5;

/// Vanilla parity: `AbstractWindCharge.getInertia`, which returns 1 where every
/// other hurting projectile returns 0.95. `getLiquidInertia` returns the same
/// value, so water does not slow a wind charge either.
const INERTIA: f64 = 1.0;

/// Vanilla parity: the `4.0F` the client plays an explosion sound at when it
/// receives `ClientboundExplodePacket`.
const BURST_VOLUME: f32 = 4.0;

/// State that is not mirrored to clients.
struct WindChargeState {
    /// Vanilla parity: `AbstractHurtingProjectile.accelerationPower`, which
    /// every `AbstractWindCharge` constructor zeroes and only a deflection
    /// brings back.
    acceleration_power: f64,
    /// Vanilla parity: `WindCharge.noDeflectTicks`. Not persisted: vanilla
    /// keeps it in a plain field, so a reloaded charge starts the count over.
    no_deflect_ticks: i32,
}

impl WindChargeState {
    const fn new() -> Self {
        Self {
            // Vanilla parity: the `this.accelerationPower = 0.0` every
            // `AbstractWindCharge` constructor ends with.
            acceleration_power: 0.0,
            no_deflect_ticks: NO_DEFLECT_TICKS,
        }
    }
}

/// A thrown wind charge.
#[entity_behavior(class = "WindCharge")]
pub struct WindChargeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<WindChargeEntityData>,
    projectile_base: ProjectileBase,
    state: SyncMutex<WindChargeState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `WindChargeEntity`.
unsafe impl DowncastType for WindChargeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/wind_charge");
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
            state: SyncMutex::new(WindChargeState::new()),
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
            state: SyncMutex::new(WindChargeState::new()),
        }
    }

    /// Returns the acceleration a deflection has left the charge with.
    ///
    /// Vanilla parity: `AbstractHurtingProjectile.accelerationPower`.
    #[must_use]
    pub fn acceleration_power(&self) -> f64 {
        self.state.lock().acceleration_power
    }

    /// Returns how many ticks of deflection immunity are left.
    ///
    /// Vanilla parity: `WindCharge.noDeflectTicks`.
    #[must_use]
    pub fn no_deflect_ticks(&self) -> i32 {
        self.state.lock().no_deflect_ticks
    }

    /// Returns where a burst set off by a block hit is centered.
    ///
    /// Vanilla parity: the `hitResult.getLocation().add(normal * 0.25)` of
    /// `AbstractWindCharge.onHitBlock`.
    #[must_use]
    pub fn block_burst_center(location: DVec3, face_normal: DVec3) -> DVec3 {
        location + face_normal * BLOCK_HIT_NORMAL_OFFSET
    }

    /// Returns the velocity a wind charge carries into the next tick.
    ///
    /// Vanilla parity: `AbstractHurtingProjectile.applyInertia`. With the wind
    /// charge's acceleration power of zero and its inertia of one this is the
    /// identity, which is exactly why a wind charge flies in a straight line at
    /// a constant speed; a deflected charge picks acceleration back up and the
    /// first term starts to matter.
    #[must_use]
    pub fn inertia_applied(velocity: DVec3, acceleration_power: f64) -> DVec3 {
        (velocity + velocity.normalize_or_zero() * acceleration_power) * INERTIA
    }

    /// Returns whether `entity` is a wind charge of either kind.
    ///
    /// Vanilla parity: the `instanceof AbstractWindCharge` of
    /// `AbstractWindCharge.canCollideWith` and `canHitEntity`. The breeze's own
    /// charge has no behavior in Steel yet, but it is a registered entity type
    /// and one wind charge must not shoot another down, so both are matched.
    #[must_use]
    pub fn is_wind_charge(entity: &dyn Entity) -> bool {
        entity.entity_type() == &vanilla_entities::WIND_CHARGE
            || entity.entity_type() == &vanilla_entities::BREEZE_WIND_CHARGE
    }

    /// Bursts at `position`.
    ///
    /// Vanilla parity: `WindCharge.explode`, which calls `Level.explode` with
    /// radius 1.2, no fire, `ExplosionInteraction.TRIGGER` and a
    /// `SimpleExplosionDamageCalculator(explodesBlocks = true,
    /// damagesEntities = false, knockbackMultiplier = 1.22F,
    /// immuneBlocks = #blocks_wind_charge_explosions)`.
    ///
    /// One part of that is still out of reach and is worth naming rather than
    /// approximating in silence: Steel has no
    /// `Explosion.BlockInteraction.TRIGGER_BLOCK`. [`ExplosionBlockInteraction`]
    /// offers only `Keep` and `Destroy`, so `Keep` is used -- blocks survive,
    /// which is the half that matters, but the per-block
    /// `BlockState.onExplosionHit` that lets a vanilla wind charge slam a door,
    /// flip a trapdoor or ring a bell never runs. That block callback is the
    /// missing system.
    ///
    /// The `#blocks_wind_charge_explosions` immunity is moot while every block
    /// is kept. Vanilla's burst sound and its gust particles both travel inside
    /// `ClientboundExplodePacket`, which Steel never sends: the sound is played
    /// directly here so the burst is at least audible, and the gust particles
    /// are absent.
    fn explode(&self, world: &Arc<World>, position: DVec3) {
        world.explode_sparing(
            ExplosionSpec {
                source_entity_id: Some(self.id()),
                // Vanilla parity: `WindCharge.explode` passes a null damage source.
                damage_source: None,
                radius: BURST_RADIUS,
                fire: false,
                interaction: ExplosionBlockInteraction::Keep,
                // This is the whole point of a wind charge: it shoves what it
                // reaches and hurts none of it, harder than an ordinary blast.
                damages_entities: false,
                knockback_multiplier: BURST_KNOCKBACK,
            },
            position,
            &|_pos| true,
        );

        // Vanilla parity: the volume and pitch `ClientPacketListener.handleExplosion`
        // plays `SoundEvents.WIND_CHARGE_BURST` at.
        let pitch = 0.2f32.mul_add(rand::random::<f32>() - rand::random::<f32>(), 1.0) * 0.7;
        world.play_sound_at(
            &sound_events::ENTITY_WIND_CHARGE_WIND_BURST,
            SoundSource::Blocks,
            position,
            BURST_VOLUME,
            pitch,
            None,
        );
    }

    /// Runs one tick of flight.
    ///
    /// Vanilla parity: `AbstractHurtingProjectile.tick`, minus the parts the
    /// wind charge switches off -- it never burns (`shouldBurn` is false) and
    /// leaves no trail particle (`getTrailParticle` is null).
    ///
    /// Vanilla also discards the projectile when its owner has been removed or
    /// its block position is in an unloaded chunk. Neither half is reachable
    /// here: [`Projectile::get_owner`] cannot tell "no owner" from "owner
    /// removed" -- both come back as `None`, which vanilla reads as reason to
    /// keep flying -- and Steel's entity manager only ticks entities in loaded
    /// chunks.
    fn hurting_projectile_tick(&self) {
        // Vanilla `Entity.setOldPosAndRot()` runs before the tick; capture it
        // here so the rotation has a base to unwind against.
        self.set_old_position_to_current();
        self.base().set_old_rotation_to_current();

        let acceleration_power = self.state.lock().acceleration_power;
        self.set_velocity(Self::inertia_applied(self.velocity(), acceleration_power));

        let hit = self.get_hit_result_on_move_vector();
        let new_position = match &hit {
            Some(result) => result.location(),
            None => self.position() + self.velocity(),
        };

        if let Err(error) = self.try_set_position(new_position) {
            log::debug!("failed to advance wind charge {}: {error}", self.id());
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        self.rotate_towards_movement();
        self.apply_effects_from_blocks();
        self.projectile_base_tick();

        if let Some(result) = hit
            && self.is_alive()
            && !self.is_world_change_pending()
        {
            self.hit_target_or_deflect_self(&result);
        }
    }

    /// Points the charge along its velocity.
    ///
    /// Vanilla parity: `ProjectileUtil.rotateTowardsMovement(this, 0.2F)`, which
    /// snaps the rotation onto the movement vector rather than lerping toward it
    /// the way `Projectile.updateRotation` does. The `0.2F` vanilla passes is
    /// unused by that method, and the old-rotation unwinding that follows it
    /// only keeps client-side interpolation from taking the long way round, so
    /// neither is reproduced.
    fn rotate_towards_movement(&self) {
        let movement = self.velocity();
        if movement.length_squared() == 0.0 {
            return;
        }
        let horizontal = movement.x.hypot(movement.z);
        let yaw = movement.z.atan2(movement.x).to_degrees() as f32 + 90.0;
        let pitch = horizontal.atan2(movement.y).to_degrees() as f32 - 90.0;
        self.set_rotation((yaw, pitch));
    }
}

impl Entity for WindChargeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `AbstractWindCharge.tick` wrapped by `WindCharge.tick`.
    fn tick(&self) {
        if let Some(world) = self.level()
            && self.block_position().y() > world.get_max_y() + MAX_Y_OVERSHOOT
        {
            self.explode(&world, self.position());
            self.set_removed(RemovalReason::Discarded);
        } else {
            self.hurting_projectile_tick();
        }

        // Vanilla parity: the countdown of `WindCharge.tick`, which runs after
        // the flight whichever branch it took.
        let mut state = self.state.lock();
        if state.no_deflect_ticks > 0 {
            state.no_deflect_ticks -= 1;
        }
    }

    /// Vanilla parity: `AbstractWindCharge.canCollideWith`.
    fn can_collide_with(&self, other: &dyn Entity) -> bool {
        !Self::is_wind_charge(other)
            && other.can_be_collided_with(Some(self.as_entity_event_source()))
            && !self.is_passenger_of_same_vehicle(other)
    }

    /// Vanilla parity: `AbstractWindCharge.push`, an empty override -- nothing
    /// shoves a wind charge off course, not even another charge's burst.
    ///
    /// Steel's explosion assigns knockback with `Entity::set_velocity` rather
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

    /// Vanilla parity: `AbstractHurtingProjectile.addAdditionalSaveData`.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        nbt.insert("acceleration_power", self.state.lock().acceleration_power);
    }

    /// Vanilla parity: `AbstractHurtingProjectile.readAdditionalSaveData`,
    /// which falls back to the class default of 0.1 rather than to the zero an
    /// unflicked wind charge carries.
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.state.lock().acceleration_power = nbt
            .double("acceleration_power")
            .unwrap_or(DEFLECTED_ACCELERATION_POWER);
    }
}

impl Projectile for WindChargeEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `AbstractWindCharge.canHitEntity`. An end crystal is
    /// spared, so a charge cannot be used to pop one from a distance.
    ///
    /// Steel reaches this test through `Entity::can_be_hit_by_projectile`,
    /// which no living entity answers yes to yet -- `Entity::is_pickable` is
    /// still false for every mob -- so today a wind charge can only score a
    /// direct hit on a player. Mobs are reached by the burst alone. That is a
    /// gap in Steel's living entities rather than in the charge.
    fn can_hit_entity(&self, entity: &dyn Entity) -> bool {
        if Self::is_wind_charge(entity) || entity.entity_type() == &vanilla_entities::END_CRYSTAL {
            return false;
        }
        self.projectile_can_hit_entity(entity)
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
        if self.state.lock().no_deflect_ticks > 0 {
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
        let mut state = self.state.lock();
        if by_attack {
            state.acceleration_power = DEFLECTED_ACCELERATION_POWER;
        } else {
            state.acceleration_power *= PASSIVE_DEFLECTION_SCALE;
        }
    }

    /// Vanilla parity: `AbstractWindCharge.onHitEntity`. One point of damage to
    /// what it struck, then the burst at the charge's own position.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let owner = self.get_owner();
        let mut source = DamageSource::environment(&vanilla_damage_types::WIND_CHARGE)
            .with_direct_entity(self.id());
        if let Some(owner) = &owner {
            source = source.with_causing_entity(owner.id());
            if let Some(living) = owner.as_living_entity() {
                living.set_last_hurt_mob(Some(entity));
            }
        }

        let Some(world) = self.level() else {
            return;
        };
        // TODO: vanilla runs `EnchantmentHelper.doPostAttackEffects` on a living
        // target the hit actually damaged; Steel has no projectile enchantment
        // dispatch yet.
        entity.hurt(&world, &source, IMPACT_DAMAGE);
        self.explode(&world, self.position());
    }

    /// Vanilla parity: `AbstractWindCharge.onHitBlock`, which nudges the burst
    /// out of the face it struck before setting it off.
    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);
        let Some(world) = self.level() else {
            return;
        };
        let normal = hit.direction.offset_vec();
        let center = Self::block_burst_center(
            hit.location,
            DVec3::new(
                f64::from(normal.x),
                f64::from(normal.y),
                f64::from(normal.z),
            ),
        );
        self.explode(&world, center);
        self.set_removed(RemovalReason::Discarded);
    }

    /// Vanilla parity: `AbstractWindCharge.onHit`, which discards the charge
    /// whatever it ran into. A block hit has already discarded itself by the
    /// time this runs, exactly as in vanilla.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        self.set_removed(RemovalReason::Discarded);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::types::UpdateFlags;
    use steel_utils::{BlockPos, ChunkPos};

    use crate::behavior::init_behaviors;
    use crate::entity::entities::{EndCrystalEntity, PigEntity};
    use crate::entity::next_entity_id;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    use super::*;

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

        assert_eq!(WindChargeEntity::inertia_applied(velocity, 0.0), velocity);
    }

    #[test]
    fn a_charge_an_attack_has_deflected_picks_up_speed_along_its_heading() {
        let accelerated = WindChargeEntity::inertia_applied(
            DVec3::new(0.0, 0.0, 2.0),
            DEFLECTED_ACCELERATION_POWER,
        );

        assert!((accelerated.z - 2.1).abs() < 1.0e-9);
        assert!(accelerated.x.abs() < 1.0e-12);
        assert!(accelerated.y.abs() < 1.0e-12);
    }

    #[test]
    fn a_burst_against_a_wall_sits_a_quarter_block_out_from_the_face_it_struck() {
        assert_eq!(
            WindChargeEntity::block_burst_center(
                DVec3::new(4.0, 65.0, 8.0),
                DVec3::new(-1.0, 0.0, 0.0),
            ),
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
