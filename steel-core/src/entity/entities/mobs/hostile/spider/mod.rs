//! Spider entity.
//!
//! Vanilla parity: `Spider`. A spider leaps at its target, climbs walls when it
//! bumps into them, and only hunts on its own in the dark.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::SpiderEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::goal::{
    FloatGoal, Goal, GoalControls, HurtByTargetGoal, LeapAtTargetGoal, LookAtPlayerGoal,
    MeleeAttackGoal, NearestAttackableTargetGoal, RandomLookAroundGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::world::{LevelReader as _, World};
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

/// Speed multiplier while wandering.
const STROLL_SPEED_MODIFIER: f64 = 0.8;

/// Distance at which a spider watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Brightness at which a spider stops looking for new prey.
///
/// Vanilla parity: the `>= 0.5F` of `Spider.SpiderTargetGoal.canUse`, which on
/// the surface means roughly light level twelve. It is why a spider caught out
/// at dawn keeps chasing what it already has but takes nothing new.
const SPIDER_HOSTILE_BRIGHTNESS: f32 = 0.5;

/// Picks targets only out of the light.
///
/// Vanilla parity: `Spider.SpiderTargetGoal`. It wraps the ordinary target
/// search and refuses to start one in daylight; a spider that already has a
/// target keeps it, which is what makes a daytime spider harmless until
/// provoked.
pub(super) struct SpiderTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl SpiderTargetGoal {
    /// Creates the daylight-gated target search for a spider.
    pub(super) fn new_for_players() -> Self {
        Self {
            inner: NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
        }
    }
}

impl Goal for SpiderTargetGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        if world.light_level_dependent_magic_value(mob.block_position())
            >= SPIDER_HOSTILE_BRIGHTNESS
        {
            return false;
        }
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }
}

/// A spider.
#[entity_behavior(class = "Spider")]
pub struct SpiderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SpiderEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SpiderEntity`.
unsafe impl DowncastType for SpiderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/spider");
}

impl SpiderEntity {
    /// Creates a spider at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a spider from saved base data.
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
        let mut entity_data = SpiderEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla Spider goal priorities in the same order.
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
        *self.entity_data.lock().flags.get() & CLIMBING_FLAG != 0
    }

    /// Marks the spider as climbing or not.
    ///
    /// Vanilla parity: `Spider.setClimbing`.
    fn set_climbing(&self, climbing: bool) {
        let mut data = self.entity_data.lock();
        let flags = *data.flags.get();
        let updated = if climbing {
            flags | CLIMBING_FLAG
        } else {
            flags & !CLIMBING_FLAG
        };
        data.flags.set(updated);
    }
}

impl Entity for SpiderEntity {
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

impl LivingEntity for SpiderEntity {
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

impl Mob for SpiderEntity {
    /// Vanilla parity: `Spider` derives from `Monster`.
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

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SpiderEntity {}

impl Enemy for SpiderEntity {}
