//! What the two wind charges share.
//!
//! Vanilla parity: `AbstractWindCharge`, the base a hand-thrown `WindCharge`
//! and a breeze's `BreezeWindCharge` sit on. Both fly dead straight -- no
//! gravity, no drag, no acceleration -- and burst on the first thing they
//! touch, shoving whatever stands nearby without breaking a block or lighting
//! a fire. Only the size of that burst, how hard it shoves and the noise it
//! makes differ, so those three are what [`AbstractWindCharge`] asks a concrete
//! charge for.

use std::sync::Arc;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_registry::particle_type::ParticleData;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{vanilla_damage_types, vanilla_entities, vanilla_particle_types};
use steel_utils::locks::SyncMutex;
use steel_utils::random::weighted_list::WeightedList;

use crate::entity::damage::DamageSource;
use crate::entity::{Entity, Projectile, RemovalReason, SharedEntity};
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};
use crate::world::{ClipHitResult, World};

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

/// How far above the world a wind charge may climb before it bursts on its own.
///
/// Vanilla parity: the `getBlockY() > getMaxY() + 30` of
/// `AbstractWindCharge.tick`.
pub(super) const MAX_Y_OVERSHOOT: i32 = 30;

/// Vanilla parity: the `0.1` an `AbstractHurtingProjectile` is born with and
/// that `onDeflection` restores after a deflecting attack. Every
/// `AbstractWindCharge` constructor zeroes it, so a wind charge carries it only
/// once something has knocked the charge off course.
pub(super) const DEFLECTED_ACCELERATION_POWER: f64 = 0.1;

/// Vanilla parity: `AbstractHurtingProjectile.DEFLECTION_SCALE`, the halving
/// `onDeflection` applies to a deflection that was not an attack.
const PASSIVE_DEFLECTION_SCALE: f64 = 0.5;

/// Vanilla parity: `AbstractWindCharge.getInertia`, which returns 1 where every
/// other hurting projectile returns 0.95. `getLiquidInertia` returns the same
/// value, so water does not slow a wind charge either.
const INERTIA: f64 = 1.0;

/// The one field `AbstractWindCharge` keeps that clients never see.
pub(super) struct AbstractWindChargeBase {
    /// Vanilla parity: `AbstractHurtingProjectile.accelerationPower`, which
    /// every `AbstractWindCharge` constructor zeroes and only a deflection
    /// brings back.
    acceleration_power: SyncMutex<f64>,
}

impl AbstractWindChargeBase {
    /// Creates the base of a charge that has not been deflected.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            // Vanilla parity: the `this.accelerationPower = 0.0` every
            // `AbstractWindCharge` constructor ends with.
            acceleration_power: SyncMutex::new(0.0),
        }
    }
}

/// Returns the velocity a wind charge carries into the next tick.
///
/// Vanilla parity: `AbstractHurtingProjectile.applyInertia`. With the wind
/// charge's acceleration power of zero and its inertia of one this is the
/// identity, which is exactly why a wind charge flies in a straight line at a
/// constant speed; a deflected charge picks acceleration back up and the first
/// term starts to matter.
#[must_use]
pub(super) fn inertia_applied(velocity: DVec3, acceleration_power: f64) -> DVec3 {
    (velocity + velocity.normalize_or_zero() * acceleration_power) * INERTIA
}

/// Returns where a burst set off by a block hit is centered.
///
/// Vanilla parity: the `hitResult.getLocation().add(normal * 0.25)` of
/// `AbstractWindCharge.onHitBlock`.
#[must_use]
pub(super) fn block_burst_center(location: DVec3, face_normal: DVec3) -> DVec3 {
    location + face_normal * BLOCK_HIT_NORMAL_OFFSET
}

/// Returns whether `entity` is a wind charge of either kind.
///
/// Vanilla parity: the `instanceof AbstractWindCharge` of
/// `AbstractWindCharge.canCollideWith` and `canHitEntity`. One wind charge must
/// not shoot another down, whoever threw either of them.
#[must_use]
pub(super) fn is_wind_charge(entity: &dyn Entity) -> bool {
    entity.entity_type() == &vanilla_entities::WIND_CHARGE
        || entity.entity_type() == &vanilla_entities::BREEZE_WIND_CHARGE
}

/// A wind charge of either kind.
///
/// Vanilla parity: `net.minecraft.world.entity.projectile.hurtingprojectile.windcharge.AbstractWindCharge`.
pub(super) trait AbstractWindCharge: Projectile {
    /// Returns the acceleration a deflection has left this charge with.
    fn wind_charge_base(&self) -> &AbstractWindChargeBase;

    /// Returns how far this charge's burst reaches.
    ///
    /// Vanilla parity: the radius each subclass hands `Level.explode`.
    fn burst_radius(&self) -> f32;

    /// Returns how much harder than an ordinary blast this charge shoves.
    ///
    /// Vanilla parity: the `knockbackMultiplier` of the subclass's
    /// `SimpleExplosionDamageCalculator`. `Optional.empty()` there means the
    /// `ExplosionDamageCalculator.getKnockbackMultiplier` default of one.
    fn burst_knockback(&self) -> f64;

    /// Returns the noise the burst makes.
    fn burst_sound(&self) -> SoundEventRef;

    /// Vanilla parity: `AbstractHurtingProjectile.accelerationPower`.
    fn acceleration_power(&self) -> f64 {
        *self.wind_charge_base().acceleration_power.lock()
    }

    /// Writes the acceleration a deflection or a load has produced.
    fn set_acceleration_power(&self, acceleration_power: f64) {
        *self.wind_charge_base().acceleration_power.lock() = acceleration_power;
    }

    /// Bursts at `position`.
    ///
    /// Vanilla parity: the `explode` each subclass overrides, which calls
    /// `Level.explode` with the subclass's radius, no fire,
    /// `ExplosionInteraction.TRIGGER` and a
    /// `SimpleExplosionDamageCalculator(explodesBlocks = true,
    /// damagesEntities = false, immuneBlocks = #blocks_wind_charge_explosions)`.
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
    /// is kept. The burst sound and the gust emitter both travel inside the
    /// explosion packet, which is why they are named here rather than played
    /// separately: a wind charge is the one blast whose presentation is not
    /// vanilla's default.
    fn explode(&self, world: &Arc<World>, position: DVec3) {
        world.explode_sparing(
            ExplosionSpec {
                direct_entity_id: Some(self.id()),
                causing_entity_id: self.get_owner().map(|owner| owner.id()),
                // Vanilla parity: both `explode` overrides pass a null damage source.
                damage_source: None,
                radius: self.burst_radius(),
                fire: false,
                interaction: ExplosionBlockInteraction::Keep,
                // This is the whole point of a wind charge: it shoves what it
                // reaches and hurts none of it.
                damages_entities: false,
                knockback_multiplier: self.burst_knockback(),
                small_particle: ParticleData::simple(&vanilla_particle_types::GUST_EMITTER_SMALL),
                large_particle: ParticleData::simple(&vanilla_particle_types::GUST_EMITTER_LARGE),
                // Vanilla parity: `WeightedList.of()`. A wind charge breaks
                // nothing, so it has no debris to throw.
                block_particles: WeightedList::empty(),
                sound: self.burst_sound(),
            },
            position,
            &|_pos| true,
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

        self.set_velocity(inertia_applied(self.velocity(), self.acceleration_power()));

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
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an angle in degrees, immediately used as a rotation"
        )]
        let yaw = movement.z.atan2(movement.x).to_degrees() as f32 + 90.0;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an angle in degrees, immediately used as a rotation"
        )]
        let pitch = horizontal.atan2(movement.y).to_degrees() as f32 - 90.0;
        self.set_rotation((yaw, pitch));
    }

    /// Vanilla parity: `AbstractWindCharge.tick`, which bursts a charge that has
    /// climbed far enough above the world rather than letting it fly forever.
    fn abstract_wind_charge_tick(&self) {
        let Some(world) = self.level() else {
            self.hurting_projectile_tick();
            return;
        };
        if self.block_position().y() > world.get_max_y() + MAX_Y_OVERSHOOT {
            self.explode(&world, self.position());
            self.set_removed(RemovalReason::Discarded);
        } else {
            self.hurting_projectile_tick();
        }
    }

    /// Vanilla parity: `AbstractWindCharge.canCollideWith`.
    fn wind_charge_can_collide_with(&self, other: &dyn Entity) -> bool {
        !is_wind_charge(other)
            && other.can_be_collided_with(Some(self.as_entity_event_source()))
            && !self.is_passenger_of_same_vehicle(other)
    }

    /// Vanilla parity: `AbstractWindCharge.canHitEntity`. An end crystal is
    /// spared, so a charge cannot be used to pop one from a distance.
    ///
    /// Steel reaches this test through `Entity::can_be_hit_by_projectile`,
    /// which no living entity answers yes to yet -- `Entity::is_pickable` is
    /// still false for every mob -- so today a wind charge can only score a
    /// direct hit on a player. Mobs are reached by the burst alone. That is a
    /// gap in Steel's living entities rather than in the charge.
    fn wind_charge_can_hit_entity(&self, entity: &dyn Entity) -> bool {
        if is_wind_charge(entity) || entity.entity_type() == &vanilla_entities::END_CRYSTAL {
            return false;
        }
        self.projectile_can_hit_entity(entity)
    }

    /// Vanilla parity: `AbstractHurtingProjectile.onDeflection`.
    fn wind_charge_on_deflection(&self, by_attack: bool) {
        if by_attack {
            self.set_acceleration_power(DEFLECTED_ACCELERATION_POWER);
        } else {
            self.set_acceleration_power(self.acceleration_power() * PASSIVE_DEFLECTION_SCALE);
        }
    }

    /// Vanilla parity: `AbstractWindCharge.onHitEntity`. One point of damage to
    /// what it struck, then the burst at the charge's own position.
    fn wind_charge_on_hit_entity(&self, entity: &SharedEntity) {
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
    fn wind_charge_on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);
        let Some(world) = self.level() else {
            return;
        };
        let normal = hit.direction.offset_vec();
        let center = block_burst_center(
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

    /// Vanilla parity: `AbstractHurtingProjectile.addAdditionalSaveData`.
    fn save_wind_charge(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        nbt.insert("acceleration_power", self.acceleration_power());
    }

    /// Vanilla parity: `AbstractHurtingProjectile.readAdditionalSaveData`,
    /// which falls back to the class default of 0.1 rather than to the zero an
    /// unflicked wind charge carries.
    fn load_wind_charge(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.set_acceleration_power(
            nbt.double("acceleration_power")
                .unwrap_or(DEFLECTED_ACCELERATION_POWER),
        );
    }
}
