//! Elder guardian entity.
//!
//! Vanilla parity: `ElderGuardian`. It is a guardian with a bigger beam, a
//! slower wander and one thing entirely its own: every minute it lays mining
//! fatigue on every player within fifty blocks, which is what an ocean monument
//! feels like from the inside.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::{CGameEvent, GameEventType, SoundSource};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::GuardianEntityData;
use steel_registry::{sound_events, vanilla_mob_effects};
use steel_utils::locks::SyncMutex;
use steel_utils::types::GameType;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use super::guardian_common::{
    self, AMBIENT_SOUND_INTERVAL, GuardianLike, GuardianState, MAX_HEAD_X_ROT, XP_REWARD,
};
use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::living_base::MobEffectInstance;
use crate::entity::mob::NavigationKind;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::physics::MoveResult;
use crate::world::World;

/// Ticks an elder guardian's beam charges for.
///
/// Vanilla parity: `ElderGuardian.getAttackDuration`, three quarters of the
/// ordinary guardian's.
const ATTACK_DURATION: i32 = 60;

/// Ticks between two rounds of mining fatigue.
///
/// Vanilla parity: `ElderGuardian.EFFECT_INTERVAL`.
const EFFECT_INTERVAL: i32 = 1200;

/// How far the mining fatigue reaches, in blocks.
///
/// Vanilla parity: `ElderGuardian.EFFECT_RADIUS`.
const EFFECT_RADIUS: f64 = 50.0;

/// How long the mining fatigue lasts, in ticks.
///
/// Vanilla parity: `ElderGuardian.EFFECT_DURATION`.
const EFFECT_DURATION: i32 = 6000;

/// Strength of the mining fatigue.
///
/// Vanilla parity: `ElderGuardian.EFFECT_AMPLIFIER`, which is mining fatigue
/// III.
const EFFECT_AMPLIFIER: i32 = 2;

/// How little of the effect may be left before it is laid on again.
///
/// Vanilla parity: `ElderGuardian.EFFECT_DISPLAY_LIMIT`, the `displayEffectLimit`
/// of `MobEffectUtil.addEffectToPlayersAround`.
const EFFECT_DISPLAY_LIMIT: i32 = 1200;

/// Ticks between two wander rolls.
///
/// Vanilla parity: the `this.randomStrollGoal.setInterval(400)` of the
/// constructor.
const STROLL_INTERVAL_TICKS: i32 = 400;

/// How far from where it woke up an elder guardian will stray.
///
/// Vanilla parity: the `setHomeTo(this.blockPosition(), 16)` of
/// `ElderGuardian.customServerAiStep`.
const HOME_RADIUS: i32 = 16;

/// An elder guardian.
#[entity_behavior(class = "ElderGuardian")]
pub struct ElderGuardianEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<GuardianEntityData>,
    state: SyncMutex<GuardianState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ElderGuardianEntity`.
unsafe impl DowncastType for ElderGuardianEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/elder_guardian");
}

impl ElderGuardianEntity {
    /// Creates an elder guardian at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an elder guardian from saved base data.
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
        // Vanilla parity: the `setPersistenceRequired()` of the constructor.
        // Monuments would empty out otherwise.
        *mob_base.persistence_required().lock() = true;

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

    /// Lays mining fatigue on every player in range, once a minute.
    ///
    /// Vanilla parity: `ElderGuardian.customServerAiStep` together with
    /// `MobEffectUtil.addEffectToPlayersAround`. A player who already has the
    /// effect at full strength and plenty of time left is skipped, so the
    /// screen effect only replays when it is about to matter.
    fn apply_mining_fatigue(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let position = self.position();
        let silent = self.is_silent();
        let radius_sqr = EFFECT_RADIUS * EFFECT_RADIUS;

        world.players.iter_players(|_uuid, player| {
            if !matches!(player.game_mode(), GameType::Survival | GameType::Adventure) {
                return true;
            }
            if self.is_allied_to(player.as_ref()) {
                return true;
            }
            if position.distance_squared(player.position()) > radius_sqr {
                return true;
            }

            let needs_effect = player
                .mob_effect(vanilla_mob_effects::MINING_FATIGUE)
                .is_none_or(|active| {
                    active.amplifier() < EFFECT_AMPLIFIER
                        || !active.is_infinite_duration()
                            && active.duration() < EFFECT_DISPLAY_LIMIT
                });
            if !needs_effect {
                return true;
            }

            player.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::MINING_FATIGUE,
                EFFECT_DURATION,
                EFFECT_AMPLIFIER,
            ));
            player.send_packet(CGameEvent {
                event: GameEventType::GuardianElderEffect,
                data: if silent { 0.0 } else { 1.0 },
            });

            true
        });
    }
}

impl GuardianLike for ElderGuardianEntity {
    fn guardian_state(&self) -> &SyncMutex<GuardianState> {
        &self.state
    }

    fn is_elder(&self) -> bool {
        true
    }

    fn attack_duration(&self) -> i32 {
        ATTACK_DURATION
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

impl Entity for ElderGuardianEntity {
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

    /// Vanilla parity: neither `ElderGuardian` nor `Guardian` overrides
    /// `addAdditionalSaveData`, so the shared half is the whole of it.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for ElderGuardianEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        guardian_common::ai_step(self);
        self.default_ai_step()
    }

    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        guardian_common::travel_in_water(self, input)
    }

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
            &sound_events::ENTITY_ELDER_GUARDIAN_HURT,
            &sound_events::ENTITY_ELDER_GUARDIAN_HURT_LAND,
        ))
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(guardian_common::sound_in_or_out_of_water(
            self,
            &sound_events::ENTITY_ELDER_GUARDIAN_DEATH,
            &sound_events::ENTITY_ELDER_GUARDIAN_DEATH_LAND,
        ))
    }
}

impl Mob for ElderGuardianEntity {
    /// Vanilla parity: `ElderGuardian` derives from `Guardian`, and so from
    /// `Monster`.
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

    fn tick_move_control(&self) {
        guardian_common::tick_move_control(self);
    }

    /// Vanilla parity: `ElderGuardian.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        if (self.tick_count() + self.id()) % EFFECT_INTERVAL == 0 {
            self.apply_mining_fatigue();
        }

        if !self.has_home() {
            self.set_home_to(self.block_position(), HOME_RADIUS);
        }
    }

    fn max_head_x_rot(&self) -> f32 {
        MAX_HEAD_X_ROT
    }

    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(guardian_common::sound_in_or_out_of_water(
            self,
            &sound_events::ENTITY_ELDER_GUARDIAN_AMBIENT,
            &sound_events::ENTITY_ELDER_GUARDIAN_AMBIENT_LAND,
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

impl PathfinderMob for ElderGuardianEntity {
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }

    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        guardian_common::walk_target_value(self, pos)
    }
}

impl Enemy for ElderGuardianEntity {}

#[cfg(test)]
mod tests;
