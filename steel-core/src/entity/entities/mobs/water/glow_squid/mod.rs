//! Glow squid entity.
//!
//! Vanilla parity: `GlowSquid`, which extends `Squid` and changes four sounds,
//! one particle, and the dark ticks it counts after being hurt. Everything else
//! comes from [`super::squid_common`], which is what makes this file short.
//!
//! The dark ticks are the whole point of the mob: hurt one and it stops
//! glowing for five seconds, so the light a cave gets from a shoal of them is
//! something a player can put out.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::GlowSquidEntityData;
use steel_registry::{sound_events, vanilla_particle_types};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use super::squid_common::{
    self, AMBIENT_SOUND_INTERVAL, BABY_SPAWN_CHANCE, SquidFleeGoal, SquidLike,
    SquidRandomMovementGoal, SquidState,
};
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_surface_water_animal_spawn_rules;
use crate::entity::{
    AgeableMob, AgeableMobBase, AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad,
    EntityMovementEmission, EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::physics::{MoveResult, MoverType};
use crate::world::World;

/// Baby glow squid hitbox.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new(0.5, 0.5, 0.37);

/// Volume a glow squid makes noise at.
const SOUND_VOLUME: f32 = 0.4;

/// Vanilla parity: `Squid.getDefaultGravity`.
const GRAVITY: f64 = 0.08;

/// Ticks a glow squid stays dark after being hurt.
///
/// Vanilla parity: the `setDarkTicks(100)` of `GlowSquid.hurtServer`.
const DARK_TICKS_ON_HURT: i32 = 100;

/// A glow squid.
#[entity_behavior(class = "GlowSquid")]
pub struct GlowSquidEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    entity_data: SyncMutex<GlowSquidEntityData>,
    state: SyncMutex<SquidState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `GlowSquidEntity`.
unsafe impl DowncastType for GlowSquidEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/glow_squid");
}

impl GlowSquidEntity {
    /// Creates a glow squid at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a glow squid from saved base data.
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
        let mut entity_data = GlowSquidEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
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

    /// Returns how much longer this squid stays dark.
    #[must_use]
    pub fn dark_ticks_remaining(&self) -> i32 {
        *self.entity_data.lock().dark_ticks_remaining.get()
    }

    fn set_dark_ticks(&self, ticks: i32) {
        self.entity_data.lock().dark_ticks_remaining.set(ticks);
    }
}

impl Entity for GlowSquidEntity {
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

    fn get_default_gravity(&self) -> f64 {
        GRAVITY
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            BABY_DIMENSIONS.scale(scale)
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        nbt.insert("DarkTicksRemaining", self.dark_ticks_remaining());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.set_dark_ticks(nbt.int("DarkTicksRemaining").unwrap_or(0));
    }
}

impl LivingEntity for GlowSquidEntity {
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
        SOUND_VOLUME
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_GLOW_SQUID_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_GLOW_SQUID_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Inks, and goes dark for five seconds.
    ///
    /// Vanilla parity: `GlowSquid.hurtServer` on top of `Squid.hurtServer`.
    /// Going dark is what makes a shoal of these a light source a player can
    /// switch off.
    fn before_actually_hurt(&self, source: &DamageSource, _amount: f32) {
        self.set_dark_ticks(DARK_TICKS_ON_HURT);

        if source.causing_entity_id.is_none() {
            return;
        }
        if let Some(world) = self.level() {
            squid_common::spawn_ink(
                self,
                &world,
                &sound_events::ENTITY_GLOW_SQUID_SQUIRT,
                &vanilla_particle_types::GLOW_SQUID_INK,
            );
        }
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        squid_common::tick_tentacles(self, &self.state);
        if self.is_in_water() {
            squid_common::face_travel_direction(self);
        }

        // Vanilla parity: `GlowSquid.aiStep` counts the darkness down.
        let dark = self.dark_ticks_remaining();
        if dark > 0 {
            self.set_dark_ticks(dark - 1);
        }

        AgeableMob::tick_ageable_mob(self);
        result
    }

    /// Vanilla parity: `Squid.travel`, which throws away the travel input.
    fn travel(&self, _input: DVec3) -> Option<MoveResult> {
        self.move_entity(MoverType::SelfMovement, self.velocity())
    }
}

impl SquidLike for GlowSquidEntity {
    fn set_movement_vector(&self, movement_vector: DVec3) {
        self.state.lock().movement_vector = movement_vector;
    }

    fn has_movement_vector(&self) -> bool {
        self.state.lock().movement_vector.length_squared() > squid_common::MOVEMENT_EPSILON
    }
}

impl AgeableMob for GlowSquidEntity {
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

impl Mob for GlowSquidEntity {
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
        Some(&sound_events::ENTITY_GLOW_SQUID_AMBIENT)
    }

    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    fn base_experience_reward_mob(&self) -> i32 {
        1 + rand::random_range(0..3)
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `GlowSquid.checkGlowSquidSpawnRules` adds a darkness
    /// test to the surface water rule, which is why they gather in flooded
    /// caves rather than in open sea.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        use crate::world::LevelReader as _;
        check_surface_water_animal_spawn_rules(world, pos)
            && world.max_local_raw_brightness(pos, world.sky_darkening()) == 0
    }

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

impl PathfinderMob for GlowSquidEntity {}
