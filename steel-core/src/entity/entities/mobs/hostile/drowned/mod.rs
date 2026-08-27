//! Drowned entity.
//!
//! Vanilla parity: `Drowned`. A zombie that lives in water and is the reason
//! every shoreline is dangerous at night. What makes it its own mob is that it
//! swims: it navigates as a water creature while submerged and as a walker on
//! land, switching between the two as it goes. Steel decides that per path
//! request through [`Mob::navigation_kind`], which is exactly the seam the
//! amphibious case needs.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::DrownedEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey};

use super::zombie_common;
use crate::entity::Enemy;
use crate::entity::ai::goal::{
    Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, RangedAttackGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ThrownTridentEntity;
use crate::entity::mob::NavigationKind;
use crate::entity::spawn_rules::is_dark_enough_to_spawn;
use crate::entity::{
    AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase, PathfinderMob,
    SharedEntity, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::physics::{MoveResult, MoverType};
use crate::world::{LevelReader as _, World};
use steel_utils::types::{Difficulty, InteractionHand};

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Speed multiplier while chasing.
///
/// Vanilla parity: the `DrownedAttackGoal(this, 1.0, false)` entry.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which a drowned turns to watch a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Speed multiplier for aimless wandering.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Speed a drowned swims toward the surface at.
///
/// Vanilla parity: the `1.0` of `DrownedSwimUpGoal`.
const SWIM_UP_SPEED: f64 = 1.0;

/// How deep below sea level a drowned stops climbing.
///
/// Vanilla parity: the `seaLevel - 1` of `DrownedSwimUpGoal`.
const SWIM_UP_MARGIN: i32 = 1;

/// Drag applied to a swimming drowned each tick.
///
/// Vanilla parity: the `scale(0.9)` of `Drowned.travelInWater`.
const SWIM_DRAG: f64 = 0.9;

/// How hard a swimming drowned pushes itself.
///
/// Vanilla parity: the `moveRelative(0.01F, input)` of the same method.
const SWIM_ACCELERATION: f32 = 0.01;

/// Blocks below sea level past which a drowned may spawn anywhere.
///
/// Vanilla parity: `Drowned.isDeepEnoughToSpawn`.
const DEEP_SPAWN_MARGIN: i32 = 5;

/// One spawn attempt in this many succeeds in ordinary water.
///
/// Vanilla parity: the `nextInt(40)` of `checkDrownedSpawnRules`.
const ORDINARY_SPAWN_ODDS: i32 = 40;

/// One attempt in this many succeeds where drowned are common.
///
/// Vanilla parity: the `nextInt(15)` for the
/// `more_frequent_drowned_spawns` biomes -- rivers, where drowned crowd.
const FREQUENT_SPAWN_ODDS: i32 = 15;

/// Speed multiplier while lining up a trident throw.
///
/// Vanilla parity: the `DrownedTridentAttackGoal(this, 1.0, 40, 10.0F)` entry.
const TRIDENT_APPROACH_SPEED: f64 = 1.0;

/// Ticks between trident throws.
const TRIDENT_ATTACK_INTERVAL: i32 = 40;

/// How far a drowned is willing to throw from.
const TRIDENT_ATTACK_RADIUS: f32 = 10.0;

/// Speed the thrown trident leaves the hand at.
///
/// Vanilla parity: the `1.6F` of `Drowned.performRangedAttack`.
const TRIDENT_POWER: f32 = 1.6;

/// Spread of a thrown trident before difficulty tightens it.
///
/// Vanilla parity: the `14` of `14 - difficulty * 4`.
const TRIDENT_UNCERTAINTY_BASE: f32 = 14.0;

/// How much each difficulty step tightens the throw.
const TRIDENT_UNCERTAINTY_PER_DIFFICULTY: f32 = 4.0;

/// Fraction of a target's height a trident is aimed at.
///
/// Vanilla parity: the `getY(0.3333333333333333)` of `performRangedAttack`.
const AIM_HEIGHT_FRACTION: f64 = 1.0 / 3.0;

/// A roll above this arms the drowned at all.
///
/// Vanilla parity: the `random.nextFloat() > 0.9` of
/// `Drowned.populateDefaultEquipmentSlots` -- nine drowned in ten spawn
/// bare-handed.
const WEAPON_SPAWN_THRESHOLD: f32 = 0.9;

/// Sides of the die that picks between the two weapons.
///
/// Vanilla parity: the `nextInt(16)` of the same method.
const WEAPON_ROLL_RANGE: i32 = 16;

/// Rolls below this hand out a trident rather than a fishing rod.
const TRIDENT_ROLL_CEILING: i32 = 10;

/// Odds a drowned spawns clutching a nautilus shell.
///
/// Vanilla parity: `Drowned.NAUTILUS_SHELL_CHANCE`.
const NAUTILUS_SHELL_CHANCE: f32 = 0.03;

/// A drowned.
#[entity_behavior(class = "Drowned")]
pub struct DrownedEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<DrownedEntityData>,
    /// Whether the drowned is currently heading for land.
    ///
    /// Vanilla parity: `Drowned.searchingForLand`, which is one of the two
    /// things that make it want to swim rather than walk.
    searching_for_land: SyncMutex<bool>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `DrownedEntity`.
unsafe impl DowncastType for DrownedEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/drowned");
}

impl DrownedEntity {
    /// Creates a drowned at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a drowned from saved base data.
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
        let mut entity_data = DrownedEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Drowned.addBehaviourGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(2, DrownedTridentAttackGoal::new());
            goals.add_goal(2, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(6, DrownedSwimUpGoal);
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
            // TODO: vanilla also has DrownedGoToWaterGoal at 1 and
            // DrownedGoToBeachGoal at 5; both need a random position search
            // biased toward or away from water, which does not exist yet.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            // Vanilla parity: a drowned only hunts a player in daylight if that
            // player is in the water with it. That is why they wait out the day
            // on the sea floor and come for you the moment you swim.
            targets.add_goal(2, DrownedTargetGoal::new());
            // TODO: vanilla also targets villagers, iron golems and axolotls,
            // none of which exist yet, and baby turtles on land at priority 5
            // (`Drowned.java`), which is now implementable.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            searching_for_land: SyncMutex::new(false),
        }
    }

    /// Returns whether this drowned is a baby.
    #[must_use]
    pub fn is_baby(&self) -> bool {
        *self.entity_data.lock().zombie().baby.get()
    }

    /// Returns whether the drowned wants to swim rather than walk.
    ///
    /// Vanilla parity: `Drowned.wantsToSwim`.
    #[must_use]
    fn wants_to_swim(&self) -> bool {
        if *self.searching_for_land.lock() {
            return true;
        }
        self.target().is_some_and(|target| target.is_in_water())
    }

    /// Returns whether this target is worth chasing right now.
    ///
    /// Vanilla parity: `Drowned.okTarget`.
    #[must_use]
    fn ok_target(&self, target: &dyn Entity) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        !world.is_bright_outside() || target.is_in_water()
    }

    /// Returns whether this drowned is the kind that throws.
    ///
    /// Vanilla parity: the `getMainHandItem().is(Items.TRIDENT)` gate on
    /// `DrownedTridentAttackGoal.canUse`.
    #[must_use]
    fn holds_a_trident(&self) -> bool {
        self.get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::TRIDENT)
    }

    /// Rolls the weapon this drowned spawned with.
    ///
    /// Vanilla parity: `Drowned.populateDefaultEquipmentSlots`. One drowned in
    /// ten carries anything at all, and five of every eight of those get the
    /// trident -- which is why a thrower is a rare and unwelcome surprise.
    fn populate_default_equipment_slots(&self) {
        if rand::random::<f32>() <= WEAPON_SPAWN_THRESHOLD {
            return;
        }

        let weapon = if rand::random_range(0..WEAPON_ROLL_RANGE) < TRIDENT_ROLL_CEILING {
            &vanilla_items::TRIDENT
        } else {
            &vanilla_items::FISHING_ROD
        };
        self.set_item_in_hand(InteractionHand::MainHand, ItemStack::new(weapon));
    }
}

/// Winds a trident up and throws it.
///
/// Vanilla parity: `Drowned.DrownedTridentAttackGoal`. The wind-up is the
/// whole reason `LivingEntity` drives active item use for mobs: the goal calls
/// `startUsingItem` and the living tick carries it from there.
struct DrownedTridentAttackGoal {
    inner: RangedAttackGoal,
}

impl DrownedTridentAttackGoal {
    fn new() -> Self {
        Self {
            inner: RangedAttackGoal::new(
                TRIDENT_APPROACH_SPEED,
                TRIDENT_ATTACK_INTERVAL,
                TRIDENT_ATTACK_RADIUS,
                throw_trident,
            ),
        }
    }
}

impl Goal for DrownedTridentAttackGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let holds_a_trident = mob
            .downcast_ref::<DrownedEntity>()
            .is_some_and(DrownedEntity::holds_a_trident);
        holds_a_trident && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
        mob.set_aggressive(true);
        mob.start_using_item(InteractionHand::MainHand);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
        mob.stop_using_item();
        mob.set_aggressive(false);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }
}

/// Throws the held trident at `target`.
///
/// Vanilla parity: `Drowned.performRangedAttack`. A drowned that somehow lost
/// its trident mid-throw still throws one -- vanilla conjures the stack rather
/// than checking, and the hand is never emptied either way.
fn throw_trident(mob: &dyn PathfinderMob, target: &SharedEntity, _power: f32) {
    let Some(drowned) = mob.downcast_ref::<DrownedEntity>() else {
        return;
    };
    let Some(world) = drowned.level() else {
        return;
    };

    let main_hand = drowned.get_item_in_hand(InteractionHand::MainHand);
    let trident_item = if main_hand.is(&vanilla_items::TRIDENT) {
        main_hand
    } else {
        ItemStack::new(&vanilla_items::TRIDENT)
    };

    // Vanilla aims a third of the way up the target rather than at its feet.
    let target_position = target.position();
    let target_height = f64::from(target.base().dimensions().height);
    let aim = target_position.with_y(target_height.mul_add(AIM_HEIGHT_FRACTION, target_position.y));

    let difficulty = u8::from(world.difficulty());
    let uncertainty = TRIDENT_UNCERTAINTY_PER_DIFFICULTY
        .mul_add(-f32::from(difficulty), TRIDENT_UNCERTAINTY_BASE);
    let trident = ThrownTridentEntity::shoot_at(
        &world,
        drowned,
        &trident_item,
        aim,
        TRIDENT_POWER,
        uncertainty,
    );
    drop(trident);

    drowned.play_sound(
        &sound_events::ENTITY_DROWNED_SHOOT,
        1.0,
        0.4f32.mul_add(rand::random::<f32>(), 0.8).recip(),
    );
}

/// Takes a player, but only one the daylight rule allows.
///
/// Vanilla parity: the `okTarget` predicate wired into `Drowned`'s
/// `NearestAttackableTargetGoal`.
struct DrownedTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl DrownedTargetGoal {
    fn new() -> Self {
        Self {
            inner: NearestAttackableTargetGoal::new_for_players(true, |drowned, target, world| {
                // Vanilla captures `this` here; Steel passes the searcher in.
                let _ = drowned;
                !world.is_bright_outside() || target.is_in_water()
            }),
        }
    }
}

impl Goal for DrownedTargetGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(drowned) = mob.downcast_ref::<DrownedEntity>() else {
            return false;
        };
        let still_ok = mob
            .target()
            .is_some_and(|target| drowned.ok_target(target.as_ref()));
        still_ok && self.inner.can_continue_to_use(mob)
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

/// Climbs toward the surface when there is nothing else to do.
///
/// Vanilla parity: `Drowned.DrownedSwimUpGoal`. Without it a drowned that
/// wandered into deep water would stay on the bottom forever.
#[derive(Default)]
struct DrownedSwimUpGoal;

impl Goal for DrownedSwimUpGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        mob.is_in_water() && mob.position().y < f64::from(world.sea_level - SWIM_UP_MARGIN)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.can_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(drowned) = mob.downcast_ref::<DrownedEntity>() {
            *drowned.searching_for_land.lock() = true;
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(drowned) = mob.downcast_ref::<DrownedEntity>() {
            *drowned.searching_for_land.lock() = false;
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(world) = mob.level() else {
            return;
        };
        let position = mob.position();
        let surface = f64::from(world.sea_level);

        // Vanilla paths to the surface; in open water that path is a straight
        // line up, so the move control is asked directly. A drowned under an
        // overhang will press against it rather than swim around, which is the
        // one place this differs.
        let target = DVec3::new(position.x, surface, position.z);
        mob.mob_base()
            .controls()
            .lock()
            .move_control
            .set_wanted_position(target, SWIM_UP_SPEED);
    }
}

/// Returns whether a drowned may appear at `pos`.
///
/// Vanilla parity: `Drowned.checkDrownedSpawnRules`. There are two rates: one
/// in fifteen where the biome is tagged for frequent drowned, one in forty
/// elsewhere and only well below sea level. That is why rivers teem and the
/// open ocean surface does not.
#[must_use]
fn check_drowned_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    use steel_registry::blocks::block_state_ext::BlockStateExt as _;
    use steel_registry::fluid::is_water_fluid;
    use steel_registry::vanilla_biome_tags::BiomeTag;

    if world.difficulty() == Difficulty::Peaceful {
        return false;
    }

    let below_is_water = is_water_fluid(
        world
            .get_block_state(pos.below())
            .get_fluid_state()
            .fluid_id,
    );
    if !below_is_water && !spawn_reason.is_spawner() {
        return false;
    }

    let can_monster_spawn = (spawn_reason.ignores_light_requirements()
        || is_dark_enough_to_spawn(world, pos))
        && (spawn_reason.is_spawner()
            || is_water_fluid(world.get_block_state(pos).get_fluid_state().fluid_id));
    if !can_monster_spawn {
        return false;
    }

    if spawn_reason.is_spawner() || spawn_reason == EntitySpawnReason::Reinforcement {
        return true;
    }

    let frequent = world
        .biome_at(pos)
        .is_some_and(|biome| biome.has_tag(&BiomeTag::MORE_FREQUENT_DROWNED_SPAWNS));
    if frequent {
        return rand::random_range(0..FREQUENT_SPAWN_ODDS) == 0;
    }

    rand::random_range(0..ORDINARY_SPAWN_ODDS) == 0 && pos.y() < world.sea_level - DEEP_SPAWN_MARGIN
}

impl Entity for DrownedEntity {
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

    /// Vanilla parity: `Drowned.isPushedByFluid`. A drowned that is swimming
    /// holds its line instead of being carried off by the current.
    fn is_pushed_by_fluid(&self) -> bool {
        !self.is_swimming()
    }

    /// Vanilla parity: `Drowned.updateSwimming`. A drowned counts as swimming
    /// only while submerged and actually wanting to, which is what keeps one
    /// wading in the shallows upright.
    fn update_swimming(&self) {
        self.set_shared_swimming(self.is_under_water() && self.wants_to_swim());
    }

    /// Vanilla parity: `Drowned` adds nothing to `Zombie.addAdditionalSaveData`.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        zombie_common::save_zombie(self, self.is_baby(), nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        zombie_common::load_zombie(self, nbt);
    }
}

impl LivingEntity for DrownedEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Lets the wind-up reach the client.
    ///
    /// Without this the drowned raises its trident server-side and the flag
    /// that draws the pose never leaves the server.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
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

    /// Vanilla parity: `Drowned.getHurtSound`, which is muffled underwater.
    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if self.is_in_water() {
            &sound_events::ENTITY_DROWNED_HURT_WATER
        } else {
            &sound_events::ENTITY_DROWNED_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_in_water() {
            &sound_events::ENTITY_DROWNED_DEATH_WATER
        } else {
            &sound_events::ENTITY_DROWNED_DEATH
        })
    }

    /// Swims under its own power instead of sinking and walking.
    ///
    /// Vanilla parity: `Drowned.travelInWater`. It only applies while fully
    /// submerged and actually wanting to swim; a drowned wading in the shallows
    /// walks like a zombie.
    fn travel_in_water(
        &self,
        input: DVec3,
        base_gravity: f64,
        is_falling: bool,
        old_y: f64,
    ) -> Option<MoveResult> {
        if !self.is_under_water() || !self.wants_to_swim() {
            return self.living_travel_in_water(input, base_gravity, is_falling, old_y);
        }

        self.move_relative(SWIM_ACCELERATION, input);
        let result = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.set_velocity(self.velocity() * SWIM_DRAG);
        result
    }
}

impl Mob for DrownedEntity {
    /// Sets whether this drowned is a baby.
    ///
    /// Vanilla parity: the `Zombie.setBaby` a drowned inherits. Like its zombie
    /// and husk siblings Steel only syncs the flag so far, without vanilla's
    /// movement-speed modifier.
    fn set_baby(&self, baby: bool) {
        self.entity_data.lock().zombie_mut().baby.set(baby);
    }

    /// Vanilla parity: `Drowned` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Vanilla parity: `Zombie.canHoldItem`.
    fn can_hold_item(&self, item_stack: &ItemStack) -> bool {
        zombie_common::can_hold_item(self, self.is_baby(), item_stack)
    }

    /// Vanilla parity: `Drowned.wantsToPickUp`, which leaves spears where they
    /// fell so a drowned never swaps the trident it spawned holding.
    fn wants_to_pick_up(&self, world: &World, item_stack: &ItemStack) -> bool {
        !REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::SPEARS)
            && zombie_common::wants_to_pick_up(self, world, item_stack)
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
        Some(if self.is_in_water() {
            &sound_events::ENTITY_DROWNED_AMBIENT_WATER
        } else {
            &sound_events::ENTITY_DROWNED_AMBIENT
        })
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_drowned_spawn_rules(world, spawn_reason, pos)
    }

    /// Rolls whether this one spawned small.
    ///
    /// Vanilla parity: the baby roll a drowned inherits from `Zombie`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if rand::random::<f32>() < AgeableMobGroupData::DEFAULT_BABY_SPAWN_CHANCE {
            self.entity_data.lock().zombie_mut().baby.set(true);
        }
        // Vanilla parity: `Zombie.finalizeSpawn` rolls the loot flag and the
        // weapon, and only for a spawn that is not a conversion -- a drowning
        // zombie keeps what it was already holding, and the flag it had.
        if spawn_reason != EntitySpawnReason::Conversion {
            self.roll_spawn_can_pick_up_loot(world);
            self.populate_default_equipment_slots();
        }
        // Vanilla parity: the nautilus shell of `Drowned.finalizeSpawn`, which
        // is where the shell in every drowned-farm chest comes from.
        if !self.has_item_in_slot(EquipmentSlot::OffHand)
            && rand::random::<f32>() < NAUTILUS_SHELL_CHANCE
        {
            self.set_item_in_hand(
                InteractionHand::OffHand,
                ItemStack::new(&vanilla_items::NAUTILUS_SHELL),
            );
            self.set_guaranteed_drop(EquipmentSlot::OffHand);
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

impl PathfinderMob for DrownedEntity {
    /// Navigates as a swimmer while in water and as a walker on land.
    ///
    /// Vanilla parity: `AmphibiousPathNavigation`, which Steel expresses by
    /// answering this per path request rather than by swapping an object. This
    /// is the case the enum was shaped for.
    fn navigation_kind(&self) -> NavigationKind {
        if self.is_in_water() {
            NavigationKind::WaterBound {
                allow_breaching: false,
            }
        } else {
            NavigationKind::Ground
        }
    }
}

impl Enemy for DrownedEntity {}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use super::*;

    /// A drowned out of the water must fall through to the shared swim code.
    ///
    /// This used to read `LivingEntity::travel_in_water(self, ..)`, which in
    /// Rust dispatches back into this very override rather than to the trait's
    /// default -- so every drowned that surfaced recursed until the stack ran
    /// out and took the server with it. If that ever comes back, this test does
    /// not fail politely: it overflows, which is exactly the noise it should
    /// make.
    #[test]
    fn a_surfaced_drowned_does_not_recurse_into_itself() {
        init_vanilla_registry();
        let drowned = DrownedEntity::new(
            &vanilla_entities::DROWNED,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        assert!(!drowned.is_under_water(), "the test needs a dry drowned");

        // No world, so the move itself cannot complete; reaching this line at
        // all is the assertion.
        let _ = LivingEntity::travel_in_water(&drowned, DVec3::ZERO, 0.08, false, 0.0);
    }
}
