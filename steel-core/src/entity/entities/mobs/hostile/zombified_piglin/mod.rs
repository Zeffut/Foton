//! Zombified piglin entity.
//!
//! Vanilla parity: `ZombifiedPiglin`. The mob that makes the Nether feel
//! populated rather than hostile: a crowd of them will ignore a player walking
//! straight through, and turn as one the moment any of them is hit. That second
//! half is the point of the mob, and it is two behaviors -- the grudge from
//! [`NeutralMob`], and the shout that spreads it.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::ZombifiedPiglinEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_blocks};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, Downcast, DowncastType, DowncastTypeKey, WorldAabb};

use crate::entity::Enemy;
use crate::entity::ai::goal::{
    HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::neutral_mob::{NeutralMob, PersistentAnger};
use crate::entity::{
    AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::world::{LevelReader as _, World};
use std::ptr;
use steel_utils::types::Difficulty;

/// Speed multiplier while chasing.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which one turns to watch a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Speed multiplier for aimless wandering.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Extra speed while angry.
///
/// Vanilla parity: `SPEED_MODIFIER_ATTACKING`, `0.05` additive. Babies do not
/// get it, which is the only reason an adult can catch you and a baby cannot.
const ANGRY_SPEED_BONUS: f64 = 0.05;

/// How far up and down the shout carries.
///
/// Vanilla parity: `ALERT_RANGE_Y`. The horizontal reach is the follow range,
/// so a shout crosses a room but not a floor.
const ALERT_RANGE_Y: f64 = 10.0;

/// Shortest gap between two shouts, in ticks.
///
/// Vanilla parity: `ALERT_INTERVAL`, four to six seconds.
const ALERT_INTERVAL_MIN: i32 = 4 * 20;
/// Longest gap between two shouts.
const ALERT_INTERVAL_MAX: i32 = 6 * 20;

/// Longest delay before the first angry grunt.
///
/// Vanilla parity: `FIRST_ANGER_SOUND_DELAY`, zero to one second.
const FIRST_ANGER_SOUND_MAX: i32 = 20;

/// Shortest grudge, in ticks.
///
/// Vanilla parity: `PERSISTENT_ANGER_TIME`, twenty to thirty-nine seconds.
const ANGER_MIN_TICKS: i64 = 20 * 20;
/// Longest grudge, in ticks.
const ANGER_MAX_TICKS: i64 = 39 * 20;

/// A zombified piglin.
#[entity_behavior(class = "ZombifiedPiglin")]
pub struct ZombifiedPiglinEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<ZombifiedPiglinEntityData>,
    anger: PersistentAnger,
    /// Ticks until the first angry grunt, once roused.
    play_first_anger_sound_in: SyncMutex<i32>,
    /// Ticks until this one may shout to its neighbors again.
    ticks_until_next_alert: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies
// `ZombifiedPiglinEntity`.
unsafe impl DowncastType for ZombifiedPiglinEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/zombified_piglin");
}

impl ZombifiedPiglinEntity {
    /// Creates a zombified piglin at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a zombified piglin from saved base data.
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
        let mut entity_data = ZombifiedPiglinEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(2, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
            // TODO: vanilla gives a piglin holding a spear a SpearUseGoal at
            // priority 1; the spear and its throw are not implemented.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            // The alerting half of the reputation: hit one and its neighbors
            // that are not already busy take the same target.
            targets.add_goal(1, HurtByTargetGoal::new().set_alert_others([]));
            // Vanilla parity: only a player it is already angry at, which is
            // what lets a crowd be walked through.
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |piglin, target, world| {
                    let Some(piglin) = piglin.and_then(Downcast::downcast_ref::<Self>) else {
                        return false;
                    };
                    // The anger check needs the shared world handle rather than
                    // the borrow the selector is given.
                    let Some(world) = piglin
                        .level()
                        .filter(|owned| ptr::eq(Arc::as_ptr(owned), ptr::from_ref(world)))
                    else {
                        return false;
                    };
                    piglin.is_angry_at(target, &world)
                }),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            anger: PersistentAnger::new(),
            play_first_anger_sound_in: SyncMutex::new(0),
            ticks_until_next_alert: SyncMutex::new(0),
        }
    }

    /// Grunts once, shortly after being roused.
    ///
    /// Vanilla parity: `maybePlayFirstAngerSound`. The delay is what makes a
    /// crowd of them answer raggedly rather than in chorus.
    fn maybe_play_first_anger_sound(&self) {
        let mut remaining = self.play_first_anger_sound_in.lock();
        if *remaining <= 0 {
            return;
        }
        *remaining -= 1;
        if *remaining == 0 {
            drop(remaining);
            self.play_sound(
                &sound_events::ENTITY_ZOMBIFIED_PIGLIN_ANGRY,
                self.sound_volume() * 2.0,
                self.voice_pitch() * 1.8,
            );
        }
    }

    /// Passes the grudge to any neighbour that has nothing better to do.
    ///
    /// Vanilla parity: `alertOthers`. Only piglins with no target of their own
    /// are recruited, so a fight does not keep re-targeting the ones already in
    /// it.
    fn alert_others(&self, world: &Arc<World>, target: &SharedEntity) {
        let reach = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::FOLLOW_RANGE);
        let position = self.position();
        let search = WorldAabb::new(
            position.x - reach,
            position.y - ALERT_RANGE_Y,
            position.z - reach,
            position.x + reach,
            position.y + ALERT_RANGE_Y,
            position.z + reach,
        );

        for other in world.get_entities_in_aabb(&search) {
            if other.id() == self.id() {
                continue;
            }
            let Some(piglin) = other.downcast_ref::<Self>() else {
                continue;
            };
            if piglin.target().is_some() {
                continue;
            }
            piglin.set_target(Some(target));
        }
    }

    /// Shouts, but no more often than every few seconds.
    ///
    /// Vanilla parity: `maybeAlertOthers`.
    fn maybe_alert_others(&self, world: &Arc<World>, target: &SharedEntity) {
        {
            let mut remaining = self.ticks_until_next_alert.lock();
            if *remaining > 0 {
                *remaining -= 1;
                return;
            }
            *remaining = rand::random_range(ALERT_INTERVAL_MIN..=ALERT_INTERVAL_MAX);
        }

        if self.has_line_of_sight_cached(target.as_ref()) {
            self.alert_others(world, target);
        }
    }
}

/// Returns whether a zombified piglin may appear at `pos`.
///
/// Vanilla parity: `checkZombifiedPiglinSpawnRules`. Light does not matter in
/// the Nether; what matters is that it will not spawn on a nether wart block,
/// which is how bastion farms keep their floors clear.
#[must_use]
fn check_zombified_piglin_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    use steel_registry::blocks::block_state_ext::BlockStateExt as _;

    world.difficulty() != Difficulty::Peaceful
        && world.get_block_state(pos.below()).get_block() != &vanilla_blocks::NETHER_WART_BLOCK
}

impl Entity for ZombifiedPiglinEntity {
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

impl LivingEntity for ZombifiedPiglinEntity {
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
        Some(&sound_events::ENTITY_ZOMBIFIED_PIGLIN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIFIED_PIGLIN_DEATH)
    }
}

impl Mob for ZombifiedPiglinEntity {
    /// Vanilla parity: `ZombifiedPiglin` derives from `Monster`.
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
        Some(&sound_events::ENTITY_ZOMBIFIED_PIGLIN_AMBIENT)
    }

    /// Starts the grunt and the shout timers when first roused.
    ///
    /// Vanilla parity: the `setTarget` override. Both timers are set only on
    /// the transition from calm to angry, not on every retarget, so a piglin
    /// switching victims does not grunt again.
    fn set_target(&self, target: Option<&SharedEntity>) -> bool {
        if self.target().is_none() && target.is_some() {
            *self.play_first_anger_sound_in.lock() = rand::random_range(0..=FIRST_ANGER_SOUND_MAX);
            *self.ticks_until_next_alert.lock() =
                rand::random_range(ALERT_INTERVAL_MIN..=ALERT_INTERVAL_MAX);
        }
        self.mob_base().set_target(target, |_| true)
    }

    /// Runs the anger clock, the speed bonus, the grunt and the shout.
    ///
    /// Vanilla parity: `customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };

        let is_baby = *self.entity_data.lock().zombie().baby.get();
        let base_speed = self
            .entity_type
            .default_attributes
            .iter()
            .find(|(key, _)| *key == "minecraft:movement_speed")
            .map_or(0.23, |(_, value)| *value);
        let wanted_speed = if self.is_angry() && !is_baby {
            base_speed + ANGRY_SPEED_BONUS
        } else {
            base_speed
        };
        self.attributes()
            .lock()
            .set_base_value(vanilla_attributes::MOVEMENT_SPEED, wanted_speed);

        if self.is_angry() {
            self.maybe_play_first_anger_sound();
        }

        self.update_persistent_anger(&world, true);

        if let Some(target) = self.target() {
            self.maybe_alert_others(&world, &target);
        }
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_zombified_piglin_spawn_rules(world, pos)
    }

    /// Rolls whether this one spawned small.
    ///
    /// Vanilla parity: the baby roll inherited from `Zombie`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if rand::random::<f32>() < AgeableMobGroupData::DEFAULT_BABY_SPAWN_CHANCE {
            self.entity_data.lock().zombie_mut().baby.set(true);
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

impl PathfinderMob for ZombifiedPiglinEntity {}

impl NeutralMob for ZombifiedPiglinEntity {
    fn persistent_anger(&self) -> &PersistentAnger {
        &self.anger
    }

    /// Vanilla parity: `startPersistentAngerTimer`, twenty to thirty-nine
    /// seconds.
    fn start_persistent_anger_timer(&self) {
        self.set_time_to_remain_angry(rand::random_range(ANGER_MIN_TICKS..=ANGER_MAX_TICKS));
    }
}

impl Enemy for ZombifiedPiglinEntity {}
