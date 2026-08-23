//! Polar bear entity.
//!
//! Vanilla parity: `PolarBear`. What makes a polar bear more than a large cow
//! is that it is only dangerous because of its cubs: an adult on its own
//! ignores players, an adult with a cub within eight blocks hunts them, and a
//! cub that is hurt calls the adults in and then runs. Before it strikes it
//! rears up, which is the `standing` flag the client animates.

use std::sync::{Arc, Weak};

use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use uuid::Uuid;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_biome_tags::BiomeTag;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_damage_type_tags::DamageTypeTag;
use steel_registry::vanilla_entity_data::PolarBearEntityData;
use steel_registry::{sound_events, vanilla_entities};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, UuidExt};

use crate::entity::ai::goal::{
    FloatGoal, FollowParentGoal, Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal,
    MeleeAttackGoal, NearestAttackableTargetGoal, PanicGoal, RandomLookAroundGoal,
    RandomStrollGoal, ResetUniversalAngerTargetGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::neutral_mob::{NeutralMob, PersistentAnger, read_persistent_anger};
use crate::entity::spawn::AgeableMobGroupData;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, MoveResult, PathfinderMob, SpawnGroupData,
};
use crate::world::World;

/// The passenger attachment of `PolarBear.BABY_DIMENSIONS`.
const BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.625, 0.0)];
/// Vanilla `PolarBear.BABY_DIMENSIONS`.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.7,
    0.7,
    0.343_75,
    EntityAttachments::new(&BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Shortest time a provoked bear stays angry.
///
/// Vanilla parity: `PolarBear.PERSISTENT_ANGER_TIME`, twenty to thirty-nine
/// seconds.
const ANGER_MIN_TICKS: i64 = 20 * 20;
/// Longest time a provoked bear stays angry.
const ANGER_MAX_TICKS: i64 = 39 * 20;

/// Ticks between two warning growls.
///
/// Vanilla parity: the `40` of `PolarBear.playWarningSound`.
const WARNING_SOUND_INTERVAL: i32 = 40;

/// Speed a hunting bear charges at.
const ATTACK_SPEED_MOD: f64 = 1.25;
/// Speed a panicking bear runs at.
const PANIC_SPEED_MOD: f64 = 2.0;
/// Speed a cub follows its mother at.
const FOLLOW_PARENT_SPEED_MOD: f64 = 1.25;
/// Speed a bear wanders at.
const STROLL_SPEED_MOD: f64 = 1.0;

/// How far around itself a bear looks for a cub worth defending.
///
/// Vanilla parity: the `inflate(8.0, 4.0, 8.0)` of
/// `PolarBearAttackPlayersGoal.canUse`.
const CUB_SEARCH_RANGE_XZ: f64 = 8.0;
/// How far up and down that same search reaches.
const CUB_SEARCH_RANGE_Y: f64 = 4.0;
/// How much of its follow range the player-hunting goal actually uses.
const ATTACK_PLAYERS_FOLLOW_SCALE: f64 = 0.5;
/// How often the player-hunting goal rolls.
const ATTACK_PLAYERS_INTERVAL: i32 = 20;
/// How often the fox-hunting goal rolls.
const ATTACK_FOXES_INTERVAL: i32 = 10;
/// How often the anger-driven player goal rolls.
const ANGRY_AT_PLAYERS_INTERVAL: i32 = 10;

/// How far past its own width a bear rears up at a target.
///
/// Vanilla parity: the `target.getBbWidth() + 3.0F` of
/// `PolarBearMeleeAttackGoal.checkAndPerformAttack`.
const STAND_UP_REACH: f32 = 3.0;
/// How close to its next swing a bear must be before it rears up.
const STAND_UP_WINDUP_TICKS: i32 = 10;

/// Vanilla `PolarBear.getWaterSlowDown`.
const WATER_SLOW_DOWN: f32 = 0.98;

/// A polar bear.
#[entity_behavior(class = "PolarBear")]
pub struct PolarBearEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    anger: PersistentAnger,
    warning_sound_ticks: SyncMutex<i32>,
    entity_data: SyncMutex<PolarBearEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `PolarBearEntity`.
unsafe impl DowncastType for PolarBearEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/polar_bear");
}

impl PolarBearEntity {
    /// Creates a polar bear at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a polar bear from saved base data.
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
        let ageable_base = AgeableMobBase::new();
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        let mut entity_data = PolarBearEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `PolarBear.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(1, PolarBearMeleeAttackGoal::new());
            goals.add_goal(
                1,
                PanicGoal::with_panic_causing_damage_types(PANIC_SPEED_MOD, |mob| {
                    // A cub runs from anything; a grown bear only from the
                    // world itself, because it fights back against the rest.
                    let is_baby = mob.as_ageable_mob().is_some_and(AgeableMob::is_baby);
                    if is_baby {
                        DamageTypeTag::PANIC_CAUSES
                    } else {
                        DamageTypeTag::PANIC_ENVIRONMENTAL_CAUSES
                    }
                }),
            );
            goals.add_goal(4, FollowParentGoal::new(FOLLOW_PARENT_SPEED_MOD));
            goals.add_goal(5, RandomStrollGoal::new(STROLL_SPEED_MOD));
            goals.add_goal(6, LookAtPlayerGoal::new(6.0));
            goals.add_goal(7, RandomLookAroundGoal::new());
        }
        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, PolarBearHurtByTargetGoal::new());
            targets.add_goal(2, PolarBearAttackPlayersGoal::new());
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new_for_players_with_interval(
                    ANGRY_AT_PLAYERS_INTERVAL,
                    true,
                    false,
                    |mob, target, _| {
                        let Some(mob) = mob else {
                            return false;
                        };
                        let Some(world) = mob.level() else {
                            return false;
                        };
                        mob.as_neutral_mob()
                            .is_some_and(|bear| bear.is_angry_at(target, &world))
                    },
                ),
            );
            targets.add_goal(
                4,
                NearestAttackableTargetGoal::new_with_interval(
                    ATTACK_FOXES_INTERVAL,
                    true,
                    true,
                    |mob, target, _| {
                        target.entity_type() == &vanilla_entities::FOX
                            && mob
                                .and_then(|mob| mob.as_ageable_mob())
                                .is_some_and(|bear| !AgeableMob::is_baby(bear))
                    },
                ),
            );
            targets.add_goal(5, ResetUniversalAngerTargetGoal::new(false));
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            anger: PersistentAnger::new(),
            warning_sound_ticks: SyncMutex::new(0),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `PolarBear.isStanding`.
    #[must_use]
    pub fn is_standing(&self) -> bool {
        *self.entity_data.lock().standing.get()
    }

    /// Sets vanilla `PolarBear.setStanding`.
    pub fn set_standing(&self, standing: bool) {
        self.entity_data.lock().standing.set(standing);
    }

    /// Vanilla parity: `PolarBear.playWarningSound`, which is rate-limited so a
    /// rearing bear growls once rather than every tick.
    fn play_warning_sound(&self) {
        let mut warning_sound_ticks = self.warning_sound_ticks.lock();
        if *warning_sound_ticks > 0 {
            return;
        }

        *warning_sound_ticks = WARNING_SOUND_INTERVAL;
        drop(warning_sound_ticks);
        self.make_sound(Some(&sound_events::ENTITY_POLAR_BEAR_WARNING));
    }

    /// Vanilla parity: `PolarBear.checkPolarBearSpawnRules`. On a frozen ocean
    /// a bear spawns on the ice, where the shared animal rule would refuse.
    #[must_use]
    pub fn check_polar_bear_spawn_rules(
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let on_alternate_blocks = world
            .biome_at(pos)
            .is_some_and(|biome| biome.has_tag(&BiomeTag::POLAR_BEARS_SPAWN_ON_ALTERNATE_BLOCKS));
        if !on_alternate_blocks {
            return <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos);
        }

        <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
            && world
                .get_block_state(pos.below())
                .get_block()
                .has_tag(&BlockTag::POLAR_BEARS_SPAWNABLE_ON_ALTERNATE)
    }

    /// Returns whether a cub is close enough for this bear to defend.
    fn has_cub_nearby(mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };

        let search_box = mob.bounding_box().inflate_xyz(
            CUB_SEARCH_RANGE_XZ,
            CUB_SEARCH_RANGE_Y,
            CUB_SEARCH_RANGE_XZ,
        );
        !world
            .get_entities_in_aabb_matching(&search_box, |entity| {
                entity
                    .downcast_ref::<Self>()
                    .is_some_and(AgeableMob::is_baby)
            })
            .is_empty()
    }
}

impl Entity for PolarBearEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `PolarBear.tick`, whose server half is the warning
    /// cooldown and the anger clock. The stand animation is client-local.
    fn tick(&self) {
        self.default_tick();

        {
            let mut warning_sound_ticks = self.warning_sound_ticks.lock();
            if *warning_sound_ticks > 0 {
                *warning_sound_ticks -= 1;
            }
        }

        if let Some(world) = self.level() {
            self.update_persistent_anger(&world, true);
        }
    }

    /// Vanilla parity: `PolarBear.getDefaultDimensions`. The taller standing
    /// box is driven by `clientSideStandAnimation`, which only the client has.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_POLAR_BEAR_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("anger_end_time", self.persistent_anger_end_time());
        if let Some(target) = self.persistent_anger_target() {
            nbt.insert("angry_at", NbtTag::IntArray(target.to_int_array().to_vec()));
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        read_persistent_anger(
            self,
            nbt.long("anger_end_time"),
            nbt.int("AngerTime"),
            nbt.int_array("angry_at")
                .and_then(|values| Uuid::from_int_array(&values)),
        );
    }
}

impl LivingEntity for PolarBearEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
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

    /// Vanilla parity: `PolarBear.getWaterSlowDown`; a bear swims better than
    /// anything else that walks.
    fn get_water_slow_down(&self) -> f32 {
        WATER_SLOW_DOWN
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_POLAR_BEAR_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_POLAR_BEAR_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for PolarBearEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }
}

impl Animal for PolarBearEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla parity: `PolarBear.isFood` refuses everything, so a polar bear
    /// cannot be bred or tempted at all.
    fn is_food(&self, _item_stack: &ItemStack) -> bool {
        false
    }
}

impl NeutralMob for PolarBearEntity {
    fn persistent_anger(&self) -> &PersistentAnger {
        &self.anger
    }

    /// Vanilla parity: `PolarBear.startPersistentAngerTimer`.
    fn start_persistent_anger_timer(&self) {
        self.set_time_to_remain_angry(rand::random_range(ANGER_MIN_TICKS..=ANGER_MAX_TICKS));
    }
}

impl Mob for PolarBearEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_POLAR_BEAR_AMBIENT_BABY
        } else {
            &sound_events::ENTITY_POLAR_BEAR_AMBIENT
        })
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        Self::check_polar_bear_spawn_rules(world, spawn_reason, pos)
    }

    /// Vanilla parity: `PolarBear.finalizeSpawn`, which forces the group data
    /// so every bear after the first in a spawn group is born a cub.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_baby_spawn_chance(1.0),
        ));
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }
}

impl PathfinderMob for PolarBearEntity {}

fn bear_of(mob: &dyn PathfinderMob) -> Option<&PolarBearEntity> {
    mob.downcast_ref::<PolarBearEntity>()
}

/// Rears up before it swings, and drops back down when it lands the blow.
///
/// Vanilla parity: `PolarBear.PolarBearMeleeAttackGoal`.
struct PolarBearMeleeAttackGoal {
    inner: MeleeAttackGoal,
}

impl PolarBearMeleeAttackGoal {
    const fn new() -> Self {
        Self {
            inner: MeleeAttackGoal::new(ATTACK_SPEED_MOD, true),
        }
    }
}

impl Goal for PolarBearMeleeAttackGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(bear) = bear_of(mob) {
            bear.set_standing(false);
        }
        self.inner.stop(mob);
    }

    /// Vanilla parity: `PolarBearMeleeAttackGoal.checkAndPerformAttack`, which
    /// replaces the base attack: no swing animation, and a rear-up in the last
    /// ten ticks before the blow lands.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = self.inner.tick_without_attack(mob) else {
            return;
        };
        let Some(bear) = bear_of(mob) else {
            return;
        };
        let Some(target_living) = target.as_living_entity() else {
            return;
        };

        if self.inner.can_perform_attack(mob, target_living) {
            self.inner.reset_attack_cooldown();
            if let Some(world) = mob.level() {
                let _ = mob.do_hurt_target(&world, &target);
            }
            bear.set_standing(false);
            return;
        }

        let stand_up_range = f64::from(target.bounding_box().width() as f32 + STAND_UP_REACH);
        if mob.position().distance_squared(target.position()) < stand_up_range * stand_up_range {
            if self.inner.is_time_to_attack() {
                bear.set_standing(false);
                self.inner.reset_attack_cooldown();
            }

            if self.inner.get_ticks_until_next_attack() <= STAND_UP_WINDUP_TICKS {
                bear.set_standing(true);
                bear.play_warning_sound();
            }
        } else {
            self.inner.reset_attack_cooldown();
            bear.set_standing(false);
        }
    }
}

/// Calls the adults in, and lets a cub back out of the fight it started.
///
/// Vanilla parity: `PolarBear.PolarBearHurtByTargetGoal`.
struct PolarBearHurtByTargetGoal {
    inner: HurtByTargetGoal,
}

impl PolarBearHurtByTargetGoal {
    fn new() -> Self {
        Self {
            // Vanilla parity: `alertOther` only wakes grown bears, so a hurt
            // cub does not turn the other cubs on the attacker.
            inner: HurtByTargetGoal::new().with_alert_filter(|other| {
                other
                    .as_ageable_mob()
                    .is_none_or(|ageable| !AgeableMob::is_baby(ageable))
            }),
        }
    }
}

impl Goal for PolarBearHurtByTargetGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    /// Vanilla parity: `PolarBearHurtByTargetGoal.start`. A cub shouts for help
    /// and then immediately gives up its own target.
    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);

        let is_baby = bear_of(mob).is_some_and(AgeableMob::is_baby);
        if !is_baby {
            return;
        }

        if let Some(hurt_by_mob) = mob.last_hurt_by_mob() {
            self.inner.alert_others(mob, &hurt_by_mob);
        }
        self.inner.stop(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }
}

/// Hunts players, but only while there is a cub to defend.
///
/// Vanilla parity: `PolarBear.PolarBearAttackPlayersGoal`.
struct PolarBearAttackPlayersGoal {
    inner: NearestAttackableTargetGoal,
}

impl PolarBearAttackPlayersGoal {
    fn new() -> Self {
        Self {
            inner: NearestAttackableTargetGoal::new_for_players_with_interval(
                ATTACK_PLAYERS_INTERVAL,
                true,
                true,
                |_, _, _| true,
            )
            .with_follow_distance_scale(ATTACK_PLAYERS_FOLLOW_SCALE),
        }
    }
}

impl Goal for PolarBearAttackPlayersGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let is_baby = bear_of(mob).is_some_and(AgeableMob::is_baby);
        if is_baby {
            return false;
        }

        self.inner.can_use(mob) && PolarBearEntity::has_cub_nearby(mob)
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
}

#[cfg(test)]
mod tests;
