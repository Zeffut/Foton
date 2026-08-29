//! Zoglin entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.Zoglin`. What a hoglin
//! becomes once it has stood in the overworld for fifteen seconds: it keeps the
//! charge, loses the fear of piglins and the taste for crimson fungus, and
//! attacks essentially everything.
//!
//! **Deviation from the brief**: the brief called the zoglin goal-driven. In
//! 26.2 it is not -- `Zoglin` carries a `Brain.Provider<Zoglin>` with the
//! nearest-entity and nearest-player sensors, a `getActivities` returning
//! core/idle/fight, and a `customServerAiStep` that ticks the brain and calls
//! `updateActivity`. It has no `registerGoals` at all. The source wins, so this
//! is brain-driven.

mod zoglin_ai;

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::ZoglinEntityData;
use foton_registry::{sound_events, vanilla_attributes};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::ai::brain::behavior::utils;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::{Activity, Brain};
use crate::entity::damage::DamageSource;
use crate::entity::hoglin_base::{
    self, ATTACK_ANIMATION_DURATION, HoglinBase, PROBABILITY_OF_SPAWNING_AS_BABY,
};
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MoveResult, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::world::World;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{EntityAttachmentPoint, EntityAttachments, EntityDimensions};

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Zoglin` constructor.
const XP_REWARD: i32 = 5;

/// Attack damage a baby zoglin has.
///
/// Vanilla parity: `Zoglin.BABY_ATTACK_DAMAGE`.
const BABY_ATTACK_DAMAGE: f64 = 0.5;

/// How long a zoglin keeps the target that hit it.
///
/// Vanilla parity: the `200L` expiry of `Zoglin.setAttackTarget`.
const ATTACK_TARGET_DURATION: i64 = 200;

/// Vanilla parity: the `EntityAttachment.PASSENGER` of `Zoglin.BABY_DIMENSIONS`.
const BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.875, 0.0)];

/// Vanilla parity: `Zoglin.BABY_DIMENSIONS`, the same box a baby hoglin has.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.75,
    0.85,
    0.625,
    EntityAttachments::new(&BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Fields a zoglin keeps that are neither synced nor on a base.
struct ZoglinState {
    /// Vanilla parity: `Zoglin.attackAnimationRemainingTicks`.
    attack_animation_remaining_ticks: i32,
}

/// A zoglin.
#[entity_behavior(class = "Zoglin")]
pub struct ZoglinEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<ZoglinEntityData>,
    state: SyncMutex<ZoglinState>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ZoglinEntity`.
unsafe impl DowncastType for ZoglinEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/zoglin");
}

impl ZoglinEntity {
    /// Creates a zoglin at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a zoglin from saved base data.
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
        mob_base.set_xp_reward(XP_REWARD);
        let mut entity_data = ZoglinEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(ZoglinState {
                attack_animation_remaining_ticks: 0,
            }),
            brain: zoglin_ai::make_brain(),
        }
    }

    /// Vanilla parity: `Zoglin.isAdult`.
    #[must_use]
    pub fn is_adult(&self) -> bool {
        !self.is_baby_zoglin()
    }

    fn is_baby_zoglin(&self) -> bool {
        *self.entity_data.lock().baby.get()
    }

    /// Vanilla parity: the private `Zoglin.setAttackTarget`.
    fn set_attack_target(&self, target: &SharedEntity) {
        self.brain
            .erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
        self.brain.set_memory_with_expiry(
            memory_module_types::ATTACK_TARGET,
            utils::remember(target),
            ATTACK_TARGET_DURATION,
        );
    }

    /// Vanilla parity: `Zoglin.updateActivity`.
    fn update_activity(&self) {
        let was_fighting = self.brain.is_active(Activity::Fight);
        self.brain
            .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
        if !was_fighting && self.brain.is_active(Activity::Fight) {
            self.make_sound(Some(&sound_events::ENTITY_ZOGLIN_ANGRY));
        }
        Mob::set_aggressive(
            self,
            self.brain
                .has_memory_value(memory_module_types::ATTACK_TARGET.id()),
        );
    }
}

impl Entity for ZoglinEntity {
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

    /// Vanilla parity: `Zoglin.getDefaultDimensions`.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if self.is_baby_zoglin() {
            BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Zoglin.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_ZOGLIN_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("IsBaby", self.is_baby_zoglin());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_baby(nbt.byte("IsBaby").unwrap_or(0) != 0);
        self.brain.load(nbt);
    }
}

impl LivingEntity for ZoglinEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: the `Mob.serverAiStep` a zoglin inherits, which is the
    /// only path to [`Mob::custom_server_ai_step`] and so to the brain.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Zoglin.aiStep`, which runs down the attack animation.
    fn ai_step(&self) -> Option<MoveResult> {
        {
            let mut state = self.state.lock();
            if state.attack_animation_remaining_ticks > 0 {
                state.attack_animation_remaining_ticks -= 1;
            }
        }
        self.default_ai_step()
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

    fn is_baby(&self) -> bool {
        self.is_baby_zoglin()
    }

    /// Vanilla parity: `Zoglin.hurtServer`, which turns on whoever hit it as
    /// long as the new attacker is not much further off than the current one.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        if !was_hurt {
            return false;
        }
        let Some(attacker) = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
        else {
            return true;
        };
        let Some(living_attacker) = attacker.as_living_entity() else {
            return true;
        };
        if Mob::can_attack(self, living_attacker)
            && !utils::is_other_target_much_further_away_than_current_attack_target(
                &self.brain,
                self,
                attacker.as_ref(),
                4.0,
            )
        {
            self.set_attack_target(&attacker);
        }
        true
    }

    /// Vanilla parity: `Zoglin.blockedByItem`, the same throw a hoglin lands on
    /// a blocker.
    fn blocked_by_item(&self, defender: &dyn LivingEntity) {
        // Vanilla's override does not call super either.
        if self.is_baby_zoglin() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        let Some(defender_entity) = world.get_entity_by_id(defender.id()) else {
            return;
        };
        hoglin_base::throw_target(self, &defender_entity);
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOGLIN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOGLIN_DEATH)
    }
}

impl Mob for ZoglinEntity {
    /// Vanilla parity: `Zoglin.setBaby`, which halves the damage on the way in
    /// and -- like vanilla -- never puts it back if the flag is cleared.
    fn set_baby(&self, baby: bool) {
        self.entity_data.lock().baby.set(baby);
        if baby {
            self.attributes()
                .lock()
                .set_base_value(vanilla_attributes::ATTACK_DAMAGE, BABY_ATTACK_DAMAGE);
        }
        self.refresh_dimensions();
    }

    /// Vanilla parity: `Zoglin` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Zoglin.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        self.update_activity();
    }

    /// Vanilla parity: `Zoglin.doHurtTarget`, the same gore-and-throw a hoglin
    /// lands.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        if target.as_living_entity().is_none() {
            return false;
        }
        self.set_attack_animation_remaining_ticks(ATTACK_ANIMATION_DURATION);
        self.broadcast_entity_event(EntityStatus::StartAttacking);
        self.make_sound(Some(&sound_events::ENTITY_ZOGLIN_ATTACK));
        hoglin_base::hurt_and_throw_target(world, self, target)
    }

    /// Vanilla parity: `Zoglin.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(
            if self
                .brain
                .has_memory_value(memory_module_types::ATTACK_TARGET.id())
            {
                &sound_events::ENTITY_ZOGLIN_ANGRY
            } else {
                &sound_events::ENTITY_ZOGLIN_AMBIENT
            },
        )
    }

    /// Vanilla parity: `Zoglin.finalizeSpawn`, one in five of which is a baby.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if rand::random::<f32>() < PROBABILITY_OF_SPAWNING_AS_BABY {
            self.set_baby(true);
        }
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for ZoglinEntity {}

impl HoglinBase for ZoglinEntity {
    fn attack_animation_remaining_ticks(&self) -> i32 {
        self.state.lock().attack_animation_remaining_ticks
    }

    fn set_attack_animation_remaining_ticks(&self, ticks: i32) {
        self.state.lock().attack_animation_remaining_ticks = ticks;
    }
}

impl Enemy for ZoglinEntity {}
