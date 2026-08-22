//! Creeper entity.
//!
//! Vanilla parity: `Creeper` and `SwellGoal`. A creeper stalks the player in
//! silence, swells while it is close enough, and detonates when the fuse fills.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::CreeperEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FloatGoal, Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob, RemovalReason,
};
use crate::world::World;
use crate::world::explosion::ExplosionBlockInteraction;

/// Ticks the fuse takes to fill.
///
/// Vanilla parity: `Creeper.maxSwell`.
const MAX_SWELL: i32 = 30;

/// Blast radius of an ordinary creeper.
///
/// Vanilla parity: `Creeper.explosionRadius`.
const EXPLOSION_RADIUS: f32 = 3.0;

/// Squared distance within which a creeper starts swelling.
///
/// Vanilla parity: the `49.0` threshold in `SwellGoal`.
const SWELL_RANGE_SQR: f64 = 49.0;

/// Speed multiplier while chasing.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Speed multiplier while wandering.
const STROLL_SPEED_MODIFIER: f64 = 0.8;

/// Distance at which a creeper watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// A creeper.
#[entity_behavior(class = "Creeper")]
pub struct CreeperEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CreeperEntityData>,
    /// How full the fuse is, from 0 to [`MAX_SWELL`].
    swell: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CreeperEntity`.
unsafe impl DowncastType for CreeperEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/creeper");
}

impl CreeperEntity {
    /// Creates a creeper at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a creeper from saved base data.
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
        let mut entity_data = CreeperEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        entity_data.swell_dir.set(-1);

        {
            // Keep vanilla Creeper goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(2, SwellGoal);
            goals.add_goal(4, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(6, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(6, RandomLookAroundGoal::new());
            // TODO: vanilla also flees ocelots and cats at priority 3.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(
                1,
                NearestAttackableTargetGoal::new_for_players(true, |_, _| true),
            );
            targets.add_goal(2, HurtByTargetGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            swell: SyncMutex::new(0),
        }
    }

    /// Returns the fuse direction: 1 while swelling, -1 while relaxing.
    #[must_use]
    pub fn swell_dir(&self) -> i32 {
        *self.entity_data.lock().swell_dir.get()
    }

    /// Sets the fuse direction.
    pub fn set_swell_dir(&self, dir: i32) {
        self.entity_data.lock().swell_dir.set(dir);
    }

    /// Returns whether this creeper was struck by lightning.
    #[must_use]
    pub fn is_powered(&self) -> bool {
        *self.entity_data.lock().is_powered.get()
    }

    /// Returns whether the creeper was lit by hand and can no longer stop.
    #[must_use]
    pub fn is_ignited(&self) -> bool {
        *self.entity_data.lock().is_ignited.get()
    }

    /// Advances the fuse and detonates when it fills.
    ///
    /// Vanilla parity: the swell block of `Creeper.tick`.
    fn tick_swell(&self) {
        if !Entity::is_alive(self) {
            return;
        }
        if self.is_ignited() {
            self.set_swell_dir(1);
        }

        let dir = self.swell_dir();
        let reached_max = {
            let mut swell = self.swell.lock();
            if dir > 0 && *swell == 0 {
                if let Some(world) = self.level() {
                    world.play_sound_at(
                        &sound_events::ENTITY_CREEPER_PRIMED,
                        SoundSource::Hostile,
                        self.position(),
                        1.0,
                        0.5,
                        None,
                    );
                }
            }

            *swell = (*swell + dir).max(0);
            if *swell >= MAX_SWELL {
                *swell = MAX_SWELL;
                true
            } else {
                false
            }
        };

        if reached_max {
            self.explode();
        }
    }

    /// Detonates and removes the creeper.
    ///
    /// Vanilla parity: `Creeper.explodeCreeper`. A charged creeper doubles its
    /// radius.
    fn explode(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let multiplier = if self.is_powered() { 2.0 } else { 1.0 };
        world.explode(
            Some(self.id()),
            None,
            self.position(),
            EXPLOSION_RADIUS * multiplier,
            false,
            ExplosionBlockInteraction::Destroy,
        );
        // TODO: vanilla also leaves a lingering cloud of whatever effects the
        // creeper carried.
        self.set_removed(RemovalReason::Killed);
    }
}

/// Drives the creeper's fuse from how close its target is.
///
/// Vanilla parity: `SwellGoal`.
struct SwellGoal;

impl Goal for SwellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(creeper) = mob.downcast_ref::<CreeperEntity>() else {
            return false;
        };
        if creeper.swell_dir() > 0 {
            return true;
        }
        mob.target().is_some_and(|target| {
            target.position().distance_squared(mob.position()) < SWELL_RANGE_SQR
        })
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(creeper) = mob.downcast_ref::<CreeperEntity>() else {
            return;
        };
        let close_enough = mob.target().is_some_and(|target| {
            target.position().distance_squared(mob.position()) <= SWELL_RANGE_SQR
        });
        creeper.set_swell_dir(if close_enough { 1 } else { -1 });
    }
}

impl Entity for CreeperEntity {
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
        self.tick_swell();
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }
}

impl LivingEntity for CreeperEntity {
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
        Some(&sound_events::ENTITY_CREEPER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_CREEPER_DEATH)
    }
}

impl Mob for CreeperEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for CreeperEntity {}
