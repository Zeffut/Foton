//! Witch entity.
//!
//! Vanilla parity: `Witch`. The only mob that fights with the alchemy a player
//! uses: it throws splash potions and drinks its own. Both halves landed
//! earlier this session, so this mob is mostly a matter of choosing which
//! bottle for which situation -- which is the whole of its character.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::data_components::PotionContents;
use steel_registry::data_components::vanilla_components::POTION_CONTENTS;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::potion::PotionRef;
use steel_registry::registry::reference::RegistryReference;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::WitchEntityData;
use steel_registry::{
    item_stack::ItemStack, sound_events, vanilla_attributes, vanilla_damage_type_tags,
    vanilla_entities, vanilla_items, vanilla_mob_effects, vanilla_potions,
};
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::behavior::potion_effects;
use crate::entity::Enemy;
use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LongDistancePatrolGoal, LookAtPlayerGoal,
    NearestAttackableTargetGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal,
    RaiderCelebrationGoal, RaiderMoveThroughVillageGoal, RandomLookAroundGoal, RangedAttackGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::SplashPotionEntity;
use crate::entity::patrolling_monster::{
    PatrolState, PatrollingMonster, read_patrol_state, write_patrol_state,
};
use crate::entity::projectile::{Projectile, ThrowableItemProjectile};
use crate::entity::raider::{
    Raider, RaiderState, finalize_spawn_raider, read_raider_state, write_raider_state,
};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob, SharedEntity, SpawnGroupData,
    next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::world::World;
use steel_registry::items::ItemRef;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Speed multiplier while repositioning.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Ticks between two thrown potions.
///
/// Vanilla parity: the `RangedAttackGoal(this, 1.0, 60, 10.0F)` entry.
const ATTACK_INTERVAL: i32 = 60;

/// How far a witch is willing to throw.
const ATTACK_RADIUS: f32 = 10.0;

/// Distance at which a witch turns to watch a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Speed multiplier for aimless wandering.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// How much slower a witch moves while drinking.
///
/// Vanilla parity: `SPEED_MODIFIER_DRINKING`, a quarter of its speed. Drinking
/// is a real commitment rather than a free action, and it is the window a
/// player has to close in.
const DRINKING_SPEED_MULTIPLIER: f64 = 0.25;

/// Ticks a witch takes to drain a bottle.
///
/// Vanilla parity: the potion item's use duration.
const DRINK_TICKS: i32 = 32;

/// Squared distance past which a witch drinks swiftness to close in.
///
/// Vanilla parity: the `distanceToSqr(this) > 121.0` of `aiStep`.
const SWIFTNESS_DISTANCE_SQR: f64 = 121.0;

/// Distance past which a thrown potion is aimed to slow rather than harm.
///
/// Vanilla parity: the `dist >= 8.0` of `performRangedAttack`.
const SLOWNESS_RANGE: f64 = 8.0;

/// Distance within which a witch reaches for weakness instead.
const WEAKNESS_RANGE: f64 = 3.0;

/// Health above which a target is worth poisoning rather than harming.
const POISON_HEALTH_THRESHOLD: f32 = 8.0;

/// How much of a magic hit a witch shrugs off.
///
/// Vanilla parity: the `damage *= 0.15F` for `WITCH_RESISTANT_TO`. It is why
/// throwing her own poison back barely works.
const MAGIC_RESISTANCE: f32 = 0.15;

/// Speed a follower patrols at.
///
/// Vanilla parity: the `0.7` of `PatrollingMonster.registerGoals`.
const PATROL_SPEED: f64 = 0.7;

/// Speed the captain patrols at, so the group keeps up.
const PATROL_LEADER_SPEED: f64 = 0.595;

/// Speed a raider walks the streets of the village it is raiding at.
///
/// Vanilla parity: the `1.05F` of `new RaiderMoveThroughVillageGoal(this, 1.05F, 1)`.
const VILLAGE_WALK_SPEED_MODIFIER: f64 = 1.05;

/// How close to a house counts as having reached it.
///
/// Vanilla parity: the `1` of the same goal.
const VILLAGE_POI_ARRIVAL_DISTANCE: f64 = 1.0;

/// A witch.
#[entity_behavior(class = "Witch")]
pub struct WitchEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<WitchEntityData>,
    patrol_state: PatrolState,
    raider_state: RaiderState,
    /// Ticks left on the bottle currently being drunk.
    using_time: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `WitchEntity`.
unsafe impl DowncastType for WitchEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/witch");
}

impl WitchEntity {
    /// Creates a witch at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a witch from saved base data.
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
        let mut entity_data = WitchEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Witch.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(
                2,
                RangedAttackGoal::new(
                    ATTACK_SPEED_MODIFIER,
                    ATTACK_INTERVAL,
                    ATTACK_RADIUS,
                    throw_potion_at,
                ),
            );
            goals.add_goal(2, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(3, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(3, RandomLookAroundGoal::new());
            // Vanilla parity: the goals `PatrollingMonster` and `Raider` add
            // above the witch's own. A swamp-hut witch never patrols -- nothing
            // sets her patrolling -- but a witch pulled into a raid uses all
            // four of these.
            goals.add_goal(
                4,
                LongDistancePatrolGoal::new(PATROL_SPEED, PATROL_LEADER_SPEED),
            );
            goals.add_goal(1, ObtainRaidLeaderBannerGoal::new());
            goals.add_goal(3, PathfindToRaidGoal::new());
            goals.add_goal(
                4,
                RaiderMoveThroughVillageGoal::new(
                    VILLAGE_WALK_SPEED_MODIFIER,
                    VILLAGE_POI_ARRIVAL_DISTANCE,
                ),
            );
            goals.add_goal(5, RaiderCelebrationGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(
                1,
                HurtByTargetGoal::new()
                    .with_ignored_damage_filter(|entity| entity.as_raider().is_some()),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
            // TODO: vanilla also heals other raiders at priority 2 through
            // `NearestHealableRaiderTargetGoal`, which Steel does not have.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            patrol_state: PatrolState::new(),
            raider_state: RaiderState::new(),
            using_time: SyncMutex::new(0),
        }
    }

    /// Returns whether the witch has a bottle to her lips.
    #[must_use]
    pub fn is_drinking_potion(&self) -> bool {
        *self.entity_data.lock().using_item.get()
    }

    fn set_using_item(&self, using: bool) {
        self.entity_data.lock().using_item.set(using);
    }

    /// Returns the potion this witch wants right now, if any.
    ///
    /// Vanilla parity: the ladder of `Witch.aiStep`. The order matters: she
    /// deals with drowning and burning before she thinks about healing, and
    /// only reaches for swiftness when the target is too far to hit.
    fn potion_to_drink(&self) -> Option<PotionRef> {
        if rand::random::<f32>() < 0.15
            && self.is_eye_in_water()
            && !self.has_mob_effect(vanilla_mob_effects::WATER_BREATHING)
        {
            return Some(&vanilla_potions::WATER_BREATHING);
        }
        if rand::random::<f32>() < 0.15
            && self.is_on_fire()
            && !self.has_mob_effect(vanilla_mob_effects::FIRE_RESISTANCE)
        {
            return Some(&vanilla_potions::FIRE_RESISTANCE);
        }
        if rand::random::<f32>() < 0.05 && self.get_health() < self.get_max_health() {
            return Some(&vanilla_potions::HEALING);
        }

        let target_far = self.target().is_some_and(|target| {
            self.position().distance_squared(target.position()) > SWIFTNESS_DISTANCE_SQR
        });
        if rand::random::<f32>() < 0.5
            && target_far
            && !self.has_mob_effect(vanilla_mob_effects::SPEED)
        {
            return Some(&vanilla_potions::SWIFTNESS);
        }

        None
    }

    /// Raises a bottle and slows down for as long as it takes.
    fn start_drinking(&self, potion: PotionRef) {
        let bottle = potion_bottle(&vanilla_items::POTION, potion);
        self.living_base()
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, bottle);

        *self.using_time.lock() = DRINK_TICKS;
        self.set_using_item(true);
        self.play_sound(
            &sound_events::ENTITY_WITCH_DRINK,
            1.0,
            0.4f32.mul_add(rand::random::<f32>(), 0.8),
        );
        self.apply_drinking_speed(true);
    }

    /// Finishes the bottle and takes what was in it.
    fn finish_drinking(&self) {
        self.set_using_item(false);

        let bottle = {
            let mut equipment = self.living_base().equipment().lock();
            let bottle = equipment.get_ref(EquipmentSlot::MainHand).clone();
            equipment.set(EquipmentSlot::MainHand, ItemStack::empty());
            bottle
        };

        if bottle.is(&vanilla_items::POTION)
            && let Some(contents) = bottle.get(POTION_CONTENTS)
        {
            for (effect, duration, amplifier) in potion_effects(contents) {
                self.add_mob_effect(MobEffectInstance::with_duration(
                    effect, duration, amplifier,
                ));
            }
        }

        self.apply_drinking_speed(false);
    }

    /// Slows the witch while she drinks, and lets her go again after.
    fn apply_drinking_speed(&self, drinking: bool) {
        let base = self
            .entity_type
            .default_attributes
            .iter()
            .find(|(key, _)| *key == "minecraft:movement_speed")
            .map_or(0.25, |(_, value)| *value);
        let speed = if drinking {
            base * DRINKING_SPEED_MULTIPLIER
        } else {
            base
        };
        self.attributes()
            .lock()
            .set_base_value(vanilla_attributes::MOVEMENT_SPEED, speed);
    }
}

/// Builds a bottle of one potion.
fn potion_bottle(item: ItemRef, potion: PotionRef) -> ItemStack {
    let mut stack = ItemStack::new(item);
    stack.set(
        POTION_CONTENTS,
        PotionContents::new(Some(RegistryReference::new(potion)), None, Vec::new(), None),
    );
    stack
}

/// Throws the potion this situation calls for.
///
/// Vanilla parity: `Witch.performRangedAttack`. The choice of bottle is the
/// witch's whole tactical repertoire: slowness to keep you away, poison while
/// you are healthy, weakness up close, and harming otherwise.
fn throw_potion_at(mob: &dyn PathfinderMob, target: &SharedEntity, _power: f32) {
    let Some(witch) = mob.downcast_ref::<WitchEntity>() else {
        return;
    };
    if witch.is_drinking_potion() {
        return;
    }
    let Some(world) = mob.level() else {
        return;
    };

    let position = mob.position();
    let target_position = target.position();
    // Vanilla leads the target by its current velocity, which is why a witch
    // hits a running player rather than the ground behind them.
    let velocity = target.velocity();
    let xd = target_position.x + velocity.x - position.x;
    let yd = target.get_eye_y() - 1.1 - position.y;
    let zd = target_position.z + velocity.z - position.z;
    let horizontal = xd.hypot(zd);

    let potion = choose_thrown_potion(target, horizontal);

    let thrown = Arc::new(SplashPotionEntity::new(
        &vanilla_entities::SPLASH_POTION,
        next_entity_id(),
        DVec3::new(position.x, position.y + 1.0, position.z),
        Arc::downgrade(&world),
    ));
    thrown.set_item_clamped(potion_bottle(&vanilla_items::SPLASH_POTION, potion));

    let power = if horizontal <= 2.0 { 0.45 } else { 0.75 };
    thrown.shoot(DVec3::new(xd, horizontal.mul_add(0.2, yd), zd), power, 8.0);

    let entity: SharedEntity = thrown;
    if let Err(error) = world.try_add_entity(entity) {
        log::debug!("witch failed to throw a potion: {error}");
        return;
    }

    witch.play_sound(&sound_events::ENTITY_WITCH_THROW, 1.0, 0.8);
}

/// Picks which potion to throw.
///
/// Vanilla parity: the ladder of `performRangedAttack`.
fn choose_thrown_potion(target: &SharedEntity, distance: f64) -> PotionRef {
    let Some(living) = target.as_living_entity() else {
        return &vanilla_potions::HARMING;
    };

    if distance >= SLOWNESS_RANGE && !living.has_mob_effect(vanilla_mob_effects::SLOWNESS) {
        return &vanilla_potions::SLOWNESS;
    }
    if living.get_health() >= POISON_HEALTH_THRESHOLD
        && !living.has_mob_effect(vanilla_mob_effects::POISON)
    {
        return &vanilla_potions::POISON;
    }
    if distance <= WEAKNESS_RANGE
        && !living.has_mob_effect(vanilla_mob_effects::WEAKNESS)
        && rand::random::<f32>() < 0.25
    {
        return &vanilla_potions::WEAKNESS;
    }
    &vanilla_potions::HARMING
}

impl Entity for WitchEntity {
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

    /// Vanilla parity: `AbstractIllager.isAlliedTo` is not shared with the
    /// witch, but `Raider` is -- her wave and her patrol are saved the same way
    /// every other raider's are.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        write_patrol_state(self, nbt);
        write_raider_state(self, nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        read_patrol_state(self, nbt);
        read_raider_state(self, nbt);
    }
}

impl LivingEntity for WitchEntity {
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
        Some(&sound_events::ENTITY_WITCH_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WITCH_DEATH)
    }

    /// Shrugs off most magic, and all of her own.
    ///
    /// Vanilla parity: `Witch.getDamageAfterMagicAbsorb`. Her own splash
    /// potions cannot hurt her at all, which is what lets her stand in her own
    /// cloud.
    fn get_damage_after_magic_absorb(&self, source: &DamageSource, damage: f32) -> f32 {
        let damage = self.living_damage_after_magic_absorb(source, damage);
        if source.causing_entity_id == Some(self.id()) {
            return 0.0;
        }
        if source.is(&vanilla_damage_type_tags::DamageTypeTag::WITCH_RESISTANT_TO) {
            return damage * MAGIC_RESISTANCE;
        }
        damage
    }
}

impl Mob for WitchEntity {
    /// Vanilla parity: `Witch` derives from `Monster`.
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
        Some(&sound_events::ENTITY_WITCH_AMBIENT)
    }

    /// Drinks when she needs to, and only then.
    ///
    /// Vanilla parity: the drinking half of `Witch.aiStep`.
    fn custom_server_ai_step(&self) {
        if !Entity::is_alive(self) {
            return;
        }

        if self.is_drinking_potion() {
            let finished = {
                let mut remaining = self.using_time.lock();
                *remaining -= 1;
                *remaining <= 0
            };
            if finished {
                self.finish_drinking();
            }
            return;
        }

        if let Some(potion) = self.potion_to_drink() {
            self.start_drinking(potion);
        }
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        finalize_spawn_raider(self, spawn_reason);
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    fn remove_when_far_away(&self, dist_sqr: f64) -> bool {
        self.remove_when_far_away_raider(dist_sqr)
    }

    fn requires_custom_persistence(&self) -> bool {
        self.requires_custom_persistence_raider() || self.is_passenger() || self.is_leashed()
    }

    /// Vanilla parity: `Raider.updateNoActionTime`.
    fn update_no_action_time(&self) {
        self.increment_no_action_time();
        self.increment_no_action_time();
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for WitchEntity {}

impl PatrollingMonster for WitchEntity {
    fn patrol_state(&self) -> &PatrolState {
        &self.patrol_state
    }

    /// Vanilla parity: `Witch.canBeLeader`, the one raider besides the ravager
    /// that refuses the banner.
    fn can_be_leader(&self) -> bool {
        false
    }

    fn can_join_patrol(&self) -> bool {
        self.can_join_patrol_raider()
    }
}

impl Raider for WitchEntity {
    fn raider_state(&self) -> &RaiderState {
        &self.raider_state
    }

    /// Vanilla parity: `Witch.applyRaidBuffs`, which is empty -- she brings her
    /// own potions and needs nothing from the wave.
    fn apply_raid_buffs(&self, _wave: i32, _is_captain: bool) {}

    fn celebrate_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_WITCH_CELEBRATE
    }

    fn is_celebrating(&self) -> bool {
        *self.entity_data.lock().raider().is_celebrating.get()
    }

    fn set_celebrating(&self, celebrating: bool) {
        self.entity_data
            .lock()
            .raider_mut()
            .is_celebrating
            .set(celebrating);
    }
}

impl Enemy for WitchEntity {}
