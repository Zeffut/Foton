//! Endermite entity.
//!
//! Vanilla parity: `Endermite`. What an ender pearl leaves behind: a small,
//! fast monster that counts its own lifetime down and vanishes after two
//! minutes unless something has made it permanent.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::EndermiteEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::goal::{
    ClimbOnTopOfPowderSnowGoal, FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_silverfish_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, RemovalReason,
};
use crate::physics::MoveResult;
use crate::world::World;

/// Experience an endermite drops.
///
/// Vanilla parity: the `this.xpReward = 3` of the constructor.
const XP_REWARD: i32 = 3;

/// Ticks an endermite lives before it vanishes.
///
/// Vanilla parity: `Endermite.MAX_LIFE`.
const MAX_LIFE: i32 = 2400;

/// Speed multiplier while chasing.
///
/// Vanilla parity: `new MeleeAttackGoal(this, 1.0, false)`.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Speed multiplier while wandering.
///
/// Vanilla parity: `new WaterAvoidingRandomStrollGoal(this, 1.0)`.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which an endermite watches a player.
///
/// Vanilla parity: `new LookAtPlayerGoal(this, Player.class, 8.0F)`.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Volume of an endermite's footstep.
///
/// Vanilla parity: the `0.15F` of `Endermite.playStepSound`.
const STEP_SOUND_VOLUME: f32 = 0.15;

/// Pitch of an endermite's footstep.
const STEP_SOUND_PITCH: f32 = 1.0;

/// An endermite.
#[entity_behavior(class = "Endermite")]
pub struct EndermiteEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<EndermiteEntityData>,
    /// Ticks this endermite has been alive (vanilla `Endermite.life`).
    life: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EndermiteEntity`.
unsafe impl DowncastType for EndermiteEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/endermite");
}

impl EndermiteEntity {
    /// Creates an endermite at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an endermite from saved base data.
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
        let mut entity_data = EndermiteEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        {
            // Keep vanilla Endermite goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(1, ClimbOnTopOfPowderSnowGoal::new());
            goals.add_goal(2, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(3, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(7, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new().set_alert_others([]));
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            life: SyncMutex::new(0),
        }
    }

    /// Returns how long this endermite has been alive, in ticks.
    #[must_use]
    pub fn life(&self) -> i32 {
        *self.life.lock()
    }

    /// Advances the lifetime and discards the endermite once it runs out.
    ///
    /// Vanilla parity: the server half of `Endermite.aiStep`. A persistent
    /// endermite stops aging entirely rather than aging and being spared at the
    /// end, which is why one summoned with `PersistenceRequired` keeps forever.
    fn tick_life(&self) {
        if !self.is_persistence_required() {
            *self.life.lock() += 1;
        }

        if self.life() >= MAX_LIFE {
            self.set_removed(RemovalReason::Discarded);
        }
    }
}

impl Entity for EndermiteEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Endermite.tick`, which pins the body to the head before
    /// anything else runs; an endermite has no independent body turn.
    fn tick(&self) {
        self.set_y_body_rot(self.rotation().0);
        self.tick_living_entity();
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Endermite.getMovementEmission`.
    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    /// Vanilla parity: `Endermite.playStepSound`, which ignores the block
    /// stepped on.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(
            &sound_events::ENTITY_ENDERMITE_STEP,
            STEP_SOUND_VOLUME,
            STEP_SOUND_PITCH,
        );
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("Lifetime", self.life());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(life) = nbt.int("Lifetime") {
            *self.life.lock() = life;
        }
    }
}

impl LivingEntity for EndermiteEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Endermite.aiStep`, which ages the endermite after the
    /// base step rather than before it.
    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        self.tick_life();
        result
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMITE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMITE_DEATH)
    }

    /// Vanilla parity: `Endermite.setYBodyRot`, which snaps the head to match
    /// whenever the body rotation is forced.
    fn set_y_body_rot(&self, y_body_rot: f32) {
        let (_, pitch) = self.rotation();
        self.set_rotation((y_body_rot, pitch));
        self.living_base().set_y_body_rot(y_body_rot);
    }
}

impl Mob for EndermiteEntity {
    /// Vanilla parity: `Endermite` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
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
        Some(&sound_events::ENTITY_ENDERMITE_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `Endermite.checkEndermiteSpawnRules`, which is character
    /// for character `Silverfish.checkSilverfishSpawnRules`: light is no
    /// obstacle, a player within five blocks is.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_silverfish_spawn_rules(world, spawn_reason, pos)
    }
}

impl PathfinderMob for EndermiteEntity {}

impl Enemy for EndermiteEntity {}

#[cfg(test)]
mod tests;
