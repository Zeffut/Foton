//! Skeleton entity.
//!
//! Vanilla parity: `AbstractSkeleton` and `Skeleton`. A skeleton keeps its
//! distance and shoots, and looks for shade when the sun is up.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::SkeletonEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FleeSunGoal, Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, RestrictSunGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ArrowEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::world::World;

/// Ticks between shots.
///
/// Vanilla parity: the `attackIntervalMin` a skeleton is built with on normal
/// difficulty.
const ATTACK_INTERVAL_TICKS: i32 = 20;

/// Range within which a skeleton will loose an arrow.
///
/// Vanilla parity: the `attackRadius` of `RangedBowAttackGoal`.
const ATTACK_RADIUS: f64 = 15.0;

/// Ticks a target must stay visible before the skeleton commits to a shot.
const SEEN_TIME_BEFORE_FIRING: i32 = 20;

/// Speed of the arrows a skeleton fires.
///
/// Vanilla parity: the `1.6F` velocity of `performRangedAttack`.
const ARROW_POWER: f32 = 1.6;

/// Spread of the arrows a skeleton fires on normal difficulty.
///
/// Vanilla parity: `14 - difficulty * 4`, with difficulty 2.
const ARROW_UNCERTAINTY: f32 = 6.0;

/// Speed multiplier while repositioning.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which a skeleton watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// A skeleton.
#[entity_behavior(class = "Skeleton")]
pub struct SkeletonEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SkeletonEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SkeletonEntity`.
unsafe impl DowncastType for SkeletonEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/skeleton");
}

impl SkeletonEntity {
    /// Creates a skeleton at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a skeleton from saved base data.
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
        let mut entity_data = SkeletonEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla AbstractSkeleton goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(2, RestrictSunGoal::new());
            goals.add_goal(3, FleeSunGoal::new(1.0));
            goals.add_goal(4, RangedBowAttackGoal::new());
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(6, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(6, RandomLookAroundGoal::new());
            // TODO: vanilla also flees wolves at priority 3.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _| true),
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

    /// Looses an arrow at `target`.
    ///
    /// Vanilla parity: `AbstractSkeleton.performRangedAttack`.
    fn perform_ranged_attack(&self, target: DVec3) {
        let Some(world) = self.level() else {
            return;
        };
        // TODO: vanilla reads the bow from the skeleton's hand and consumes a
        // projectile; Steel's skeletons are not equipped yet.
        let arrow = ArrowEntity::shoot_at(&world, self, target, ARROW_POWER, ARROW_UNCERTAINTY);
        drop(arrow);

        world.play_sound_at(
            &sound_events::ENTITY_SKELETON_SHOOT,
            SoundSource::Hostile,
            self.position(),
            1.0,
            0.4f32.mul_add(rand::random::<f32>(), 0.8).recip(),
            None,
        );
    }
}

/// Keeps distance from the target and fires arrows at it.
///
/// Vanilla parity: `RangedBowAttackGoal`.
struct RangedBowAttackGoal {
    /// Ticks until the next shot.
    attack_time: i32,
    /// Ticks the target has been continuously visible.
    seen_time: i32,
}

impl RangedBowAttackGoal {
    const fn new() -> Self {
        Self {
            attack_time: -1,
            seen_time: 0,
        }
    }
}

impl Goal for RangedBowAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }

    fn stop(&mut self, _mob: &dyn PathfinderMob) {
        self.seen_time = 0;
        self.attack_time = -1;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };
        let Some(skeleton) = mob.downcast_ref::<SkeletonEntity>() else {
            return;
        };

        let target_position = target.position();
        let distance_sqr = target_position.distance_squared(mob.position());
        let in_range = distance_sqr <= ATTACK_RADIUS * ATTACK_RADIUS;

        // Vanilla only fires once the target has stayed visible for a moment, which
        // is what stops a skeleton snapping off a shot the instant it rounds a
        // corner.
        if in_range {
            self.seen_time += 1;
        } else {
            self.seen_time = 0;
        }

        if !in_range || self.seen_time < SEEN_TIME_BEFORE_FIRING {
            return;
        }

        if self.attack_time > 0 {
            self.attack_time -= 1;
            return;
        }

        skeleton.perform_ranged_attack(target_position);
        self.attack_time = ATTACK_INTERVAL_TICKS;
    }
}

impl Entity for SkeletonEntity {
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

impl LivingEntity for SkeletonEntity {
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
        Some(&sound_events::ENTITY_SKELETON_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SKELETON_DEATH)
    }
}

impl Mob for SkeletonEntity {
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
        Some(&sound_events::ENTITY_SKELETON_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SkeletonEntity {}
