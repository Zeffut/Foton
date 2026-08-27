//! Guardian entity.
//!
//! Vanilla parity: `Guardian`. Everything a guardian does that the elder does
//! too lives in [`super::guardian_common`]; this is the ordinary guardian's own
//! shape, sounds and attack duration.
//!
//! **Gap**: `Guardian.checkSpawnObstruction` drops the "no liquid in the
//! bounding box" half of `Mob.checkSpawnObstruction`, which is what lets a
//! guardian spawn inside the sea at all. Steel's spawn path has no
//! obstruction hook to override yet.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::GuardianEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use super::guardian_common::{
    self, AMBIENT_SOUND_INTERVAL, ATTACK_TIME, GuardianLike, GuardianState, MAX_HEAD_X_ROT,
    STROLL_INTERVAL_TICKS, XP_REWARD,
};
use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::mob::NavigationKind;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::physics::MoveResult;
use crate::world::World;

/// A guardian.
#[entity_behavior(class = "Guardian")]
pub struct GuardianEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<GuardianEntityData>,
    state: SyncMutex<GuardianState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `GuardianEntity`.
unsafe impl DowncastType for GuardianEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/guardian");
}

impl GuardianEntity {
    /// Creates a guardian at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a guardian from saved base data.
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
        let mut entity_data = GuardianEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        // Vanilla parity: the `setPathfindingMalus(PathType.WATER, 0.0F)` of the
        // constructor, which stops the swim path from charging for water.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);

        guardian_common::register_goals::<Self>(&mob_base, STROLL_INTERVAL_TICKS);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(GuardianState::default()),
        }
    }

    /// Returns whether this guardian is beaming something.
    ///
    /// Vanilla parity: `Guardian.hasActiveAttackTarget`.
    #[must_use]
    pub fn has_active_attack_target(&self) -> bool {
        self.active_attack_target_id() != 0
    }
}

impl GuardianLike for GuardianEntity {
    fn guardian_state(&self) -> &SyncMutex<GuardianState> {
        &self.state
    }

    fn is_elder(&self) -> bool {
        false
    }

    fn attack_duration(&self) -> i32 {
        ATTACK_TIME
    }

    fn is_moving(&self) -> bool {
        *self.entity_data.lock().guardian().id_moving.get()
    }

    fn set_moving(&self, moving: bool) {
        self.entity_data.lock().guardian_mut().id_moving.set(moving);
    }

    fn active_attack_target_id(&self) -> i32 {
        *self.entity_data.lock().guardian().id_attack_target.get()
    }

    fn set_active_attack_target(&self, entity_id: i32) {
        self.entity_data
            .lock()
            .guardian_mut()
            .id_attack_target
            .set(entity_id);
    }
}

impl Entity for GuardianEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Guardian.getMovementEmission`.
    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    /// Vanilla parity: `Guardian` inherits `Mob.addAdditionalSaveData`
    /// unchanged, so the shared half is the whole of it.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for GuardianEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: the server half of `Guardian.aiStep`, which runs before
    /// the base step.
    fn ai_step(&self) -> Option<MoveResult> {
        guardian_common::ai_step(self);
        self.default_ai_step()
    }

    /// Vanilla parity: `Guardian.travelInWater`.
    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        guardian_common::travel_in_water(self, input)
    }

    /// Vanilla parity: the thorns half of `Guardian.hurtServer`.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        guardian_common::on_hurt(self, world, source);
        self.living_hurt_server(world, source, amount)
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
        Some(guardian_common::sound_in_or_out_of_water(
            self,
            &sound_events::ENTITY_GUARDIAN_HURT,
            &sound_events::ENTITY_GUARDIAN_HURT_LAND,
        ))
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(guardian_common::sound_in_or_out_of_water(
            self,
            &sound_events::ENTITY_GUARDIAN_DEATH,
            &sound_events::ENTITY_GUARDIAN_DEATH_LAND,
        ))
    }
}

impl Mob for GuardianEntity {
    /// Vanilla parity: `Guardian` derives from `Monster`.
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

    /// Vanilla parity: `Guardian` installs a `GuardianMoveControl`.
    fn tick_move_control(&self) {
        guardian_common::tick_move_control(self);
    }

    /// Vanilla parity: `Guardian.getMaxHeadXRot`.
    fn max_head_x_rot(&self) -> f32 {
        MAX_HEAD_X_ROT
    }

    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(guardian_common::sound_in_or_out_of_water(
            self,
            &sound_events::ENTITY_GUARDIAN_AMBIENT,
            &sound_events::ENTITY_GUARDIAN_AMBIENT_LAND,
        ))
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        guardian_common::check_spawn_rules(world, spawn_reason, pos)
    }
}

impl PathfinderMob for GuardianEntity {
    /// Vanilla parity: `Guardian.createNavigation`, a `WaterBoundPathNavigation`
    /// that never breaches.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }

    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        guardian_common::walk_target_value(self, pos)
    }
}

impl Enemy for GuardianEntity {}

#[cfg(test)]
mod tests;
