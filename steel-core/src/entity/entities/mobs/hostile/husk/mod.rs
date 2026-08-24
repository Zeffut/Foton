//! Husk entity.
//!
//! Vanilla parity: `Husk`. A desert zombie with the same behaviour, except that
//! it is absent from the `burn_in_daylight` tag, so it survives the morning
//! without any code of its own.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::HuskEntityData;
use steel_registry::{sound_events, vanilla_mob_effects};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::ai::goal::{
    HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_surface_monster_spawn_rules;
use crate::entity::{
    AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob, SharedEntity,
    SpawnGroupData,
};
use crate::world::World;
use steel_utils::BlockPos;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Speed multiplier the zombie uses while chasing.
///
/// Vanilla parity: the `ZombieAttackGoal(this, 1.0, false)` entry.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which a zombie turns to watch a player.
///
/// Vanilla parity: `LookAtPlayerGoal(this, Player.class, 8.0F)`.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Hunger ticks a husk inflicts per whole point of local difficulty.
///
/// Vanilla parity: the `140 * (int) difficulty` of `Husk.doHurtTarget`, which
/// reads the scaled local difficulty rather than the level setting.
const HUNGER_TICKS_PER_DIFFICULTY: i32 = 140;

/// Speed multiplier for aimless wandering.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// A husk.
#[entity_behavior(class = "Husk")]
pub struct HuskEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<HuskEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `HuskEntity`.
unsafe impl DowncastType for HuskEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/husk");
}

impl HuskEntity {
    /// Creates a husk at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a husk from saved base data.
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
        let mut entity_data = HuskEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla Husk goal priorities in the same order. The goals that
            // need systems Steel lacks are listed in the module TODO instead.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(3, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }

        {
            // Vanilla parity: the husk's targetSelector.
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
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
        }
    }

    /// Returns whether this husk is a baby.
    #[must_use]
    pub fn is_baby(&self) -> bool {
        *self.entity_data.lock().zombie.baby.get()
    }

    /// Sets whether this husk is a baby.
    ///
    /// Vanilla parity: `Zombie.setBaby` also swaps in a movement-speed modifier;
    /// Steel only syncs the flag so far.
    pub fn set_baby(&self, baby: bool) {
        self.entity_data.lock().zombie.baby.set(baby);
    }
}

impl Entity for HuskEntity {
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
}

impl LivingEntity for HuskEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    /// Without this the goal selector is never ticked and every goal this mob
    /// registers is dead code.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
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
        Some(&sound_events::ENTITY_HUSK_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_HUSK_DEATH)
    }
}

impl Mob for HuskEntity {
    /// Vanilla parity: `Husk` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `Monster::checkSurfaceMonstersSpawnRules`. A husk needs
    /// open sky above it, which is what keeps it in the desert rather than in
    /// the caves under it.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_surface_monster_spawn_rules(world, spawn_reason, pos)
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
        Some(&sound_events::ENTITY_HUSK_AMBIENT)
    }

    /// Leaves the target hungry after a successful hit.
    ///
    /// Vanilla parity: `Husk.doHurtTarget`, which only applies when the husk is
    /// bare-handed and scales the duration with the difficulty.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        if !Mob::mob_do_hurt_target(self, world, target) {
            return false;
        }
        let Some(living) = target.as_living_entity() else {
            return true;
        };
        let difficulty = world
            .get_current_difficulty_at(self.block_position())
            .effective_difficulty() as i32;
        let hunger_ticks = HUNGER_TICKS_PER_DIFFICULTY * difficulty;
        if hunger_ticks > 0 {
            living.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::HUNGER,
                hunger_ticks,
                0,
            ));
        }
        true
    }

    /// Rolls whether this one spawned small.
    ///
    /// Vanilla parity: `Zombie.finalizeSpawn`, which gives one zombie in twenty
    /// the baby form. Steel's zombies are not `AgeableMob`s, so the shared
    /// group-data roll does not reach them and the chance is applied here.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if rand::random::<f32>() < AgeableMobGroupData::DEFAULT_BABY_SPAWN_CHANCE {
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

impl PathfinderMob for HuskEntity {}

impl Enemy for HuskEntity {}
