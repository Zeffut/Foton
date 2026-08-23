//! Cave spider entity.
//!
//! Vanilla parity: `CaveSpider`. A smaller spider with the same climbing and
//! pouncing behaviour.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::CaveSpiderEntityData;
use steel_registry::{sound_events, vanilla_mob_effects};
use steel_utils::locks::SyncMutex;
use steel_utils::types::Difficulty;
use steel_utils::{DowncastType, DowncastTypeKey};

use super::spider::SpiderTargetGoal;
use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LeapAtTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, MobEffectInstance, PathfinderMob, SharedEntity,
};
use crate::world::World;
use std::sync::Arc;
use steel_utils::BlockPos;

/// Bit of the synced flags byte that marks a climbing spider.
///
/// Vanilla parity: the `1` mask of `Spider.setClimbing`.
const CLIMBING_FLAG: i8 = 1;

/// Upward velocity of a spider's pounce.
///
/// Vanilla parity: `LeapAtTargetGoal(this, 0.4F)`.
const LEAP_HEIGHT: f32 = 0.4;

/// Speed multiplier while chasing.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Seconds of poison a cave spider inflicts on normal difficulty.
const POISON_SECONDS_NORMAL: i32 = 7;

/// Seconds of poison a cave spider inflicts on hard difficulty.
const POISON_SECONDS_HARD: i32 = 15;

/// Speed multiplier while wandering.
const STROLL_SPEED_MODIFIER: f64 = 0.8;

/// Distance at which a spider watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// A cave spider.
#[entity_behavior(class = "CaveSpider")]
pub struct CaveSpiderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CaveSpiderEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CaveSpiderEntity`.
unsafe impl DowncastType for CaveSpiderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/cave_spider");
}

impl CaveSpiderEntity {
    /// Creates a cave spider at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a cave spider from saved base data.
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
        let mut entity_data = CaveSpiderEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // A cave spider keeps the Spider goal set unchanged.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(3, LeapAtTargetGoal::new(LEAP_HEIGHT));
            goals.add_goal(4, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, true));
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(6, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(6, RandomLookAroundGoal::new());
            // TODO: vanilla also avoids armadillos at priority 2.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            targets.add_goal(2, SpiderTargetGoal::new_for_players());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns whether the spider is currently stuck to a wall.
    #[must_use]
    pub fn is_climbing(&self) -> bool {
        *self.entity_data.lock().spider.flags.get() & CLIMBING_FLAG != 0
    }

    /// Marks the spider as climbing or not.
    ///
    /// Vanilla parity: `Spider.setClimbing`.
    fn set_climbing(&self, climbing: bool) {
        let mut data = self.entity_data.lock();
        let flags = *data.spider.flags.get();
        let updated = if climbing {
            flags | CLIMBING_FLAG
        } else {
            flags & !CLIMBING_FLAG
        };
        data.spider.flags.set(updated);
    }
}

impl Entity for CaveSpiderEntity {
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
        // Vanilla parity: a spider clings to whatever it just walked into, which is
        // why walking a spider into a wall is enough to send it up.
        self.set_climbing(self.horizontal_collision());
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Spider.onClimbable`.
    fn on_climbable(&self) -> bool {
        self.is_climbing()
    }
}

impl LivingEntity for CaveSpiderEntity {
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SPIDER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SPIDER_DEATH)
    }
}

impl Mob for CaveSpiderEntity {
    /// Vanilla parity: `CaveSpider` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` this mob is
    /// registered with in `SpawnPlacements`. It only appears in the dark.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
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
        Some(&sound_events::ENTITY_SPIDER_AMBIENT)
    }

    /// Poisons the target after a successful hit.
    ///
    /// Vanilla parity: `CaveSpider.doHurtTarget`. Easy difficulty poisons nothing,
    /// which is why a cave spider is merely annoying on peaceful settings.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        if !Mob::mob_do_hurt_target(self, world, target) {
            return false;
        }
        let Some(living) = target.as_living_entity() else {
            return true;
        };
        let poison_seconds = match world.difficulty() {
            Difficulty::Normal => POISON_SECONDS_NORMAL,
            Difficulty::Hard => POISON_SECONDS_HARD,
            Difficulty::Peaceful | Difficulty::Easy => 0,
        };
        if poison_seconds > 0 {
            living.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::POISON,
                poison_seconds * 20,
                0,
            ));
        }
        true
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for CaveSpiderEntity {}

impl Enemy for CaveSpiderEntity {}
