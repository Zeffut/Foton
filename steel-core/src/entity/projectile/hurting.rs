//! Vanilla `AbstractHurtingProjectile` -- the self-propelled projectile loop.
//!
//! What sets these apart from a thrown snowball is that they carry their own
//! thrust. Every tick the projectile adds `acceleration_power` along its own
//! heading before drag is applied, so a fireball crosses a room at the speed it
//! left the ghast instead of arcing to the floor; gravity never enters into it.

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_utils::ChunkPos;
use steel_utils::locks::SyncMutex;

use crate::entity::projectile::{Projectile, rotate_towards_movement};
use crate::entity::{RemovalReason, SharedEntity};

/// Thrust a hurting projectile adds along its heading each tick.
///
/// Vanilla parity: the initial-acceleration-power constant of
/// `AbstractHurtingProjectile` (vanilla spells the field name wrong).
pub const INITIAL_ACCELERATION_POWER: f64 = 0.1;

/// What a deflection that was not an attack leaves of the thrust.
///
/// Vanilla parity: `AbstractHurtingProjectile.DEFLECTION_SCALE`.
const DEFLECTION_SCALE: f64 = 0.5;

/// Velocity kept each tick in air.
///
/// Vanilla parity: `AbstractHurtingProjectile.getInertia`.
const AIR_INERTIA: f64 = 0.95;

/// Velocity kept each tick under water.
///
/// Vanilla parity: `AbstractHurtingProjectile.getLiquidInertia`. Water costs a
/// fireball four times as much speed as air, which is why one shot into a pool
/// stops dead rather than crossing it.
const LIQUID_INERTIA: f64 = 0.8;

/// How long a burning projectile keeps itself alight, in ticks.
///
/// Vanilla parity: the `igniteForSeconds(1.0F)` of `AbstractHurtingProjectile.tick`,
/// reapplied every tick so the flame never lapses in flight.
const BURN_TICKS: i32 = 20;

/// How far toward its heading a hurting projectile turns each tick.
///
/// Vanilla parity: the `rotateTowardsMovement(this, 0.2F)` of the tick loop.
const ROTATION_SPEED: f32 = 0.2;

/// Runtime fields shared by vanilla hurting projectiles.
///
/// Vanilla parity: the `AbstractHurtingProjectile.accelerationPower` field.
pub struct HurtingProjectileBase {
    acceleration_power: SyncMutex<f64>,
}

impl HurtingProjectileBase {
    /// Creates hurting-projectile state with vanilla's starting thrust.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            acceleration_power: SyncMutex::new(INITIAL_ACCELERATION_POWER),
        }
    }
}

impl Default for HurtingProjectileBase {
    fn default() -> Self {
        Self::new()
    }
}

/// Vanilla-shaped behavior shared by entities that extend `AbstractHurtingProjectile`.
pub trait HurtingProjectile: Projectile {
    /// Returns shared hurting-projectile runtime state.
    fn hurting_projectile_base(&self) -> &HurtingProjectileBase;

    /// Returns the thrust added along the heading each tick.
    fn acceleration_power(&self) -> f64 {
        *self.hurting_projectile_base().acceleration_power.lock()
    }

    /// Sets the thrust added along the heading each tick.
    fn set_acceleration_power(&self, power: f64) {
        *self.hurting_projectile_base().acceleration_power.lock() = power;
    }

    /// Vanilla parity: `AbstractHurtingProjectile.getInertia`.
    fn get_inertia(&self) -> f64 {
        AIR_INERTIA
    }

    /// Vanilla parity: `AbstractHurtingProjectile.getLiquidInertia`.
    fn get_liquid_inertia(&self) -> f64 {
        LIQUID_INERTIA
    }

    /// Returns whether this projectile sets itself alight while it flies.
    ///
    /// Vanilla parity: `AbstractHurtingProjectile.shouldBurn`. The dragon
    /// fireball and the wither skull say no; a ghast's fireball says yes, and
    /// that flame is what the client draws.
    fn should_burn(&self) -> bool {
        true
    }

    /// Points the projectile down `direction` at its current thrust.
    ///
    /// Vanilla parity: `AbstractHurtingProjectile.assignDirectionalMovement`.
    fn assign_directional_movement(&self, direction: DVec3) {
        let speed = self.acceleration_power();
        self.set_velocity(direction.normalize_or_zero() * speed);
        self.mark_velocity_sync();
    }

    /// Aims a freshly spawned projectile away from the mob that fired it.
    ///
    /// Vanilla parity: the `AbstractHurtingProjectile(type, mob, direction, level)`
    /// constructor -- owner, then the shooter's rotation, then the thrust.
    fn shoot_from_owner(&self, owner: &SharedEntity, direction: DVec3) {
        self.set_owner_entity(Some(owner));
        self.set_rotation(owner.rotation());
        self.base().set_old_rotation_to_current();
        self.assign_directional_movement(direction);
    }

    /// Adds thrust along the heading, then applies drag.
    ///
    /// Vanilla parity: `AbstractHurtingProjectile.applyInertia`.
    fn apply_hurting_inertia(&self) {
        // VANILLA CLIENT-LOCAL: `applyInertia` also spawns four bubbles behind a
        // submerged fireball.
        let inertia = if self.is_in_water() {
            self.get_liquid_inertia()
        } else {
            self.get_inertia()
        };
        let movement = self.velocity();
        let thrust = movement.normalize_or_zero() * self.acceleration_power();
        self.set_velocity((movement + thrust) * inertia);
    }

    /// Vanilla parity: `AbstractHurtingProjectile.tick`.
    ///
    /// Reached from a subclass's `tick`. Thrust and drag, then the move-vector
    /// raycast, then the move, then the `Projectile` base tick, then the hit.
    fn hurting_projectile_tick(&self) {
        // Vanilla's level runs `Entity.setOldPosAndRot()` before ticking the
        // entity; Steel's projectiles capture it at the top of their own tick,
        // the same way `ThrowableProjectile::throwable_projectile_tick` does.
        self.set_old_position_to_current();
        self.base().set_old_rotation_to_current();

        self.apply_hurting_inertia();

        // Vanilla parity: a projectile whose shooter has been removed, or that
        // has drifted out of a loaded chunk, is dropped rather than left flying.
        let owner_removed = self.get_owner().is_some_and(|owner| owner.is_removed());
        let chunk_loaded = self.level().is_some_and(|world| {
            world.has_full_chunk(ChunkPos::from_block_pos(self.block_position()))
        });
        if owner_removed || !chunk_loaded {
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        let hit = self.get_hit_result_on_move_vector();
        let new_position = match &hit {
            Some(result) => result.location(),
            None => self.position() + self.velocity(),
        };

        rotate_towards_movement(self.as_projectile_event_source(), ROTATION_SPEED);
        if let Err(error) = self.try_set_position(new_position) {
            log::debug!(
                "failed to advance hurting projectile {}: {error}",
                self.id()
            );
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        self.apply_effects_from_blocks();
        self.projectile_base_tick();

        if self.should_burn() {
            self.ignite_for_ticks(BURN_TICKS);
        }

        if let Some(result) = hit
            && self.is_alive()
            && !self.is_world_change_pending()
        {
            self.hit_target_or_deflect_self(&result);
        }

        // VANILLA CLIENT-LOCAL: `createParticleTrail` draws the smoke behind it.
    }

    /// Vanilla parity: `AbstractHurtingProjectile.onDeflection`.
    ///
    /// A projectile batted away by an attack gets its full thrust back; one
    /// merely bounced off a shield keeps half, so it falls short.
    fn hurting_projectile_on_deflection(&self, by_attack: bool) {
        if by_attack {
            self.set_acceleration_power(INITIAL_ACCELERATION_POWER);
        } else {
            self.set_acceleration_power(self.acceleration_power() * DEFLECTION_SCALE);
        }
    }

    /// Saves the vanilla `acceleration_power` field.
    fn save_hurting_projectile(&self, nbt: &mut NbtCompound) {
        nbt.insert("acceleration_power", self.acceleration_power());
    }

    /// Loads the vanilla `acceleration_power` field.
    fn load_hurting_projectile(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.set_acceleration_power(
            nbt.double("acceleration_power")
                .unwrap_or(INITIAL_ACCELERATION_POWER),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use crate::entity::entities::LargeFireballEntity;
    use crate::entity::{Entity, next_entity_id};

    use super::{HurtingProjectile, INITIAL_ACCELERATION_POWER};

    fn fireball() -> LargeFireballEntity {
        init_vanilla_registry();
        LargeFireballEntity::new(
            &vanilla_entities::FIREBALL,
            next_entity_id(),
            DVec3::ZERO,
            Weak::new(),
        )
    }

    #[test]
    fn thrust_keeps_a_fireball_at_speed_instead_of_letting_drag_stop_it() {
        let fireball = fireball();
        fireball.set_velocity(DVec3::new(2.0, 0.0, 0.0));

        // 0.95 * (2.0 + 0.1) is above the speed it started at: the thrust more
        // than pays for the drag, which is why a ghast's fireball crosses a room.
        fireball.apply_hurting_inertia();

        assert!((fireball.velocity().x - 1.995).abs() < 1.0e-9);
        assert!(fireball.velocity().y.abs() < 1.0e-9);
        assert!(fireball.velocity().z.abs() < 1.0e-9);
    }

    #[test]
    fn a_stationary_hurting_projectile_gains_no_thrust_from_nowhere() {
        let fireball = fireball();
        fireball.set_velocity(DVec3::ZERO);

        fireball.apply_hurting_inertia();

        assert_eq!(fireball.velocity(), DVec3::ZERO);
    }

    #[test]
    fn directional_movement_leaves_the_muzzle_at_the_thrust_speed() {
        let fireball = fireball();

        fireball.assign_directional_movement(DVec3::new(0.0, 0.0, 5.0));

        assert!((fireball.velocity().z - INITIAL_ACCELERATION_POWER).abs() < 1.0e-9);
        assert!(fireball.needs_velocity_sync());
    }

    #[test]
    fn being_batted_back_restores_full_thrust_but_a_glancing_deflection_halves_it() {
        let fireball = fireball();

        fireball.hurting_projectile_on_deflection(false);
        assert!((fireball.acceleration_power() - INITIAL_ACCELERATION_POWER / 2.0).abs() < 1.0e-9);

        fireball.hurting_projectile_on_deflection(true);
        assert!((fireball.acceleration_power() - INITIAL_ACCELERATION_POWER).abs() < 1.0e-9);
    }
}
