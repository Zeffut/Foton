//! Squid entity.
//!
//! Vanilla parity: `Squid` and `AgeableWaterCreature`. A squid does not swim
//! toward anything: its tentacles beat on a cycle, and once per beat it throws
//! itself along whatever direction it last picked. That is why it drifts in
//! pulses rather than gliding like a fish.

use std::f32::consts::PI;
use std::f64::consts::TAU;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_damage_types;
use steel_registry::vanilla_entity_data::SquidEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{Goal, GoalControls};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad,
    EntityMovementEmission, EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::physics::{MoveResult, MoverType};
use crate::world::World;
use steel_utils::Downcast as _;

/// Air a squid holds, in ticks.
///
/// Vanilla parity: the `300` of `AgeableWaterCreature.handleAirSupply`.
const SQUID_AIR_SUPPLY: i32 = 300;

/// Damage a suffocating squid takes each tick.
///
/// Vanilla parity: the `2.0F` of the same method.
const SUFFOCATION_DAMAGE: f32 = 2.0;

/// Ticks between two idle sounds.
///
/// Vanilla parity: `AgeableWaterCreature.getAmbientSoundInterval`.
const AMBIENT_SOUND_INTERVAL: i32 = 120;

/// Chance a squid spawns small.
///
/// Vanilla parity: the `AgeableMobGroupData(0.05F)` of `Squid.finalizeSpawn`.
const BABY_SPAWN_CHANCE: f32 = 0.05;

/// Ticks of stillness after which a squid stops pushing itself at all.
///
/// Vanilla parity: the `noActionTime > 100` of `SquidRandomMovementGoal`.
const IDLE_TICKS_BEFORE_DRIFTING: i32 = 100;

/// One chance in this many ticks of picking a new direction.
///
/// Vanilla parity: the `reducedTickDelay(50)` of the same goal.
const RETARGET_INTERVAL_TICKS: i32 = 50;

/// How hard a squid pushes horizontally.
///
/// Vanilla parity: the `0.2F` of the movement vector.
const PUSH_HORIZONTAL: f64 = 0.2;

/// Slowest vertical push a squid gives itself.
///
/// Vanilla parity: the `-0.1F` of the same vector, so a squid tends to sink
/// slightly unless the roll sends it up.
const PUSH_VERTICAL_MIN: f64 = -0.1;

/// Range of the vertical push above the minimum.
///
/// Vanilla parity: the `+ nextFloat() * 0.2F` of the same vector.
const PUSH_VERTICAL_RANGE: f64 = 0.2;

/// Point in the tentacle beat at which the push lands.
///
/// Vanilla parity: the `tentacleScale > 0.75` of `Squid.aiStep`, three quarters
/// of the way through the forward half of the stroke.
const PUSH_POINT_IN_STROKE: f32 = 0.75;

/// Fraction of speed a squid keeps while its tentacles recover.
///
/// Vanilla parity: the `scale(0.9)` of the same method.
const RECOVERY_DRAG: f64 = 0.9;

/// Below this, the movement vector counts as nothing.
///
/// Vanilla parity: the `lengthSqr() > 1.0E-5F` of `Squid.hasMovementVector`.
const MOVEMENT_EPSILON: f64 = 1.0e-5;

/// Volume a squid makes noise at.
///
/// Vanilla parity: `Squid.getSoundVolume`.
const SQUID_SOUND_VOLUME: f32 = 0.4;

/// Vanilla parity: `Squid.getDefaultGravity`.
const SQUID_GRAVITY: f64 = 0.08;

/// Baby squid hitbox.
///
/// Vanilla parity: `Squid.BABY_DIMENSIONS`.
const SQUID_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new(0.5, 0.5, 0.37);

/// State a squid keeps to itself.
struct SquidState {
    /// Where the next tentacle beat will throw it.
    ///
    /// Vanilla parity: `Squid.movementVector`.
    movement_vector: DVec3,
    /// How far through the tentacle beat the squid is, in radians.
    ///
    /// Vanilla parity: `Squid.tentacleMovement`.
    tentacle_movement: f32,
    /// How fast the beat advances.
    ///
    /// Vanilla parity: `Squid.tentacleSpeed`.
    tentacle_speed: f32,
}

/// A squid.
#[entity_behavior(class = "Squid")]
pub struct SquidEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    entity_data: SyncMutex<SquidEntityData>,
    state: SyncMutex<SquidState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SquidEntity`.
unsafe impl DowncastType for SquidEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/squid");
}

/// Rolls the speed of one tentacle beat.
///
/// Vanilla parity: `1.0F / (nextFloat() + 1.0F) * 0.2F`, so a squid that draws
/// a low number beats slowly for a while.
fn roll_tentacle_speed() -> f32 {
    (rand::random::<f32>() + 1.0).recip() * 0.2
}

impl SquidEntity {
    /// Creates a squid at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a squid from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        let ageable_base = AgeableMobBase::new();
        let mut entity_data = SquidEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `Squid.registerGoals`. The flee goal is not ported;
            // see the module TODO.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, SquidRandomMovementGoal);
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(SquidState {
                movement_vector: DVec3::ZERO,
                tentacle_movement: 0.0,
                tentacle_speed: roll_tentacle_speed(),
            }),
        }
    }

    /// Returns whether the squid has a direction to throw itself in.
    ///
    /// Vanilla parity: `Squid.hasMovementVector`.
    #[must_use]
    pub fn has_movement_vector(&self) -> bool {
        self.state.lock().movement_vector.length_squared() > MOVEMENT_EPSILON
    }

    /// Points the next tentacle beat somewhere.
    fn set_movement_vector(&self, movement_vector: DVec3) {
        self.state.lock().movement_vector = movement_vector;
    }

    /// Advances the tentacle beat and pushes the squid when it lands.
    ///
    /// Vanilla parity: the in-water branch of `Squid.aiStep`.
    fn tick_tentacles(&self) {
        let (stroke, movement_vector) = {
            let mut state = self.state.lock();
            state.tentacle_movement += state.tentacle_speed;
            if state.tentacle_movement > TAU as f32 {
                state.tentacle_movement -= TAU as f32;
                if rand::random_range(0..10) == 0 {
                    state.tentacle_speed = roll_tentacle_speed();
                }
            }
            (state.tentacle_movement, state.movement_vector)
        };

        if !self.is_in_water() {
            return;
        }

        if stroke < PI {
            // The forward half of the stroke: the push lands three quarters
            // through it, which is what makes a squid lurch rather than glide.
            if stroke / PI > PUSH_POINT_IN_STROKE {
                self.set_velocity(movement_vector);
            }
        } else {
            self.set_velocity(self.velocity() * RECOVERY_DRAG);
        }
    }

    /// Drains air out of water and refills it in.
    ///
    /// Vanilla parity: `AgeableWaterCreature.handleAirSupply`.
    fn handle_air_supply(&self, world: &World, air_before_tick: i32) {
        if Entity::is_alive(self) && !self.is_in_water() {
            self.set_air_supply(air_before_tick - 1);
            if self.should_take_drowning_damage() {
                self.set_air_supply(0);
                self.hurt_server(
                    world,
                    &DamageSource::environment(&vanilla_damage_types::DROWN),
                    SUFFOCATION_DAMAGE,
                );
            }
        } else {
            self.set_air_supply(SQUID_AIR_SUPPLY);
        }
    }
}

/// Picks a direction for the next tentacle beat.
///
/// Vanilla parity: `Squid.SquidRandomMovementGoal`.
struct SquidRandomMovementGoal;

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
        let Some(squid) = mob.downcast_ref::<SquidEntity>() else {
            return;
        };

        // Vanilla parity: a squid nobody has bothered for five seconds stops
        // pushing itself and simply drifts.
        if mob.no_action_time() > IDLE_TICKS_BEFORE_DRIFTING {
            squid.set_movement_vector(DVec3::ZERO);
            return;
        }

        let needs_direction = rand::random_range(0..RETARGET_INTERVAL_TICKS) == 0
            || !squid.is_in_water()
            || !squid.has_movement_vector();
        if !needs_direction {
            return;
        }

        let angle = rand::random::<f64>() * TAU;
        squid.set_movement_vector(DVec3::new(
            angle.cos() * PUSH_HORIZONTAL,
            rand::random::<f64>().mul_add(PUSH_VERTICAL_RANGE, PUSH_VERTICAL_MIN),
            angle.sin() * PUSH_HORIZONTAL,
        ));
    }
}

impl Entity for SquidEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `AgeableWaterCreature.baseTick`, which reads the air left
    /// before the shared tick spends it.
    fn base_tick(&self) {
        let air_before_tick = self.air_supply();
        Mob::base_tick_mob(self);
        if let Some(world) = self.level() {
            self.handle_air_supply(&world, air_before_tick);
        }
    }

    /// Vanilla parity: `Squid.getDefaultGravity`.
    fn get_default_gravity(&self) -> f64 {
        SQUID_GRAVITY
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            SQUID_BABY_DIMENSIONS.scale(scale)
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `Squid.getMovementEmission`.
    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    /// Vanilla parity: `Squid.playStepSound` does not exist; a squid has no feet.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
    }
}

impl LivingEntity for SquidEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn sound_volume(&self) -> f32 {
        SQUID_SOUND_VOLUME
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SQUID_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SQUID_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        self.tick_tentacles();
        AgeableMob::tick_ageable_mob(self);
        result
    }

    /// Moves on nothing but its own momentum.
    ///
    /// Vanilla parity: `Squid.travel`, which throws away the travel input
    /// entirely: a squid is carried by the shove its tentacles gave it, not by
    /// anything the AI asks for this tick.
    fn travel(&self, _input: DVec3) -> Option<MoveResult> {
        self.move_entity(MoverType::SelfMovement, self.velocity())
    }
}

impl AgeableMob for SquidEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }

    fn age_boundary_changed(&self, _baby: bool) {
        self.refresh_dimensions();
    }
}

impl Mob for SquidEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SQUID_AMBIENT)
    }

    /// Vanilla parity: `AgeableWaterCreature.getAmbientSoundInterval`.
    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AgeableWaterCreature.getBaseExperienceReward`.
    fn base_experience_reward_mob(&self) -> i32 {
        1 + rand::random_range(0..3)
    }

    /// Vanilla parity: `Squid.finalizeSpawn`, which passes the one-in-twenty
    /// baby chance down to the shared ageable roll.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_baby_spawn_chance(BABY_SPAWN_CHANCE),
        ));
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SquidEntity {}
