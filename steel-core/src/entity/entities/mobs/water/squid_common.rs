//! Behavior shared by the squid and the glow squid.
//!
//! Vanilla puts this in `Squid`, which the glow squid extends. Steel has no
//! inheritance, so it lives here the way [`super::fish`] serves the cod and the
//! salmon: the two entities keep only what actually differs, which for the glow
//! squid is four sounds, a particle, and the dark ticks it counts after being
//! hurt.

use std::f32::consts::PI;
use std::f64::consts::TAU;
use std::sync::Arc;

use glam::DVec3;
use steel_registry::particle_type::{ParticleData, ParticleTypeRef};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_damage_types;
use steel_utils::Downcast as _;
use steel_utils::locks::SyncMutex;

use crate::entity::ai::goal::{Goal, GoalControls};
use crate::entity::damage::DamageSource;
use crate::entity::{AgeableMob, Entity, Mob, PathfinderMob};
use crate::world::World;

/// Air a squid holds, in ticks.
///
/// Vanilla parity: the `300` of `AgeableWaterCreature.handleAirSupply`.
pub(super) const SQUID_AIR_SUPPLY: i32 = 300;

/// Damage a suffocating squid takes each tick.
pub(super) const SUFFOCATION_DAMAGE: f32 = 2.0;

/// Ticks between two idle sounds.
pub(super) const AMBIENT_SOUND_INTERVAL: i32 = 120;

/// Chance a squid spawns small.
pub(super) const BABY_SPAWN_CHANCE: f32 = 0.05;

/// Ticks of stillness after which a squid stops pushing itself at all.
///
/// Vanilla parity: the `noActionTime > 100` of `SquidRandomMovementGoal`.
const IDLE_TICKS_BEFORE_DRIFTING: i32 = 100;

/// One tick in this many picks a new drift direction.
const RETARGET_INTERVAL_TICKS: i32 = 50;

/// How hard a beat pushes sideways.
const PUSH_HORIZONTAL: f64 = 0.2;

/// Slowest a beat pushes upward.
const PUSH_VERTICAL_MIN: f64 = -0.1;

/// Spread of the upward push.
const PUSH_VERTICAL_RANGE: f64 = 0.2;

/// How far through a beat the push lands.
const PUSH_POINT_IN_STROKE: f32 = 0.75;

/// Drag applied on the recovery half of a beat.
const RECOVERY_DRAG: f64 = 0.9;

/// Below this a movement vector counts as nothing.
pub(super) const MOVEMENT_EPSILON: f64 = 1.0e-7;

/// Particles in one ink cloud.
const INK_PARTICLE_COUNT: i32 = 30;

/// Sideways scatter of each ink jet.
const INK_SCATTER: f64 = 0.6;

/// Shortest distance an ink jet is thrown, for a grown squid.
const INK_REACH_ADULT: f64 = 0.3;

/// Shortest distance an ink jet is thrown, for a baby.
const INK_REACH_BABY: f64 = 0.1;

/// Extra distance an ink jet may travel beyond the minimum.
const INK_REACH_SPREAD: f64 = 2.0;

/// Speed the ink particles carry.
const INK_SPEED: f64 = 0.1;

/// How fast a squid turns to face where it is drifting.
const FACING_RESPONSIVENESS: f32 = 0.1;

/// Squared distance within which a squid bolts from what hurt it.
const FLEE_TRIGGER_DISTANCE_SQR: f64 = 100.0;

/// How hard a squid pushes away from what hurt it.
const FLEE_SPEED: f64 = 3.0;

/// Distance past which the flee push starts easing off.
const FLEE_EASE_FROM: f64 = 5.0;

/// Divisor turning the flee push into a per-tick movement vector.
const FLEE_PUSH_DIVISOR: f64 = 20.0;

/// State a squid keeps to itself.
pub(super) struct SquidState {
    /// Where the next tentacle beat will throw it.
    pub movement_vector: DVec3,
    /// How far through the tentacle beat the squid is, in radians.
    pub tentacle_movement: f32,
    /// How fast the beat advances.
    pub tentacle_speed: f32,
}

impl SquidState {
    /// Creates the drift state a freshly spawned squid starts with.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            movement_vector: DVec3::ZERO,
            tentacle_movement: 0.0,
            tentacle_speed: roll_tentacle_speed(),
        }
    }
}

/// What the shared goals need from a concrete squid.
///
/// The goals reach the entity by downcast, and a downcast needs a concrete
/// type, so each squid hands over the two accessors rather than a trait.
#[derive(Clone, Copy)]
pub(super) struct SquidHooks {
    /// Points the next beat somewhere.
    pub set_movement_vector: fn(&dyn PathfinderMob, DVec3),
    /// Returns whether the squid has a direction to throw itself in.
    pub has_movement_vector: fn(&dyn PathfinderMob) -> bool,
    /// Returns whether the squid is in water.
    pub is_in_water: fn(&dyn PathfinderMob) -> bool,
}

/// Wraps an angle into the shortest turn that reaches it.
///
/// Vanilla parity: `Mth.wrapDegrees`.
fn wrap_degrees(degrees: f32) -> f32 {
    let wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else if wrapped < -180.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

/// Rolls the speed of one tentacle beat.
///
/// Vanilla parity: `1.0F / (nextFloat() + 1.0F) * 0.2F`, so a squid that draws
/// a low number beats slowly for a while.
fn roll_tentacle_speed() -> f32 {
    (rand::random::<f32>() + 1.0).recip() * 0.2
}

/// Advances the tentacle beat and pushes the squid when it lands.
///
/// Vanilla parity: the in-water branch of `Squid.aiStep`.
pub(super) fn tick_tentacles<M: Mob + ?Sized>(squid: &M, state: &SyncMutex<SquidState>) {
    let (stroke, movement_vector) = {
        let mut state = state.lock();
        state.tentacle_movement += state.tentacle_speed;
        if state.tentacle_movement > TAU as f32 {
            state.tentacle_movement -= TAU as f32;
            if rand::random_range(0..10) == 0 {
                state.tentacle_speed = roll_tentacle_speed();
            }
        }
        (state.tentacle_movement, state.movement_vector)
    };

    if !squid.is_in_water() {
        return;
    }

    if stroke < PI {
        // The forward half of the stroke: the push lands three quarters
        // through it, which is what makes a squid lurch rather than glide.
        if stroke / PI > PUSH_POINT_IN_STROKE {
            squid.set_velocity(movement_vector);
        }
    } else {
        squid.set_velocity(squid.velocity() * RECOVERY_DRAG);
    }
}

/// Turns the squid to face the way it is drifting.
///
/// Vanilla parity: the yaw interpolation of `Squid.aiStep`. This is not only
/// cosmetic: the yaw is synced, and the ink is thrown relative to it.
pub(super) fn face_travel_direction<M: Mob + ?Sized>(squid: &M) {
    let velocity = squid.velocity();
    if velocity.length_squared() < MOVEMENT_EPSILON {
        return;
    }

    let (yaw, pitch) = squid.rotation();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an angle in degrees fits a float, as it does in vanilla"
    )]
    let wanted_yaw = -(velocity.x.atan2(velocity.z).to_degrees() as f32);
    let horizontal = velocity.with_y(0.0).length();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an angle in degrees fits a float, as it does in vanilla"
    )]
    let wanted_pitch = -(horizontal.atan2(velocity.y).to_degrees() as f32);

    squid.set_rotation((
        FACING_RESPONSIVENESS.mul_add(wrap_degrees(wanted_yaw - yaw), yaw),
        FACING_RESPONSIVENESS.mul_add(wrap_degrees(wanted_pitch - pitch), pitch),
    ));
}

/// Rotates a vector out of the squid's own frame into the world's.
///
/// Vanilla parity: `Squid.rotateVector`.
fn rotate_vector<M: Mob + ?Sized>(squid: &M, vector: DVec3) -> DVec3 {
    let (yaw, pitch) = squid.rotation();
    let pitch_radians = f64::from(pitch).to_radians();
    let yaw_radians = f64::from(-yaw).to_radians();

    let (sin_pitch, cos_pitch) = pitch_radians.sin_cos();
    let pitched = DVec3::new(
        vector.x,
        vector.y * cos_pitch - vector.z * sin_pitch,
        vector.y * sin_pitch + vector.z * cos_pitch,
    );

    let (sin_yaw, cos_yaw) = yaw_radians.sin_cos();
    DVec3::new(
        pitched.z.mul_add(sin_yaw, pitched.x * cos_yaw),
        pitched.y,
        pitched.x.mul_add(-sin_yaw, pitched.z * cos_yaw),
    )
}

/// Squirts a cloud of ink.
///
/// Vanilla parity: `Squid.spawnInk`. The jets come out of the squid's
/// underside, wherever that happens to be pointing. The particle and the sound
/// are the caller's, because that is all a glow squid changes.
pub(super) fn spawn_ink<M: Mob + AgeableMob + ?Sized>(
    squid: &M,
    world: &Arc<World>,
    squirt_sound: SoundEventRef,
    ink_particle: ParticleTypeRef,
) {
    squid.play_sound(squirt_sound, 1.0, 1.0);

    let origin = rotate_vector(squid, DVec3::new(0.0, -1.0, 0.0)) + squid.position();
    let reach = if AgeableMob::is_baby(squid) {
        INK_REACH_BABY
    } else {
        INK_REACH_ADULT
    };

    for _ in 0..INK_PARTICLE_COUNT {
        let scatter = || rand::random::<f64>().mul_add(INK_SCATTER, -INK_SCATTER / 2.0);
        let direction = rotate_vector(squid, DVec3::new(scatter(), -1.0, scatter()));
        let jet = direction * rand::random::<f64>().mul_add(INK_REACH_SPREAD, reach);

        world.send_particles(
            ParticleData::simple(ink_particle),
            origin.with_y(origin.y + 0.5),
            0,
            jet,
            INK_SPEED,
        );
    }
}

/// Drains air out of water and refills it in.
///
/// Vanilla parity: `AgeableWaterCreature.handleAirSupply`.
pub(super) fn handle_air_supply<M: Mob + ?Sized>(squid: &M, world: &World, air_before_tick: i32) {
    if Entity::is_alive(squid) && !squid.is_in_water() {
        squid.set_air_supply(air_before_tick - 1);
        if squid.should_take_drowning_damage() {
            squid.set_air_supply(0);
            squid.hurt_server(
                world,
                &DamageSource::environment(&vanilla_damage_types::DROWN),
                SUFFOCATION_DAMAGE,
            );
        }
    } else {
        squid.set_air_supply(SQUID_AIR_SUPPLY);
    }
}

/// Picks a direction for the next tentacle beat.
///
/// Vanilla parity: `Squid.SquidRandomMovementGoal`.
pub(super) struct SquidRandomMovementGoal {
    hooks: SquidHooks,
}

impl SquidRandomMovementGoal {
    /// Creates the drift goal for a squid that exposes these accessors.
    #[must_use]
    pub(super) const fn new(hooks: SquidHooks) -> Self {
        Self { hooks }
    }
}

impl Goal for SquidRandomMovementGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        true
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        // Vanilla parity: a squid nobody has bothered for five seconds stops
        // pushing itself and simply drifts.
        if mob.no_action_time() > IDLE_TICKS_BEFORE_DRIFTING {
            (self.hooks.set_movement_vector)(mob, DVec3::ZERO);
            return;
        }

        let needs_direction = rand::random_range(0..RETARGET_INTERVAL_TICKS) == 0
            || !(self.hooks.is_in_water)(mob)
            || !(self.hooks.has_movement_vector)(mob);
        if !needs_direction {
            return;
        }

        let angle = rand::random::<f64>() * TAU;
        (self.hooks.set_movement_vector)(
            mob,
            DVec3::new(
                angle.cos() * PUSH_HORIZONTAL,
                rand::random::<f64>().mul_add(PUSH_VERTICAL_RANGE, PUSH_VERTICAL_MIN),
                angle.sin() * PUSH_HORIZONTAL,
            ),
        );
    }
}

/// Bolts away from whatever hurt the squid.
///
/// Vanilla parity: `Squid.SquidFleeGoal`.
pub(super) struct SquidFleeGoal {
    hooks: SquidHooks,
}

impl SquidFleeGoal {
    /// Creates the flee goal for a squid that exposes these accessors.
    #[must_use]
    pub(super) const fn new(hooks: SquidHooks) -> Self {
        Self { hooks }
    }
}

impl Goal for SquidFleeGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(attacker) = mob.last_hurt_by_mob() else {
            return false;
        };
        mob.is_in_water()
            && attacker.position().distance_squared(mob.position()) < FLEE_TRIGGER_DISTANCE_SQR
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(attacker) = mob.last_hurt_by_mob() else {
            return;
        };

        let away = mob.position() - attacker.position();
        let distance = away.length();
        if distance <= 0.0 {
            return;
        }

        // Vanilla eases the push off past five blocks, so a squid stops
        // sprinting once it has put some water between them.
        let mut speed = FLEE_SPEED;
        if distance > FLEE_EASE_FROM {
            speed -= (distance - FLEE_EASE_FROM) / FLEE_EASE_FROM;
        }
        if speed <= 0.0 {
            return;
        }

        (self.hooks.set_movement_vector)(mob, away.normalize() * speed / FLEE_PUSH_DIVISOR);
    }
}

/// Builds the hooks for one concrete squid type.
///
/// Each squid calls this with itself, so the downcasts inside are the only
/// place the concrete type is named.
pub(super) fn hooks_for<S>() -> SquidHooks
where
    S: SquidLike + steel_utils::DowncastType + 'static,
{
    SquidHooks {
        set_movement_vector: |mob, vector| {
            if let Some(squid) = mob.downcast_ref::<S>() {
                squid.set_movement_vector(vector);
            }
        },
        has_movement_vector: |mob| {
            mob.downcast_ref::<S>()
                .is_some_and(SquidLike::has_movement_vector)
        },
        is_in_water: |mob| mob.is_in_water(),
    }
}

/// What a concrete squid exposes to the shared goals.
pub(super) trait SquidLike {
    /// Points the next tentacle beat somewhere.
    fn set_movement_vector(&self, movement_vector: DVec3);

    /// Returns whether the squid has a direction to throw itself in.
    fn has_movement_vector(&self) -> bool;
}
