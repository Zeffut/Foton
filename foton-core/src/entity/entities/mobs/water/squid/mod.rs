//! Squid entity.
//!
//! Vanilla parity: `Squid` and `AgeableWaterCreature`. A squid does not swim
//! toward anything: its tentacles beat on a cycle, and once per beat it throws
//! itself along whatever direction it last picked. That is why it drifts in
//! pulses rather than gliding like a fish.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;
use foton_registry::vanilla_entity_data::SquidEntityData;
use foton_registry::vanilla_particle_types;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use super::squid_common::{
    self, AMBIENT_SOUND_INTERVAL, BABY_SPAWN_CHANCE, SquidFleeGoal, SquidLike,
    SquidRandomMovementGoal, SquidState,
};
use crate::entity::LivingEntitySyncedData;
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_surface_water_animal_spawn_rules;
use crate::entity::{
    AgeableMob, AgeableMobBase, AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad,
    EntityMovementEmission, EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::physics::{MoveResult, MoverType};
use crate::world::World;

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

// SAFETY: This key is owned by Foton and uniquely identifies `SquidEntity`.
unsafe impl DowncastType for SquidEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/squid");
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
            let hooks = squid_common::hooks_for::<Self>();
            goals.add_goal(0, SquidRandomMovementGoal::new(hooks));
            goals.add_goal(1, SquidFleeGoal::new(hooks));
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(SquidState::new()),
        }
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
            squid_common::handle_air_supply(self, &world, air_before_tick);
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
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

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

    /// Vanilla parity: `Squid.hurtServer`, which inks only when something
    /// actually attacked it, not when it drowns or starves.
    ///
    /// Vanilla inks after the damage lands and reads `getLastHurtByMob`;
    /// Foton has no post-damage hook, so this runs just before, reading the
    /// damage source instead. Both come to the same thing: ink for a hit that
    /// has an attacker behind it and got past the invulnerability window.
    fn before_actually_hurt(&self, source: &DamageSource, _amount: f32) {
        if source.causing_entity_id.is_none() {
            return;
        }
        if let Some(world) = self.level() {
            squid_common::spawn_ink(
                self,
                &world,
                &sound_events::ENTITY_SQUID_SQUIRT,
                &vanilla_particle_types::SQUID_INK,
            );
        }
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        squid_common::tick_tentacles(self, &self.state);
        if self.is_in_water() {
            squid_common::face_travel_direction(self);
        }
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

impl SquidLike for SquidEntity {
    fn set_movement_vector(&self, movement_vector: DVec3) {
        self.state.lock().movement_vector = movement_vector;
    }

    fn has_movement_vector(&self) -> bool {
        self.state.lock().movement_vector.length_squared() > squid_common::MOVEMENT_EPSILON
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
    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `WaterAnimal::checkSurfaceWaterAnimalSpawnRules`,
    /// which keeps it in the top thirteen blocks of the sea.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_surface_water_animal_spawn_rules(world, pos)
    }

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
