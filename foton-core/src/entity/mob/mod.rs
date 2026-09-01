//! Vanilla-shaped mob foundations.

mod leash;
mod pathfinder;

pub use leash::LeashAttachment;
use leash::{
    DELAYED_LEASH_DROP_TICKS, ENTITY_LEASH_ATTACHMENT_POINT, LEASH_ELASTIC_DISTANCE,
    LEASH_SNAP_DISTANCE, LEASH_STIFFNESS, LEASH_TORSIONAL_ELASTICITY, LEASHER_ATTACHMENT_POINT,
    LeashData, QUAD_LEASH_WRENCH_SCALE, SHARED_QUAD_ATTACHMENT_POINTS,
    axis_specific_leash_elasticity, compute_elastic_interaction, leash_bounding_box_center,
    leash_holder_movement,
};
use pathfinder::tick_path_navigation_target;
pub use pathfinder::{NavigationKind, PathfinderMob};
#[cfg(test)]
use pathfinder::{find_ground_path_target_surface, path_end_node_can_reach_target};

use std::f32::consts::PI;
use std::sync::Arc;

use foton_math::fast_floor;
use foton_protocol::packets::game::{CTakeItemEntity, SoundSource};
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::enchantment_effect::EnchantmentEffectComponent;
use foton_registry::item_stack::ItemStack;
use foton_registry::loot_table::{LootContext, LootTableRef};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::spawn_data::EquipmentTable;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_entity_type_tags::EntityTypeTag;
use foton_registry::vanilla_game_rules::{ENTITY_DROPS, MOB_GRIEFING};
use foton_registry::{
    REGISTRY, RegistryExt, TaggedRegistryExt as _, sound_events, vanilla_attributes,
    vanilla_damage_types, vanilla_entities, vanilla_game_events, vanilla_items,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::types::{Difficulty, InteractionHand};
use foton_utils::{BlockPos, ChunkPos, Downcast as _, Identifier, WorldAabb, axis::Axis};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};

use crate::behavior::items::SpawnEggItem;
use crate::behavior::{
    BLOCK_BEHAVIORS, BlockCollisionContext, ITEM_BEHAVIORS, InteractionResult, InventoryAccess,
};
use crate::enchantment_helper::{self, EnchantmentDamageContext, EnchantmentPostAttackContext};
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::control::{
    BodyRotationInput, MobControls, MoveControlOperation, rotate_if_necessary, rotate_towards,
};
use crate::entity::ai::goal::{GoalControl, GoalSelector};
use crate::entity::ai::navigation::PathNavigation;
use crate::entity::ai::path::{PathType, PathfindingContext, PathfindingMalus};
use crate::entity::ai::sensing::Sensing;
use crate::entity::ai::walk::WalkPathEvaluator;
use crate::entity::attribute::{AttributeModifier, AttributeModifierOperation};
use crate::entity::damage::DamageSource;
use crate::entity::entities::{ItemEntity, LeashFenceKnotEntity};
use crate::entity::entity_loot_ref;
use crate::entity::raider;
use crate::entity::spawn_rules::check_mob_spawn_rules;
use crate::entity::{
    Entity, EntitySpawnReason, LivingEntity, LivingTravelInput, RemovalReason, SharedEntity,
    SpawnGroupData, WeakEntity,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Odds a mob of the zombie or skeleton family spawns able to pick loot up.
///
/// Vanilla parity: the `0.55F` of `Zombie.finalizeSpawn` and
/// `AbstractSkeleton.finalizeSpawn`, scaled by the local difficulty's special
/// multiplier.
const SPAWN_CAN_PICK_UP_LOOT_CHANCE: f32 = 0.55;

const MOB_FLAG_NO_AI: i8 = 1;
const MOB_FLAG_LEFT_HANDED: i8 = 2;
const MOB_FLAG_AGGRESSIVE: i8 = 4;
const MOVE_CONTROL_MIN_SPEED_SQR: f64 = 2.500_000_3e-7;
const MOVE_CONTROL_MAX_TURN: f32 = 90.0;
/// Vanilla parity: the `1.0E-5F` of `FlyingMoveControl.tick`, below which a
/// flier stops climbing rather than chasing a target it is already level with.
const MOVE_CONTROL_MIN_FLYING_DELTA: f32 = 1.0e-5;
const DEFAULT_EQUIPMENT_DROP_CHANCE: f32 = 0.085;
const PRESERVE_ITEM_DROP_CHANCE_THRESHOLD: f32 = 1.0;
const PRESERVE_ITEM_DROP_CHANCE: f32 = 2.0;
const BODY_ROTATION_MOVING_DISTANCE_SQR: f64 = 2.500_000_3e-7;
const TARGET_REACH_DISTANCE_SQR: f64 = 2.25;
const DEFAULT_ATTACK_REACH_BASE: f32 = 2.04;
const DEFAULT_ATTACK_REACH_OFFSET: f32 = 0.6;
const RANDOM_SPAWN_BONUS_ID: Identifier = Identifier::vanilla_static("random_spawn_bonus");
const RANDOM_SPAWN_BONUS_SCALE: f64 = 0.114_850_000_000_000_01;
const LEFT_HANDED_SPAWN_CHANCE: f32 = 0.05;

/// How a mob turns a wanted position into movement.
///
/// Vanilla parity: the `MoveControl` subclass a mob installs in its
/// constructor. Foton keeps one control and asks the mob which shape it wants,
/// the way [`NavigationKind`] already handles navigation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoveControlKind {
    /// Vanilla parity: `MoveControl`.
    Ground,
    /// Vanilla parity: `FlyingMoveControl`.
    Flying {
        /// How fast the mob may pitch toward its target, in degrees per tick.
        max_turn: f32,
        /// Whether the mob keeps gravity off while it has nowhere to be.
        hovers_in_place: bool,
    },
}

/// Picks the slot an item rolled by an equipment table goes into.
///
/// Vanilla parity: `EquipmentUser.resolveSlot`.
fn resolve_equipment_slot(
    to_equip: &ItemStack,
    already_inserted: &[EquipmentSlot],
) -> Option<EquipmentSlot> {
    if to_equip.is_empty() {
        return None;
    }
    match to_equip.get_equippable_slot() {
        Some(slot) if !already_inserted.contains(&slot) => Some(slot),
        Some(_) => None,
        None => (!already_inserted.contains(&EquipmentSlot::MainHand))
            .then_some(EquipmentSlot::MainHand),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DropChances {
    by_equipment: [f32; EquipmentSlot::ALL.len()],
}

impl DropChances {
    const DEFAULT: Self = Self {
        by_equipment: [DEFAULT_EQUIPMENT_DROP_CHANCE; EquipmentSlot::ALL.len()],
    };

    #[must_use]
    const fn by_equipment(self, slot: EquipmentSlot) -> f32 {
        self.by_equipment[slot.index()]
    }

    const fn set_guaranteed_drop(&mut self, slot: EquipmentSlot) {
        self.by_equipment[slot.index()] = PRESERVE_ITEM_DROP_CHANCE;
    }

    fn set_equipment_chance(&mut self, slot: EquipmentSlot, chance: f32) -> bool {
        if chance < 0.0 {
            return false;
        }

        self.by_equipment[slot.index()] = chance;
        true
    }

    #[must_use]
    fn is_preserved(self, slot: EquipmentSlot) -> bool {
        self.by_equipment(slot) > PRESERVE_ITEM_DROP_CHANCE_THRESHOLD
    }

    fn save(self, nbt: &mut NbtCompound) {
        if self == Self::DEFAULT {
            return;
        }

        let mut drop_chances = NbtCompound::new();
        for slot in EquipmentSlot::ALL {
            let chance = self.by_equipment(slot);
            if chance.to_bits() != DEFAULT_EQUIPMENT_DROP_CHANCE.to_bits() {
                drop_chances.insert(slot.name(), chance);
            }
        }

        nbt.insert("drop_chances", NbtTag::Compound(drop_chances));
    }

    fn load(nbt: BorrowedNbtCompoundView<'_, '_>) -> Self {
        let Some(drop_chances) = nbt.compound("drop_chances") else {
            return Self::DEFAULT;
        };

        let mut loaded = Self::DEFAULT;
        for slot in EquipmentSlot::ALL {
            let Some(chance) = drop_chances.float(slot.name()) else {
                continue;
            };
            if !loaded.set_equipment_chance(slot, chance) {
                return Self::DEFAULT;
            }
        }

        loaded
    }
}

#[derive(Debug)]
pub struct MobBase {
    goal_selector: SyncMutex<GoalSelector>,
    target_selector: SyncMutex<GoalSelector>,
    target: SyncMutex<Option<WeakEntity>>,
    sensing: SyncMutex<Sensing>,
    controls: SyncMutex<MobControls>,
    navigation: SyncMutex<PathNavigation>,
    pathfinding_malus: SyncMutex<PathfindingMalus>,
    persistence_required: SyncMutex<bool>,
    can_pick_up_loot: SyncMutex<bool>,
    drop_chances: SyncMutex<DropChances>,
    home_restriction: SyncMutex<MobHomeRestriction>,
    death_loot_table: SyncMutex<Option<Identifier>>,
    death_loot_table_seed: SyncMutex<i64>,
    leash_data: SyncMutex<Option<LeashData>>,
    ambient_sound_time: SyncMutex<i32>,
    xp_reward: SyncMutex<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MobHomeRestriction {
    position: BlockPos,
    radius: i32,
}
impl MobHomeRestriction {
    const fn none() -> Self {
        Self {
            position: BlockPos::ZERO,
            radius: -1,
        }
    }
}

impl MobBase {
    #[must_use]
    pub fn new() -> Self {
        Self {
            goal_selector: SyncMutex::new(GoalSelector::new()),
            target_selector: SyncMutex::new(GoalSelector::new()),
            target: SyncMutex::new(None),
            sensing: SyncMutex::new(Sensing::new()),
            controls: SyncMutex::new(MobControls::new()),
            navigation: SyncMutex::new(PathNavigation::new()),
            pathfinding_malus: SyncMutex::new(PathfindingMalus::new()),
            persistence_required: SyncMutex::new(false),
            can_pick_up_loot: SyncMutex::new(false),
            drop_chances: SyncMutex::new(DropChances::DEFAULT),
            home_restriction: SyncMutex::new(MobHomeRestriction::none()),
            death_loot_table: SyncMutex::new(None),
            death_loot_table_seed: SyncMutex::new(0),
            leash_data: SyncMutex::new(None),
            ambient_sound_time: SyncMutex::new(0),
            xp_reward: SyncMutex::new(0),
        }
    }

    #[must_use]
    pub const fn goal_selector(&self) -> &SyncMutex<GoalSelector> {
        &self.goal_selector
    }

    #[must_use]
    pub const fn target_selector(&self) -> &SyncMutex<GoalSelector> {
        &self.target_selector
    }

    #[must_use]
    pub(crate) const fn sensing(&self) -> &SyncMutex<Sensing> {
        &self.sensing
    }

    #[must_use]
    pub fn target(&self, is_valid: impl Fn(&dyn LivingEntity) -> bool) -> Option<SharedEntity> {
        let mut target = self.target.lock();
        let Some(upgraded) = target.as_ref().and_then(WeakEntity::upgrade) else {
            *target = None;
            return None;
        };
        let living_target = upgraded.as_living_entity()?;
        if !is_valid(living_target) {
            return None;
        }
        Some(upgraded)
    }

    pub fn set_target(
        &self,
        target: Option<&SharedEntity>,
        is_valid: impl Fn(&dyn LivingEntity) -> bool,
    ) -> bool {
        let Some(target) = target else {
            *self.target.lock() = None;
            return true;
        };
        if !target.is_living_entity() {
            return false;
        }
        let Some(living_target) = target.as_living_entity() else {
            return false;
        };
        if !is_valid(living_target) {
            *self.target.lock() = None;
            return false;
        }

        *self.target.lock() = Some(Arc::downgrade(target));
        true
    }

    #[must_use]
    pub const fn controls(&self) -> &SyncMutex<MobControls> {
        &self.controls
    }

    #[must_use]
    pub const fn navigation(&self) -> &SyncMutex<PathNavigation> {
        &self.navigation
    }

    #[must_use]
    pub const fn pathfinding_malus(&self) -> &SyncMutex<PathfindingMalus> {
        &self.pathfinding_malus
    }

    #[must_use]
    pub const fn persistence_required(&self) -> &SyncMutex<bool> {
        &self.persistence_required
    }

    pub const fn can_pick_up_loot(&self) -> &SyncMutex<bool> {
        &self.can_pick_up_loot
    }

    const fn drop_chances(&self) -> &SyncMutex<DropChances> {
        &self.drop_chances
    }

    const fn home_restriction(&self) -> &SyncMutex<MobHomeRestriction> {
        &self.home_restriction
    }

    const fn death_loot_table(&self) -> &SyncMutex<Option<Identifier>> {
        &self.death_loot_table
    }

    const fn death_loot_table_seed(&self) -> &SyncMutex<i64> {
        &self.death_loot_table_seed
    }

    const fn leash_data(&self) -> &SyncMutex<Option<LeashData>> {
        &self.leash_data
    }

    #[must_use]
    pub fn ambient_sound_time(&self) -> i32 {
        *self.ambient_sound_time.lock()
    }

    pub fn set_ambient_sound_time(&self, ambient_sound_time: i32) {
        *self.ambient_sound_time.lock() = ambient_sound_time;
    }

    fn get_and_increment_ambient_sound_time(&self) -> i32 {
        let mut ambient_sound_time = self.ambient_sound_time.lock();
        let previous = *ambient_sound_time;
        *ambient_sound_time += 1;
        previous
    }

    #[must_use]
    pub fn xp_reward(&self) -> i32 {
        *self.xp_reward.lock()
    }

    pub fn set_xp_reward(&self, xp_reward: i32) {
        *self.xp_reward.lock() = xp_reward;
    }
}

impl Default for MobBase {
    fn default() -> Self {
        Self::new()
    }
}

/// Ticks an undead mob burns for when caught in sunlight.
///
/// Vanilla parity: the `igniteForSeconds(8.0F)` of `Mob.burnUndead`.
const DAYLIGHT_BURN_TICKS: i32 = 160;

/// Object-safe access to a mob trait object from default `Mob` methods.
///
/// Same shape, and same reason, as [`crate::entity::EntityEventSource`]: `Self`
/// is `?Sized` inside a default method, so it cannot coerce itself.
pub trait MobSource {
    /// Returns this mob as a trait object.
    fn as_mob_source(&self) -> &dyn Mob;
}

impl<T: Mob> MobSource for T {
    fn as_mob_source(&self) -> &dyn Mob {
        self
    }
}

pub trait Mob: LivingEntity + MobSource {
    fn mob_base(&self) -> &MobBase;

    fn mob_flags(&self) -> i8;

    fn set_mob_flags(&self, flags: i8);

    /// Returns vanilla `Mob.isSaddled`.
    fn is_saddled(&self) -> bool {
        let mut is_saddled = false;
        self.with_equipment_slot(EquipmentSlot::Saddle, &mut |item_stack| {
            is_saddled = self.is_equippable_in_slot(item_stack, EquipmentSlot::Saddle);
        });
        is_saddled
    }

    /// Returns whether this mob is a vanilla `Monster`.
    ///
    /// Vanilla parity: the `instanceof Monster` tests that goals such as
    /// `Rabbit.RabbitAvoidEntityGoal` and `Fox` use. Foton has no entity class
    /// hierarchy, so each mob that vanilla derives from `Monster` says so here.
    /// A slime is deliberately not one: vanilla derives it from `AbstractCubeMob`
    /// and only tags it `Enemy`, which is why a rabbit never flees a slime.
    fn is_monster(&self) -> bool {
        false
    }

    /// Returns this mob's brain, when it has one.
    ///
    /// Vanilla parity: `LivingEntity.getBrain`. Vanilla gives every
    /// `LivingEntity` a brain and lets `makeBrain` return an empty one, which
    /// `Brain.isBrainDead` then has to detect. Foton's brain owns three mutexes
    /// and two maps, and every goal-driven mob in the tree would pay for one it
    /// never reads, so a mob opts in by overriding this and holding a
    /// [`crate::entity::ai::brain::Brain`] of its own.
    fn brain(&self) -> Option<&Brain> {
        None
    }

    fn custom_server_ai_step(&self) {}

    /// Runs vanilla `Mob.ate`, invoked after an eating goal resolves a block.
    fn ate(&self) {}

    fn tick_goal_selectors(&self) {}

    fn xp_reward(&self) -> i32 {
        self.mob_base().xp_reward()
    }

    fn set_xp_reward(&self, xp_reward: i32) {
        self.mob_base().set_xp_reward(xp_reward);
    }

    /// Returns vanilla `Mob.getTarget`.
    fn target(&self) -> Option<SharedEntity> {
        self.mob_base()
            .target(|target| self.is_valid_target(target))
    }

    /// Returns whatever this mob's brain is attacking.
    ///
    /// Vanilla parity: `Mob.getTargetFromBrain`. A brain mob has no `target`
    /// field of its own -- the memory is the target -- so a mob whose `getTarget`
    /// is `getTargetFromBrain` overrides [`Self::target`] with this.
    fn target_from_brain(&self) -> Option<SharedEntity> {
        let target = self
            .brain()?
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())?;
        let living = target.as_living_entity()?;
        if !self.is_valid_target(living) {
            return None;
        }
        Some(target)
    }

    /// Sets vanilla `Mob.target`.
    ///
    /// Returns `false` when the supplied entity is not a living entity.
    fn set_target(&self, target: Option<&SharedEntity>) -> bool {
        self.mob_base()
            .set_target(target, |target| self.is_valid_target(target))
    }

    fn is_valid_target(&self, target: &dyn LivingEntity) -> bool {
        if target
            .as_player()
            .is_some_and(|player| player.has_infinite_materials() || player.is_spectator())
        {
            return false;
        }

        Mob::can_attack(self, target)
    }

    /// Returns vanilla `Mob.canAttack`.
    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        self.mob_can_attack(target)
    }

    /// The body of [`Self::can_attack`], callable from an override.
    ///
    /// Rust has no `super`, so a mob that only adds a condition -- the dolphin,
    /// whose calves pick no fights -- calls this for the rest. Going through
    /// [`Self::is_valid_target`] instead would recurse.
    fn mob_can_attack(&self, target: &dyn LivingEntity) -> bool {
        target.entity_type() != &vanilla_entities::GHAST && LivingEntity::can_attack(self, target)
    }

    fn base_experience_reward_mob(&self) -> i32 {
        let xp_reward = self.xp_reward();
        if xp_reward <= 0 {
            return xp_reward;
        }

        let mut result = xp_reward;
        for slot in EquipmentSlot::ALL {
            if !slot.can_increase_experience() {
                continue;
            }

            let should_increase = {
                let equipment = self.living_base().equipment().lock();
                !equipment.get_ref(slot).is_empty() && self.equipment_drop_chance(slot) <= 1.0
            };
            if should_increase {
                result += 1 + rand::random_range(0..3);
            }
        }
        result
    }

    fn ambient_sound_interval(&self) -> i32 {
        if let Some(animal) = self.as_animal() {
            return animal.ambient_sound_interval_animal();
        }

        80
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    fn play_ambient_sound(&self) {
        self.make_sound(self.ambient_sound());
    }

    fn reset_ambient_sound_time(&self) {
        self.mob_base()
            .set_ambient_sound_time(-self.ambient_sound_interval());
    }

    /// Runs vanilla `Mob.baseTick`.
    fn base_tick_mob(&self) {
        self.base_tick_living_entity();
        self.mob_base_tick();
    }

    /// Runs the mob-owned portion of vanilla `Mob.baseTick`.
    fn mob_base_tick(&self) {
        if !LivingEntity::is_alive(self) {
            return;
        }

        self.burn_in_daylight();

        let ambient_sound_time = self.mob_base().get_and_increment_ambient_sound_time();
        if rand::random_range(0..1000) < ambient_sound_time {
            self.reset_ambient_sound_time();
            self.play_ambient_sound();
        }
    }

    /// Sets undead mobs alight when the sun reaches them.
    ///
    /// Vanilla parity: `Mob.burnUndead`, gated by the `burn_in_daylight` entity
    /// tag, so the behavior is data-driven rather than hard-coded per mob.
    fn burn_in_daylight(&self) {
        if !REGISTRY
            .entity_types
            .is_in_tag(self.entity_type(), &EntityTypeTag::BURN_IN_DAYLIGHT)
        {
            return;
        }
        if !LivingEntity::is_alive(self) || !self.is_sun_burn_tick() {
            return;
        }

        // Vanilla parity: `Mob.burnUndead` spends the helmet instead of the mob.
        // A hat is the whole reason a zombie can cross an open field at noon.
        let slot = self.sun_protection_slot();
        let mut wearing_a_hat = false;
        let mut broke_the_hat = false;
        self.with_equipment_slot_mut(slot, &mut |sun_blocker| {
            if sun_blocker.is_empty() {
                return;
            }
            wearing_a_hat = true;
            if !sun_blocker.is_damageable_item() {
                return;
            }
            sun_blocker.set_damage_value(sun_blocker.get_damage_value() + rand::random_range(0..2));
            if sun_blocker.get_damage_value() >= sun_blocker.get_max_damage() {
                broke_the_hat = true;
                *sun_blocker = ItemStack::empty();
            }
        });

        if broke_the_hat {
            self.on_equipped_item_broken(slot);
        }
        if !wearing_a_hat {
            self.ignite_for_ticks(DAYLIGHT_BURN_TICKS);
        }
    }

    /// The slot an undead mob can keep the sun off with.
    ///
    /// Vanilla parity: `Mob.sunProtectionSlot`, which the zombie horse and the
    /// zombie nautilus override.
    fn sun_protection_slot(&self) -> EquipmentSlot {
        EquipmentSlot::Head
    }

    /// Returns whether the sun is currently strong enough to set this mob alight.
    ///
    /// Vanilla parity: `Mob.isSunBurnTick`. The roll against the local
    /// brightness is why a zombie in the shade of a tree survives, and why one
    /// standing on open ground can last a few ticks before catching.
    fn is_sun_burn_tick(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        // Deviation: 26.2 reads the `gameplay/monsters_burn` environment
        // attribute off the dimension's timeline. Foton has no timelines, and
        // the vanilla overworld track turns the attribute on for exactly the
        // daylight `isBrightOutside` already describes.
        if !world.is_bright_outside() {
            return false;
        }

        let brightness = world.light_level_dependent_magic_value(self.block_position());
        if brightness <= 0.5 {
            return false;
        }
        if rand::random::<f32>() * 30.0 >= (brightness - 0.4) * 2.0 {
            return false;
        }
        if self.is_in_water_or_rain() || self.is_in_powder_snow() || self.was_in_powder_snow() {
            return false;
        }

        let position = self.position();
        world.can_see_sky(BlockPos::containing(
            position.x,
            self.get_eye_y(),
            position.z,
        ))
    }

    /// Returns whether this mob accepts the spot the spawner picked for it.
    ///
    /// Vanilla parity: the predicate registered next to the entity type in
    /// `SpawnPlacements`, defaulting to `Mob.checkMobSpawnRules`. Vanilla tests
    /// it before creating anything; Foton creates the mob and asks it, because
    /// nothing leads from an entity type to its behavior without an instance.
    /// A mob that answers no is dropped, unspawned.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_mob_spawn_rules(world, spawn_reason, pos)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    fn finalize_spawn_mob_base(
        &self,
        _world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let needs_random_spawn_bonus = !self
            .attributes()
            .lock()
            .has_modifier(vanilla_attributes::FOLLOW_RANGE, &RANDOM_SPAWN_BONUS_ID);
        let random_spawn_bonus = needs_random_spawn_bonus
            .then(|| RANDOM_SPAWN_BONUS_SCALE * (rand::random::<f64>() - rand::random::<f64>()));
        let left_handed = rand::random::<f32>() < LEFT_HANDED_SPAWN_CHANCE;

        if let Some(amount) = random_spawn_bonus {
            self.attributes().lock().add_modifier(
                vanilla_attributes::FOLLOW_RANGE,
                AttributeModifier {
                    id: RANDOM_SPAWN_BONUS_ID,
                    amount,
                    operation: AttributeModifierOperation::AddMultipliedBase,
                },
                true,
            );
        }
        self.set_left_handed(left_handed);
        group_data
    }

    /// Rolls whether this mob spawned able to take loot off the ground.
    ///
    /// Vanilla parity: the `setCanPickUpLoot(random.nextFloat() < 0.55F *
    /// difficulty.getSpecialMultiplier())` that `Zombie.finalizeSpawn` and
    /// `AbstractSkeleton.finalizeSpawn` both run. Foton has no class hierarchy
    /// to inherit it from, so every mob in those two families calls this. The
    /// multiplier is zero below local difficulty 2, which is why an easy-mode
    /// zombie never turns up wearing your armor.
    fn roll_spawn_can_pick_up_loot(&self, world: &World) {
        let difficulty = world.get_current_difficulty_at(Entity::block_position(self));
        let chance = SPAWN_CAN_PICK_UP_LOOT_CHANCE * difficulty.special_multiplier();
        Mob::set_can_pick_up_loot(self, rand::random::<f32>() < chance);
    }

    /// Handles vanilla `Mob.interact`.
    fn interact_mob(
        &self,
        player: &Player,
        hand: InteractionHand,
        location: DVec3,
    ) -> InteractionResult {
        if !LivingEntity::is_alive(self) {
            return InteractionResult::Pass;
        }

        let interaction_result = self.check_and_handle_important_interactions(player, hand);
        if interaction_result.consumes_action() {
            self.emit_entity_interact_game_event(player);
            return interaction_result;
        }

        let interaction_result = self.interact_entity(player, hand, location);
        if interaction_result != InteractionResult::Pass {
            return interaction_result;
        }

        let interaction_result = self.mob_interact(player, hand);
        if interaction_result.consumes_action() {
            self.emit_entity_interact_game_event(player);
        }

        interaction_result
    }

    /// Handles the two items that mean the same thing on every mob, whatever
    /// its own `mob_interact` would rather do with a right click.
    ///
    /// Vanilla parity: `Mob.checkAndHandleImportantInteractions`. Running first
    /// is the whole point of it: a villager's `mob_interact` opens its trades
    /// and a horse's puts you in the saddle, so a name tag held out to either
    /// would never be read if this came second.
    fn check_and_handle_important_interactions(
        &self,
        player: &Player,
        hand: InteractionHand,
    ) -> InteractionResult {
        let this = self.as_mob_source();
        let held = InventoryAccess::new(player.inventory.clone(), hand);
        let (is_name_tag, item) =
            held.with_item(|stack| (stack.is(&vanilla_items::NAME_TAG), stack.item()));
        let behavior = ITEM_BEHAVIORS.get_behavior(item);

        if is_name_tag {
            let result =
                held.with_item(|stack| behavior.interact_living_entity(stack, player, this, hand));
            if result.consumes_action() {
                return result;
            }
        }

        if !behavior.is_spawn_egg() {
            return InteractionResult::Pass;
        }

        // Vanilla's `level() instanceof ServerLevel` guard. Foton only ever runs
        // the server side, so the only way past it is a mob with no world at
        // all, which vanilla's client branch answers the same way.
        let Some(world) = self.level() else {
            return InteractionResult::SuccessServer;
        };

        let offspring = held.with_item(|stack| {
            SpawnEggItem::spawn_offspring_from_spawn_egg(player, this, &world, stack)
        });
        let Some(offspring) = offspring else {
            // A wrong egg for this mob falls through to the ordinary
            // interaction rather than eating the click.
            return InteractionResult::Pass;
        };
        if let Some(baby) = offspring.as_mob() {
            self.on_offspring_spawned_from_egg(player, baby);
        }

        InteractionResult::SuccessServer
    }

    /// Applies whatever the parent wants said about a baby a spawn egg made.
    ///
    /// Vanilla parity: `Mob.onOffspringSpawnedFromEgg`, a no-op that `Fox` and
    /// `Zombie` override.
    fn on_offspring_spawned_from_egg(&self, _spawner: &Player, _offspring: &dyn Mob) {}

    /// Emits vanilla's `GameEvent.ENTITY_INTERACT` for a consumed interaction.
    fn emit_entity_interact_game_event(&self, player: &Player) {
        let Some(world) = self.level() else {
            return;
        };
        world.game_event(
            &vanilla_game_events::ENTITY_INTERACT,
            self.block_position(),
            &GameEventContext::new(Some(player), None),
        );
    }

    /// Handles vanilla `Mob.mobInteract`.
    fn mob_interact(&self, _player: &Player, _hand: InteractionHand) -> InteractionResult {
        InteractionResult::Pass
    }

    /// Returns vanilla `Mob.canShearEquipment`.
    fn can_shear_equipment(&self, _player: &Player) -> bool {
        !self.is_vehicle()
    }

    /// Applies vanilla `Mob.usePlayerItem`.
    fn use_player_item(&self, player: &Player, hand: InteractionHand) {
        player.inventory.lock().shrink_item_in_hand(hand, 1);
        // TODO: Apply USE_REMAINDER components once item use-remainder support exists.
    }

    fn remove_when_far_away(&self, dist_sqr: f64) -> bool {
        self.as_animal()
            .is_none_or(|animal| animal.remove_when_far_away_animal(dist_sqr))
    }

    fn requires_custom_persistence(&self) -> bool {
        self.is_passenger() || self.is_leashed()
    }

    fn is_persistence_required(&self) -> bool {
        *self.mob_base().persistence_required().lock()
    }

    fn set_persistence_required(&self) {
        *self.mob_base().persistence_required().lock() = true;
    }

    /// Returns vanilla `Mob.canPickUpLoot`.
    fn can_pick_up_loot(&self) -> bool {
        *self.mob_base().can_pick_up_loot().lock()
    }

    fn set_can_pick_up_loot(&self, can_pick_up_loot: bool) {
        *self.mob_base().can_pick_up_loot().lock() = can_pick_up_loot;
    }

    fn equipment_drop_chance(&self, slot: EquipmentSlot) -> f32 {
        self.mob_base().drop_chances().lock().by_equipment(slot)
    }

    fn is_equipment_drop_preserved(&self, slot: EquipmentSlot) -> bool {
        self.mob_base().drop_chances().lock().is_preserved(slot)
    }

    fn set_guaranteed_drop(&self, slot: EquipmentSlot) {
        self.mob_base()
            .drop_chances()
            .lock()
            .set_guaranteed_drop(slot);
    }

    /// Returns how likely one slot is to drop.
    ///
    /// Vanilla parity: `Mob.getDropChances().byEquipment`.
    fn drop_chance(&self, slot: EquipmentSlot) -> f32 {
        self.mob_base().drop_chances().lock().by_equipment(slot)
    }

    /// Sets how likely one slot is to drop, refusing a negative chance.
    ///
    /// Vanilla parity: `Mob.setDropChance`.
    fn set_drop_chance(&self, slot: EquipmentSlot, chance: f32) {
        self.mob_base()
            .drop_chances()
            .lock()
            .set_equipment_chance(slot, chance);
    }

    /// Plays the puff of smoke a spawner makes when it produces a mob.
    ///
    /// Vanilla parity: `Mob.spawnAnim`, whose server half is one entity event.
    /// The particles themselves are the client's answer to it.
    fn spawn_anim(&self) {
        self.broadcast_entity_event(EntityStatus::SilverfishMergeAnim);
    }

    /// Dresses this mob from a loot table.
    ///
    /// Vanilla parity: `Mob.equip(EquipmentTable)` through
    /// `EquipmentUser.equip`. Vanilla threads a `LootParams` built from
    /// `LootContextParamSets.EQUIPMENT`; Foton's loot context takes the same
    /// two facts -- where the mob is and which mob it is.
    fn equip_from_table(&self, world: &Arc<World>, table: &EquipmentTable) {
        let mut rng = rand::rng();
        let position = self.position();
        let mut context = LootContext::new(&mut rng)
            .with_origin(position.x, position.y, position.z)
            .with_this_entity(entity_loot_ref(self.as_entity_event_source()))
            .with_game_time(world.game_time());
        let rolled = table.loot_table.get_random_items(&mut context);

        let mut filled: Vec<EquipmentSlot> = Vec::new();
        for stack in rolled {
            let Some(slot) = resolve_equipment_slot(&stack, &filled) else {
                continue;
            };
            // Vanilla parity: `EquipmentSlot.limit`, which keeps a single item
            // in every slot but the main hand.
            let equipped = if slot == EquipmentSlot::MainHand {
                stack
            } else {
                stack.copy_with_count(1)
            };
            self.living_base().equipment().lock().set(slot, equipped);
            if let Some(chance) = table.slot_drop_chances.get(&slot) {
                self.set_drop_chance(slot, *chance);
            }
            filled.push(slot);
        }
    }

    /// Drops the equipment this mob was told to keep.
    ///
    /// Vanilla parity: `Mob.dropPreservedEquipment`.
    fn drop_preserved_equipment(&self, _world: &Arc<World>) {
        for slot in EquipmentSlot::ALL {
            if !self.is_equipment_drop_preserved(slot) {
                continue;
            }
            let mut taken = ItemStack::empty();
            self.with_equipment_slot_mut(slot, &mut |item| {
                taken = item.copy_with_count(item.count());
                *item = ItemStack::empty();
            });
            if !taken.is_empty() {
                let _ = self.spawn_at_location(taken, 0.0);
            }
        }
    }

    fn drop_custom_death_loot_mob(&self, _source: &DamageSource, killed_by_player: bool) {
        if self.level().is_none() {
            return;
        }

        for slot in EquipmentSlot::ALL {
            let drop_chance = self.equipment_drop_chance(slot);
            let preserve = self.is_equipment_drop_preserved(slot);
            if !can_attempt_equipment_drop(drop_chance, preserve, killed_by_player) {
                continue;
            }

            let can_drop_item = {
                let equipment = self.living_base().equipment().lock();
                let item_stack = equipment.get_ref(slot);
                !item_stack.is_empty()
                    && !item_stack
                        .has_enchantment_effect(EnchantmentEffectComponent::PreventEquipmentDrop)
            };
            if !can_drop_item {
                continue;
            }

            // TODO: Apply EquipmentDrops enchantment value effects once damage
            // sources can resolve their living attacker context.
            let random_roll = rand::random::<f32>();
            if random_roll >= drop_chance {
                continue;
            }

            let mut item_stack = {
                let mut equipment = self.living_base().equipment().lock();
                let item_stack = equipment.get_ref(slot);
                if item_stack.is_empty()
                    || item_stack
                        .has_enchantment_effect(EnchantmentEffectComponent::PreventEquipmentDrop)
                {
                    continue;
                }

                equipment.take(slot)
            };
            if !preserve && item_stack.is_damageable_item() {
                let max_damage = item_stack.get_max_damage();
                let inner = rand::random_range(0..(max_damage - 3).max(1));
                let damage = max_damage - rand::random_range(0..=inner);
                item_stack.set_damage_value(damage);
            }

            self.spawn_at_location(item_stack, 0.0);
        }
    }

    fn save_mob(&self, nbt: &mut NbtCompound) {
        nbt.insert("CanPickUpLoot", i8::from(self.can_pick_up_loot()));
        nbt.insert(
            "PersistenceRequired",
            i8::from(self.is_persistence_required()),
        );
        self.mob_base().drop_chances().lock().save(nbt);
        if let Some(leash_data) = self.mob_base().leash_data().lock().as_ref() {
            leash_data.save(nbt);
        }

        if self.has_home() {
            let home = *self.mob_base().home_restriction().lock();
            nbt.insert("home_radius", home.radius);
            nbt.insert(
                "home_pos",
                NbtTag::IntArray(vec![
                    home.position.x(),
                    home.position.y(),
                    home.position.z(),
                ]),
            );
        }

        nbt.insert("LeftHanded", i8::from(self.is_left_handed()));
        if let Some(loot_table) = self.mob_base().death_loot_table().lock().as_ref() {
            nbt.insert("DeathLootTable", loot_table.to_string());
        }
        let loot_table_seed = *self.mob_base().death_loot_table_seed().lock();
        if loot_table_seed != 0 {
            nbt.insert("DeathLootTableSeed", loot_table_seed);
        }
        if self.is_no_ai() {
            nbt.insert("NoAI", i8::from(true));
        }
    }

    fn load_mob(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.set_can_pick_up_loot(nbt.byte("CanPickUpLoot").is_some_and(|value| value != 0));
        *self.mob_base().persistence_required().lock() = nbt
            .byte("PersistenceRequired")
            .is_some_and(|value| value != 0);
        *self.mob_base().drop_chances().lock() = DropChances::load(nbt);
        *self.mob_base().leash_data().lock() = LeashData::load(nbt);
        let home_radius = nbt.int("home_radius").unwrap_or(-1);
        if home_radius >= 0 {
            let home_position = nbt
                .int_array("home_pos")
                .filter(|position| position.len() == 3)
                .map_or(BlockPos::ZERO, |position| {
                    BlockPos::new(position[0], position[1], position[2])
                });
            self.set_home_to(home_position, home_radius);
        } else {
            self.clear_home();
        }

        self.set_left_handed(nbt.byte("LeftHanded").is_some_and(|value| value != 0));
        let death_loot_table = nbt
            .string("DeathLootTable")
            .and_then(|loot_table| loot_table.to_str().as_ref().parse().ok());
        *self.mob_base().death_loot_table().lock() = death_loot_table;
        *self.mob_base().death_loot_table_seed().lock() =
            nbt.long("DeathLootTableSeed").unwrap_or(0);
        self.set_no_ai(nbt.byte("NoAI").is_some_and(|value| value != 0));
    }

    fn set_death_loot_table(&self, loot_table: Option<Identifier>) {
        *self.mob_base().death_loot_table().lock() = loot_table;
    }

    fn set_death_loot_table_seed(&self, seed: i64) {
        *self.mob_base().death_loot_table_seed().lock() = seed;
    }

    fn custom_death_loot_table(&self) -> Option<LootTableRef> {
        self.mob_base()
            .death_loot_table()
            .lock()
            .as_ref()
            .and_then(|key| REGISTRY.loot_tables.by_key(key))
    }

    fn has_custom_death_loot_table(&self) -> bool {
        self.mob_base().death_loot_table().lock().is_some()
    }

    fn death_loot_table_seed(&self) -> i64 {
        *self.mob_base().death_loot_table_seed().lock()
    }

    fn clear_custom_death_loot_table(&self) {
        *self.mob_base().death_loot_table().lock() = None;
    }

    fn is_leashed(&self) -> bool {
        self.leash_holder().is_some()
    }

    fn may_be_leashed(&self) -> bool {
        self.mob_base().leash_data().lock().is_some()
    }

    fn leash_holder(&self) -> Option<SharedEntity> {
        self.mob_base()
            .leash_data()
            .lock()
            .as_ref()
            .and_then(LeashData::holder)
    }

    fn leash_attachment(&self) -> Option<LeashAttachment> {
        self.mob_base()
            .leash_data()
            .lock()
            .as_ref()
            .map(LeashData::saved_attachment)
    }

    fn set_delayed_leash_attachment(&self, attachment: LeashAttachment) {
        *self.mob_base().leash_data().lock() = Some(LeashData::from_delayed_attachment(attachment));
    }

    fn can_be_leashed(&self) -> bool {
        // TODO: Return false for enemy mobs once hostile mob foundations exist.
        true
    }

    fn leash_distance_to(&self, holder: &dyn Entity) -> f64 {
        leash_bounding_box_center(self.as_entity_event_source())
            .distance(leash_bounding_box_center(holder))
    }

    fn leash_snap_distance(&self) -> f64 {
        LEASH_SNAP_DISTANCE
    }

    fn leash_elastic_distance(&self) -> f64 {
        LEASH_ELASTIC_DISTANCE
    }

    fn when_leashed_to(&self, holder: &dyn Entity) {
        holder.notify_leash_holder(self.as_entity_event_source());
    }

    fn leash_too_far_behavior(&self) {
        self.drop_leash();
    }

    fn on_elastic_leash_pull(&self) {
        self.check_fall_distance_accumulation();
    }

    fn close_range_leash_behavior(&self, _holder: &dyn Entity) {}

    /// Returns whether this mob hangs four leads off a single holder.
    ///
    /// Vanilla parity: `Leashable.supportQuadLeash`. Only a mob whose holder
    /// also answers `support_quad_leash_as_holder` gets the four-rope
    /// treatment; anything else stays on the single-rope path.
    fn support_quad_leash(&self) -> bool {
        false
    }

    /// Vanilla parity: `Leashable.checkElasticInteractions`.
    fn check_elastic_interactions(&self, holder: &dyn Entity) -> bool {
        let quad_connection = holder.support_quad_leash_as_holder() && self.support_quad_leash();
        let (entity_attachment_points, leasher_attachment_points): (&[DVec3], &[DVec3]) =
            if quad_connection {
                (
                    &SHARED_QUAD_ATTACHMENT_POINTS,
                    &SHARED_QUAD_ATTACHMENT_POINTS,
                )
            } else {
                (&ENTITY_LEASH_ATTACHMENT_POINT, &LEASHER_ATTACHMENT_POINT)
            };

        let Some(wrench) = compute_elastic_interaction(
            self.as_entity_event_source(),
            holder,
            self.leash_elastic_distance(),
            entity_attachment_points,
            leasher_attachment_points,
        ) else {
            return false;
        };
        let wrench = if quad_connection {
            wrench.scale(QUAD_LEASH_WRENCH_SCALE)
        } else {
            wrench
        };

        {
            let mut leash_data = self.mob_base().leash_data().lock();
            let Some(leash_data) = leash_data.as_mut() else {
                return false;
            };
            leash_data.angular_momentum += LEASH_TORSIONAL_ELASTICITY * wrench.torque;
        }

        let relative_velocity_to_leasher =
            leash_holder_movement(holder) - leash_holder_movement(self.as_entity_event_source());
        self.push_impulse(
            axis_specific_leash_elasticity(wrench.force)
                + relative_velocity_to_leasher * LEASH_STIFFNESS,
        );
        true
    }

    fn apply_leash_angular_momentum(&self) -> bool {
        let angular_friction = self.leash_angular_friction();
        let angular_momentum = {
            let mut leash_data = self.mob_base().leash_data().lock();
            let Some(leash_data) = leash_data.as_mut() else {
                return false;
            };
            let angular_momentum = leash_data.angular_momentum;
            leash_data.angular_momentum *= angular_friction;
            angular_momentum
        };
        self.rotate_by_leash_angular_momentum(angular_momentum);
        true
    }

    fn rotate_by_leash_angular_momentum(&self, angular_momentum: f64) {
        let (yaw, pitch) = self.rotation();
        self.set_rotation((yaw - angular_momentum as f32, pitch));
    }

    fn leash_angular_momentum(&self) -> Option<f64> {
        self.mob_base()
            .leash_data()
            .lock()
            .as_ref()
            .map(|leash_data| leash_data.angular_momentum)
    }

    fn leash_angular_friction(&self) -> f64 {
        if self.on_ground() {
            let Some(world) = self.level() else {
                return 0.91;
            };
            let Some(pos) = self.block_pos_below_that_affects_movement() else {
                return 0.91;
            };
            return f64::from(world.get_block_state(pos).get_block().config.friction * 0.91);
        }

        if self.is_in_water() || self.is_in_lava() {
            return 0.8;
        }

        0.91
    }

    fn can_have_a_leash_attached_to(&self, holder: &dyn Entity) -> bool {
        self.id() != holder.id()
            && self.leash_distance_to(holder) <= self.leash_snap_distance()
            && self.can_be_leashed()
    }

    fn set_leashed_to(&self, holder: &SharedEntity) -> bool {
        if self.id() == holder.id() {
            return false;
        }

        let old_holder = self.leash_holder();
        {
            let mut leash_data = self.mob_base().leash_data().lock();
            if let Some(leash_data) = leash_data.as_mut() {
                leash_data.set_holder(holder);
            } else {
                *leash_data = Some(LeashData::from_entity(holder));
            }
        }

        if self.is_passenger() {
            self.stop_riding();
        }
        if let Some(old_holder) = old_holder
            && old_holder.id() != holder.id()
        {
            old_holder.notify_leashee_removed(self.as_entity_event_source());
        }
        true
    }

    fn tick_leash(&self) {
        if let Some(holder) = self.leash_holder() {
            if !self.can_interact_with_level() || !holder.can_interact_with_level() {
                if let Some(world) = self.level()
                    && world.get_game_rule(&ENTITY_DROPS)
                {
                    self.drop_leash();
                } else {
                    self.remove_leash();
                }
                return;
            }

            let distance_to = self.leash_distance_to(holder.as_ref());
            self.when_leashed_to(holder.as_ref());
            let angular_momentum_before_distance_action = self.leash_angular_momentum();
            if distance_to > self.leash_snap_distance() {
                if let Some(world) = self.level() {
                    world.play_sound_at(
                        &sound_events::ITEM_LEAD_BREAK,
                        SoundSource::Neutral,
                        holder.position(),
                        1.0,
                        1.0,
                        None,
                    );
                }
                self.leash_too_far_behavior();
            } else if distance_to
                > self.leash_elastic_distance()
                    - f64::from(holder.base().dimensions().width)
                    - f64::from(self.base().dimensions().width)
                && self.check_elastic_interactions(holder.as_ref())
            {
                self.on_elastic_leash_pull();
            } else {
                self.close_range_leash_behavior(holder.as_ref());
            }
            if !self.apply_leash_angular_momentum()
                && let Some(angular_momentum) = angular_momentum_before_distance_action
            {
                self.rotate_by_leash_angular_momentum(angular_momentum);
            }
            return;
        }

        let Some(attachment) = self.leash_attachment() else {
            return;
        };

        let Some(world) = self.level() else {
            return;
        };

        match attachment {
            LeashAttachment::Entity(uuid) => {
                if let Some(holder) = world.get_entity_by_uuid(&uuid) {
                    let _ = self.set_leashed_to(&holder);
                    return;
                }

                if self.tick_count() > DELAYED_LEASH_DROP_TICKS {
                    let _ = self.spawn_at_location(ItemStack::new(&vanilla_items::LEAD), 0.0);
                    self.remove_leash_state();
                }
            }
            LeashAttachment::FenceKnot(pos) => {
                if let Some(holder) = LeashFenceKnotEntity::get_or_create_knot(&world, pos) {
                    let _ = self.set_leashed_to(&holder);
                    return;
                }

                if self.tick_count() > DELAYED_LEASH_DROP_TICKS {
                    let _ = self.spawn_at_location(ItemStack::new(&vanilla_items::LEAD), 0.0);
                    self.remove_leash_state();
                }
            }
        }
    }

    fn drop_leash(&self) {
        if self.leash_holder().is_none() {
            return;
        }

        let holder = self.remove_leash_state();
        let _ = self.spawn_at_location(ItemStack::new(&vanilla_items::LEAD), 0.0);
        if let Some(holder) = holder {
            holder.notify_leashee_removed(self.as_entity_event_source());
        }
    }

    fn remove_leash(&self) {
        if self.leash_holder().is_some()
            && let Some(holder) = self.remove_leash_state()
        {
            holder.notify_leashee_removed(self.as_entity_event_source());
        }
    }

    fn remove_leash_state(&self) -> Option<SharedEntity> {
        self.mob_base()
            .leash_data()
            .lock()
            .take()
            .and_then(|leash_data| leash_data.holder())
    }

    fn is_within_home(&self) -> bool {
        self.is_within_home_pos(self.block_position())
    }

    fn is_within_home_pos(&self, pos: BlockPos) -> bool {
        let home = *self.mob_base().home_restriction().lock();
        home.radius == -1
            || block_pos_distance_sqr(home.position, pos) < home_radius_sqr(home.radius)
    }

    fn is_within_home_vec(&self, pos: DVec3) -> bool {
        let home = *self.mob_base().home_restriction().lock();
        home.radius == -1
            || block_center_distance_sqr(home.position, pos) < home_radius_sqr(home.radius)
    }

    fn set_home_to(&self, position: BlockPos, radius: i32) {
        *self.mob_base().home_restriction().lock() = MobHomeRestriction { position, radius };
    }

    fn home_position(&self) -> BlockPos {
        self.mob_base().home_restriction().lock().position
    }

    fn home_radius(&self) -> i32 {
        self.mob_base().home_restriction().lock().radius
    }

    fn clear_home(&self) {
        self.mob_base().home_restriction().lock().radius = -1;
    }

    fn has_home(&self) -> bool {
        self.home_radius() != -1
    }

    fn check_mob_despawn(&self) {
        if self
            .level()
            .is_some_and(|world| world.difficulty() == Difficulty::Peaceful)
            && !self.entity_type().allowed_in_peaceful
        {
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        if self.is_persistence_required() || self.requires_custom_persistence() {
            self.set_no_action_time(0);
            return;
        }

        let Some(nearest_player_dist_sqr) = self.nearest_player_distance_sqr() else {
            return;
        };

        let mob_category = self.entity_type().mob_category;
        let despawn_distance = mob_category.despawn_distance();
        let despawn_distance_sqr = despawn_distance * despawn_distance;
        if nearest_player_dist_sqr > f64::from(despawn_distance_sqr)
            && self.remove_when_far_away(nearest_player_dist_sqr)
        {
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        let no_despawn_distance = mob_category.no_despawn_distance();
        let no_despawn_distance_sqr = no_despawn_distance * no_despawn_distance;
        if self.no_action_time() > 600
            && nearest_player_dist_sqr > f64::from(no_despawn_distance_sqr)
            && self.remove_when_far_away(nearest_player_dist_sqr)
        {
            let should_discard = rand::random_range(0..800) == 0;
            if should_discard {
                self.set_removed(RemovalReason::Discarded);
            }
        } else if nearest_player_dist_sqr < f64::from(no_despawn_distance_sqr) {
            self.set_no_action_time(0);
        }
    }

    fn nearest_player_distance_sqr(&self) -> Option<f64> {
        let world = self.level()?;
        world.nearest_player_distance_sqr(self.position())
    }

    fn controlling_passenger_mob(&self) -> Option<SharedEntity> {
        let first_passenger = self.first_passenger()?;
        if self.is_no_ai() || !first_passenger.is_mob() || !first_passenger.can_control_vehicle() {
            return None;
        }

        Some(first_passenger)
    }

    fn get_pathfinding_malus(&self, path_type: PathType) -> f32 {
        self.mob_base().pathfinding_malus().lock().get(path_type)
    }

    /// Vanilla `Entity.getMaxFallDistance` baseline.
    fn max_fall_distance(&self) -> i32 {
        3
    }

    fn set_pathfinding_malus(&self, path_type: PathType, malus: f32) {
        self.mob_base()
            .pathfinding_malus()
            .lock()
            .set(path_type, malus);
    }

    fn is_no_ai(&self) -> bool {
        self.mob_flags() & MOB_FLAG_NO_AI != 0
    }

    fn set_no_ai(&self, no_ai: bool) {
        self.set_mob_flag(MOB_FLAG_NO_AI, no_ai);
    }

    fn is_left_handed(&self) -> bool {
        self.mob_flags() & MOB_FLAG_LEFT_HANDED != 0
    }

    fn set_left_handed(&self, left_handed: bool) {
        self.set_mob_flag(MOB_FLAG_LEFT_HANDED, left_handed);
    }

    fn is_aggressive(&self) -> bool {
        self.mob_flags() & MOB_FLAG_AGGRESSIVE != 0
    }

    /// Returns vanilla `Mob.getMaxHeadXRot`.
    fn max_head_x_rot(&self) -> f32 {
        40.0
    }

    /// Returns vanilla `Mob.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        75.0
    }

    /// Handles vanilla `Mob.doHurtTarget`.
    ///
    /// Override this to add what a specific mob does on a landed hit, and call
    /// [`Self::mob_do_hurt_target`] from the override for the shared behavior;
    /// Rust has no `super`, so the base body lives in its own method.
    #[must_use]
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        self.mob_do_hurt_target(world, target)
    }

    /// The shared part of vanilla `Mob.doHurtTarget`.
    #[must_use]
    fn mob_do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        let Some(attacker) = self.as_entity_event_source().as_living_entity() else {
            return false;
        };
        let weapon_item = {
            let mut main_hand = ItemStack::empty();
            self.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
                main_hand = item_stack.copy_with_count(item_stack.count());
            });
            main_hand
        };
        let attack_damage = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE) as f32;
        let damage_source = self.mob_attack_damage_source(&weapon_item, attacker);
        let enchantment_context = EnchantmentDamageContext::new(
            target.entity_type(),
            Some(self.entity_type()),
            Some(self.entity_type()),
            &damage_source,
        );
        let mut damage =
            enchantment_helper::modify_damage(&weapon_item, &enchantment_context, attack_damage);
        damage += ITEM_BEHAVIORS
            .get_behavior(weapon_item.item())
            .get_attack_damage_bonus(attacker, target.as_ref(), damage, &damage_source);

        let old_movement = target.velocity();
        let was_hurt = target.hurt(world, &damage_source, damage);
        if was_hurt {
            self.cause_extra_knockback(
                target.as_ref(),
                self.get_attack_knockback(target.as_ref(), &weapon_item, &damage_source),
                old_movement,
            );
            self.with_equipment_slot_mut(EquipmentSlot::MainHand, &mut |stack| {
                if stack.is_empty() {
                    return;
                }
                if let Some(living_target) = target.as_living_entity() {
                    ITEM_BEHAVIORS.get_behavior(stack.item()).hurt_enemy(
                        stack,
                        living_target,
                        attacker,
                    );
                }
            });
            let post_attack_context = EnchantmentPostAttackContext::new(
                target.as_ref(),
                Some(self.as_entity_event_source()),
                Some(self.as_entity_event_source()),
                &damage_source,
            );
            enchantment_helper::do_post_attack_effects_from_item(
                world,
                &weapon_item,
                &post_attack_context,
            );
            self.set_last_hurt_mob(Some(target));
            self.play_attack_sound();
        }

        if let Some(user) = self.as_entity_event_source().as_living_entity() {
            enchantment_helper::do_post_piercing_attack_effects(world, user);
        }
        was_hurt
    }

    /// Returns the damage source used by vanilla `ItemStack.getDamageSource`.
    fn mob_attack_damage_source(
        &self,
        weapon_item: &ItemStack,
        attacker: &dyn LivingEntity,
    ) -> DamageSource {
        let damage_source = if let Some(damage_type) = weapon_item.get_damage_type() {
            DamageSource::environment(damage_type)
        } else {
            ITEM_BEHAVIORS
                .get_behavior(weapon_item.item())
                .get_item_damage_source(attacker)
                .unwrap_or_else(|| DamageSource::environment(&vanilla_damage_types::MOB_ATTACK))
        };

        damage_source
            .with_causing_entity(self.id())
            .with_direct_entity(self.id())
            .with_source_position(self.position())
    }

    /// Returns vanilla `LivingEntity.getKnockback` for mob attacks.
    fn get_attack_knockback(
        &self,
        target: &dyn Entity,
        weapon_item: &ItemStack,
        damage_source: &DamageSource,
    ) -> f64 {
        let attack_knockback = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_KNOCKBACK);
        let enchantment_context = EnchantmentDamageContext::new(
            target.entity_type(),
            Some(self.entity_type()),
            Some(self.entity_type()),
            damage_source,
        );
        let modified = enchantment_helper::modify_knockback(
            weapon_item,
            &enchantment_context,
            attack_knockback as f32,
        );
        f64::from(modified) / 2.0
    }

    /// Applies vanilla `LivingEntity.causeExtraKnockback`.
    fn cause_extra_knockback(
        &self,
        target: &dyn Entity,
        knockback_amount: f64,
        _old_movement: DVec3,
    ) {
        if knockback_amount <= 0.0 {
            return;
        }
        let Some(living_target) = target.as_living_entity() else {
            return;
        };

        let yaw_radians = self.rotation().0.to_radians();
        let yaw_sin = f64::from(yaw_radians.sin());
        let yaw_cos = f64::from(yaw_radians.cos());
        living_target.knockback(knockback_amount, yaw_sin, -yaw_cos);

        let velocity = self.velocity();
        self.set_velocity(DVec3::new(velocity.x * 0.6, velocity.y, velocity.z * 0.6));
    }

    /// Turns toward `target`, no faster than the given limits.
    ///
    /// Vanilla parity: `Mob.lookAt(Entity, float, float)`. Unlike the look
    /// control this applies at once, which is why vanilla uses it for a mob
    /// that must be facing something by the end of the tick.
    fn look_at(&self, target: &dyn Entity, max_y_rot_increase: f32, max_x_rot_increase: f32) {
        let position = self.position();
        let target_position = target.position();
        let dx = target_position.x - position.x;
        let dz = target_position.z - position.z;
        let dy = if let Some(living) = target.as_living_entity() {
            living.get_eye_y() - self.get_eye_y()
        } else {
            let target_box = target.bounding_box();
            f64::midpoint(target_box.min(Axis::Y), target_box.max(Axis::Y)) - self.get_eye_y()
        };

        let horizontal = dx.hypot(dz);
        let wanted_yaw = dz.atan2(dx).to_degrees() as f32 - 90.0;
        let wanted_pitch = -(dy.atan2(horizontal).to_degrees()) as f32;
        let (yaw, pitch) = self.rotation();
        self.set_rotation((
            rotlerp(yaw, wanted_yaw, max_y_rot_increase),
            rotlerp(pitch, wanted_pitch, max_x_rot_increase),
        ));
    }

    /// Plays vanilla `LivingEntity.playAttackSound`.
    fn play_attack_sound(&self) {}

    /// Returns vanilla `Mob.isWithinMeleeAttackRange`.
    fn is_within_melee_attack_range(&self, target: &dyn LivingEntity) -> bool {
        // TODO: Use the held item's ATTACK_RANGE component once it has typed component data.
        let max_range = default_attack_reach();
        let min_range = 0.0;
        let target_hitbox = target.bounding_box();
        self.attack_bounding_box(max_range)
            .intersects(target_hitbox)
            && (min_range <= 0.0
                || !self
                    .attack_bounding_box(min_range)
                    .intersects(target_hitbox))
    }

    /// Returns vanilla `Mob.getAttackBoundingBox`.
    ///
    /// Override this to change a mob's reach -- a ravager's is narrower than
    /// its body -- and call [`Self::mob_attack_bounding_box`] from the override
    /// for the shared box; Rust has no `super`, so the base body lives in its
    /// own method.
    fn attack_bounding_box(&self, horizontal_expansion: f64) -> WorldAabb {
        self.mob_attack_bounding_box(horizontal_expansion)
    }

    /// Runs the shared body of [`Self::attack_bounding_box`].
    fn mob_attack_bounding_box(&self, horizontal_expansion: f64) -> WorldAabb {
        let own_aabb = self.bounding_box();
        let base = if let Some(vehicle) = self.vehicle() {
            let mount_aabb = vehicle.bounding_box();
            WorldAabb::new(
                own_aabb.min_x().min(mount_aabb.min_x()),
                own_aabb.min_y(),
                own_aabb.min_z().min(mount_aabb.min_z()),
                own_aabb.max_x().max(mount_aabb.max_x()),
                own_aabb.max_y(),
                own_aabb.max_z().max(mount_aabb.max_z()),
            )
        } else {
            own_aabb
        };

        base.inflate_xyz(horizontal_expansion, 0.0, horizontal_expansion)
    }

    fn set_aggressive(&self, aggressive: bool) {
        self.set_mob_flag(MOB_FLAG_AGGRESSIVE, aggressive);
    }

    fn set_mob_flag(&self, flag: i8, enabled: bool) {
        let flags = self.mob_flags();
        let next = if enabled { flags | flag } else { flags & !flag };
        self.set_mob_flags(next);
    }

    fn controlled_mob_vehicle(&self) -> Option<SharedEntity> {
        let vehicle = self.vehicle()?;
        if vehicle
            .controlling_passenger()
            .is_none_or(|passenger| passenger.id() != self.id())
        {
            return None;
        }
        vehicle.as_mob()?;
        Some(vehicle)
    }

    fn set_wanted_position(&self, position: DVec3, speed_modifier: f64) {
        self.default_set_wanted_position(position, speed_modifier);
    }

    /// The body of [`Self::set_wanted_position`], callable from an override.
    ///
    /// Rust has no `super`, so a mob whose move control overrides
    /// `setWantedPosition` -- the rabbit, which forces a swim speed -- calls
    /// this for the rest.
    fn default_set_wanted_position(&self, position: DVec3, speed_modifier: f64) {
        if let Some(vehicle) = self.controlled_mob_vehicle()
            && let Some(mob) = vehicle.as_mob()
        {
            mob.set_wanted_position(position, speed_modifier);
            return;
        }

        self.mob_base()
            .controls()
            .lock()
            .move_control
            .set_wanted_position(position, speed_modifier);
    }

    fn jump_control_jump(&self) {
        self.mob_base().controls().lock().jump_control.jump();
    }

    /// Mirrors vanilla `Mob.setSpeed`: update cached speed and forward AI input.
    fn set_mob_speed(&self, speed: f32) {
        self.set_speed(speed);
        let input = self.travel_input();
        self.set_travel_input(LivingTravelInput::new(
            input.sideways(),
            input.vertical(),
            speed,
        ));
    }

    /// Stops the mob where it stands, without dropping it out of the air.
    ///
    /// Vanilla parity: `LivingEntity.stopInPlace`. Vanilla keeps it on
    /// `LivingEntity`; here it needs the navigation, which only a `Mob` has.
    fn stop_in_place(&self) {
        self.mob_base().navigation().lock().stop();
        self.set_travel_input(LivingTravelInput::ZERO);
        self.set_mob_speed(0.0);
        let velocity = self.velocity();
        self.set_velocity(DVec3::new(0.0, velocity.y, 0.0));
    }

    /// How far around itself a mob reaches for dropped items.
    ///
    /// Vanilla parity: `Mob.getPickupReach`, whose `ITEM_PICKUP_REACH` is one
    /// block sideways and none vertically.
    fn pickup_reach(&self) -> DVec3 {
        DVec3::new(1.0, 0.0, 1.0)
    }

    /// Whether this mob would rather shoot `item_stack` than swing it.
    ///
    /// Vanilla parity: `Mob.canUseNonMeleeWeapon`, which is false for every mob
    /// but the three that carry a bow or a crossbow. It is what stops a piglin
    /// punching with a loaded crossbow in its hand, and what lets it hang back
    /// and fire instead.
    fn can_use_non_melee_weapon(&self, _item_stack: &ItemStack) -> bool {
        false
    }

    /// Sets whether this mob is a baby.
    ///
    /// Vanilla parity: `Mob.setBaby`, which is a no-op on `Mob` itself and is
    /// overridden by `AgeableMob` and by the four monsters that carry a baby
    /// flag of their own. The default here is the `AgeableMob` override,
    /// reached through [`crate::entity::Entity::as_ageable_mob`], so an ageable
    /// mob needs no override at all and a non-ageable one that has no baby form
    /// correctly does nothing.
    fn set_baby(&self, baby: bool) {
        let Some(ageable) = self.as_ageable_mob() else {
            return;
        };
        // Vanilla parity: the `if (this.canBeABaby())` of the `final
        // AgeableMob.setBaby`, which is what makes this a no-op on a camel husk.
        if !ageable.can_be_a_baby() {
            return;
        }
        ageable.set_age(if baby {
            ageable.get_baby_start_age()
        } else {
            0
        });
    }

    /// Returns vanilla `Mob.canHoldItem`.
    fn can_hold_item(&self, _item_stack: &ItemStack) -> bool {
        true
    }

    /// Whether this mob is allowed to start a hunt.
    ///
    /// Vanilla parity: `AbstractPiglin.canHunt`, which only the two piglins
    /// answer -- a piglin by its `cannotHunt` flag and a brute always with no.
    fn can_hunt(&self) -> bool {
        false
    }

    /// The weapon tag this mob would rather carry than any other.
    ///
    /// Vanilla parity: `Mob.getPreferredWeaponType`, which is null for every
    /// mob but the piglin.
    fn preferred_weapon_type(&self) -> Option<&'static foton_utils::Identifier> {
        None
    }

    /// Puts `item_stack` in `slot` and guarantees it drops on death.
    ///
    /// Vanilla parity: `Mob.setItemSlotAndDropWhenKilled`.
    fn set_item_slot_and_drop_when_killed(&self, slot: EquipmentSlot, item_stack: ItemStack) {
        self.set_item_slot(slot, item_stack);
        Mob::set_guaranteed_drop(self, slot);
    }

    /// Returns whether this mob would swap `current_item_stack` for `new_item_stack`.
    ///
    /// Vanilla parity: `Mob.canReplaceCurrentItem(ItemStack, ItemStack, EquipmentSlot)`.
    /// A mob that overrides this calls [`Self::mob_can_replace_current_item`]
    /// for the base body, which is where vanilla writes `super`.
    fn can_replace_current_item(
        &self,
        new_item_stack: &ItemStack,
        current_item_stack: &ItemStack,
        slot: EquipmentSlot,
    ) -> bool {
        self.mob_can_replace_current_item(new_item_stack, current_item_stack, slot)
    }

    /// The shared part of vanilla `Mob.canReplaceCurrentItem`.
    fn mob_can_replace_current_item(
        &self,
        new_item_stack: &ItemStack,
        current_item_stack: &ItemStack,
        slot: EquipmentSlot,
    ) -> bool {
        if current_item_stack.is_empty() {
            return true;
        }
        if slot.is_armor() {
            return self.compare_armor(new_item_stack, current_item_stack, slot);
        }
        slot == EquipmentSlot::MainHand
            && self.compare_weapons(new_item_stack, current_item_stack, slot)
    }

    /// Vanilla parity: the private `Mob.compareArmor`.
    #[expect(
        clippy::float_cmp,
        reason = "vanilla compares the two attribute values exactly; an epsilon here \
                  would change which armor a mob swaps for"
    )]
    fn compare_armor(
        &self,
        new_item_stack: &ItemStack,
        current_item_stack: &ItemStack,
        slot: EquipmentSlot,
    ) -> bool {
        if current_item_stack.has_enchantment_effect(EnchantmentEffectComponent::PreventArmorChange)
        {
            return false;
        }

        let new_defense =
            self.approximate_attribute_with(new_item_stack, vanilla_attributes::ARMOR, slot);
        let old_defense =
            self.approximate_attribute_with(current_item_stack, vanilla_attributes::ARMOR, slot);
        if new_defense != old_defense {
            return new_defense > old_defense;
        }

        let new_toughness = self.approximate_attribute_with(
            new_item_stack,
            vanilla_attributes::ARMOR_TOUGHNESS,
            slot,
        );
        let old_toughness = self.approximate_attribute_with(
            current_item_stack,
            vanilla_attributes::ARMOR_TOUGHNESS,
            slot,
        );
        if new_toughness != old_toughness {
            return new_toughness > old_toughness;
        }
        self.can_replace_equal_item(new_item_stack, current_item_stack)
    }

    /// Vanilla parity: the private `Mob.compareWeapons`.
    #[expect(
        clippy::float_cmp,
        reason = "vanilla compares the two attack damages exactly; an epsilon here \
                  would change which weapon a mob swaps for"
    )]
    fn compare_weapons(
        &self,
        new_item_stack: &ItemStack,
        current_item_stack: &ItemStack,
        slot: EquipmentSlot,
    ) -> bool {
        if let Some(preferred) = self.preferred_weapon_type() {
            let current_preferred = REGISTRY
                .items
                .is_in_tag(current_item_stack.item(), preferred);
            let new_preferred = REGISTRY.items.is_in_tag(new_item_stack.item(), preferred);
            if current_preferred && !new_preferred {
                return false;
            }
            if !current_preferred && new_preferred {
                return true;
            }
        }

        let new_damage = self.approximate_attribute_with(
            new_item_stack,
            vanilla_attributes::ATTACK_DAMAGE,
            slot,
        );
        let old_damage = self.approximate_attribute_with(
            current_item_stack,
            vanilla_attributes::ATTACK_DAMAGE,
            slot,
        );
        if new_damage != old_damage {
            return new_damage > old_damage;
        }
        self.can_replace_equal_item(new_item_stack, current_item_stack)
    }

    /// Breaks a tie between two equally good items.
    ///
    /// Vanilla parity: `Mob.canReplaceEqualItem` -- more enchantments wins,
    /// then less damage, then a named item over an unnamed one.
    fn can_replace_equal_item(
        &self,
        new_item_stack: &ItemStack,
        current_item_stack: &ItemStack,
    ) -> bool {
        use foton_registry::data_components::vanilla_components::{
            CUSTOM_NAME, ENCHANTMENTS, ItemEnchantments,
        };

        let new_enchantments = new_item_stack
            .get(ENCHANTMENTS)
            .map_or(0, ItemEnchantments::len);
        let current_enchantments = current_item_stack
            .get(ENCHANTMENTS)
            .map_or(0, ItemEnchantments::len);
        if new_enchantments != current_enchantments {
            return new_enchantments > current_enchantments;
        }

        let new_damage = new_item_stack.get_damage_value();
        let current_damage = current_item_stack.get_damage_value();
        if new_damage != current_damage {
            return new_damage < current_damage;
        }
        new_item_stack.get(CUSTOM_NAME).is_some() && current_item_stack.get(CUSTOM_NAME).is_none()
    }

    /// Wears or holds `item_stack` when it beats what is already there.
    ///
    /// Vanilla parity: `Mob.equipItemIfPossible`. Returns what was actually
    /// equipped, so an empty stack means the mob turned it down. Armor the mob
    /// will not swap falls back to the main hand, which is how a piglin ends up
    /// carrying a helmet it is not wearing. Vanilla also takes the level, which
    /// Foton reads off the entity instead.
    fn equip_item_if_possible(&self, item_stack: &ItemStack) -> ItemStack {
        let mut slot = self.equipment_slot_for_item(item_stack);
        if !self.is_equippable_in_slot(item_stack, slot) {
            return ItemStack::empty();
        }

        let mut current = self.get_item_by_slot(slot);
        let mut can_replace = self.can_replace_current_item(item_stack, &current, slot);
        if slot.is_armor() && !can_replace {
            slot = EquipmentSlot::MainHand;
            current = self.get_item_by_slot(slot);
            can_replace = current.is_empty();
        }

        if !can_replace || !self.can_hold_item(item_stack) {
            return ItemStack::empty();
        }

        let drop_chance = self.equipment_drop_chance(slot);
        if !current.is_empty() && (rand::random::<f32>() - 0.1).max(0.0) < drop_chance {
            self.spawn_at_location(current, 0.0);
        }

        let to_equip = item_stack.copy_with_count(slot.limit(item_stack.count()));
        self.set_item_slot_and_drop_when_killed(slot, to_equip.clone());
        Mob::set_persistence_required(self);
        to_equip
    }

    /// Returns vanilla `Mob.wantsToPickUp`.
    fn wants_to_pick_up(&self, world: &World, item_stack: &ItemStack) -> bool {
        self.mob_wants_to_pick_up(world, item_stack)
    }

    /// The shared part of vanilla `Mob.wantsToPickUp`.
    fn mob_wants_to_pick_up(&self, world: &World, item_stack: &ItemStack) -> bool {
        world.get_game_rule(&MOB_GRIEFING)
            && Mob::can_pick_up_loot(self)
            && self.can_hold_item(item_stack)
    }

    /// Takes one dropped item off the ground.
    ///
    /// Vanilla parity: `Mob.pickUpItem`.
    fn pick_up_item(&self, world: &Arc<World>, item_entity: &SharedEntity) {
        self.default_pick_up_item(world, item_entity);
    }

    /// The body of [`Self::pick_up_item`], callable from an override.
    ///
    /// Vanilla parity: `Raider.pickUpItem`, which is what promotes whichever
    /// raider reaches the fallen captain's banner first, and otherwise falls
    /// through to `Mob.pickUpItem`. It sits on the shared body because Foton's
    /// raiders would otherwise repeat the same override six times.
    fn default_pick_up_item(&self, world: &Arc<World>, item_entity: &SharedEntity) {
        if let Some(raider) = self.as_raider()
            && raider::pick_up_banner(raider, world, item_entity)
        {
            return;
        }
        self.mob_pick_up_item(world, item_entity);
    }

    /// Equips a dropped stack, or as much of it as fits.
    ///
    /// Vanilla parity: the body of `Mob.pickUpItem`. This is what makes a
    /// zombie wear the helmet you dropped and swing the sword that came with
    /// it. Overrides that want the shared behavior call this rather than
    /// [`Self::pick_up_item`], which would come straight back to them.
    fn mob_pick_up_item(&self, world: &Arc<World>, item_entity: &SharedEntity) {
        let Some(item) = item_entity.downcast_ref::<ItemEntity>() else {
            return;
        };
        let mut stack = item.get_item();
        let equipped = self.equip_item_if_possible(&stack);
        if equipped.is_empty() {
            return;
        }

        // Vanilla parity: `this.take(entity, equippedWithStack.getCount())`,
        // the pickup animation every client draws.
        let taken = equipped.count();
        world.broadcast_to_nearby(
            ChunkPos::from_entity_pos(item_entity.position()),
            CTakeItemEntity::new(item_entity.id(), self.id(), taken),
            None,
        );

        stack.shrink(taken);
        if stack.is_empty() {
            item_entity.set_removed(RemovalReason::Discarded);
        } else {
            item.set_item(stack);
        }
    }

    /// Runs the item-pickup half of vanilla `Mob.aiStep`.
    fn pick_up_nearby_items(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if !Mob::can_pick_up_loot(self)
            || !Entity::is_alive(self)
            || !world.get_game_rule(&MOB_GRIEFING)
        {
            return;
        }

        let reach = self.pickup_reach();
        let search = self.bounding_box().inflate_xyz(reach.x, reach.y, reach.z);
        let items = world.get_entities_in_aabb_matching(&search, |entity| {
            let Some(item_entity) = entity.downcast_ref::<ItemEntity>() else {
                return false;
            };
            !entity.is_removed()
                && !item_entity.get_item().is_empty()
                && !item_entity.has_pickup_delay()
                && self.wants_to_pick_up(world.as_ref(), &item_entity.get_item())
        });

        for item in items {
            let mut event = crate::event::EntityPickupItemEvent::new(self.uuid(), item.uuid());
            world.fire_event(&mut event);
            if !event.is_cancelled() {
                self.pick_up_item(&world, &item);
            }
        }
    }

    /// Advances the idle clock by one tick.
    ///
    /// Vanilla parity: `Mob.updateNoActionTime`. A raider counts double, which
    /// is what makes it stop being recruitable after two minutes of standing
    /// around rather than four.
    fn update_no_action_time(&self) {
        self.increment_no_action_time();
    }

    fn mob_server_ai_step(&self) {
        self.update_no_action_time();
        // Vanilla runs this from `Raider.aiStep`, an override of
        // `LivingEntity.aiStep`. Foton's mobs reach their server-side tick
        // through this method instead, so the raid half of a raider's tick
        // lives here rather than in six identical overrides.
        if let Some(raider) = self.as_raider() {
            raider::ai_step_raider(raider);
        }
        self.pick_up_nearby_items();
        self.mob_base().sensing().lock().tick();
        if self.tick_count() % 5 == 0 {
            self.update_control_flags();
        }
        self.tick_goal_selectors();
        self.tick_path_navigation();
        self.custom_server_ai_step();
        self.tick_move_control();
        self.tick_look_control();
        self.tick_jump_control();
    }

    fn tick_path_navigation(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let game_time = world.game_time();
        self.mob_base().navigation().lock().tick();
        tick_path_navigation_target(self, &world, game_time, true);
    }

    /// Override this to gate the move control, and call
    /// [`Self::default_tick_move_control`] from the override; Rust has no
    /// `super`, so the base body lives in its own method.
    fn tick_move_control(&self) {
        self.default_tick_move_control();
    }

    /// The body of [`Self::tick_move_control`], callable from an override.
    ///
    /// Rust has no `super`, so a mob whose move control only prefixes the base
    /// tick -- the rabbit, which picks its jump speed first -- calls this for
    /// the rest.
    /// The shared part of vanilla `MoveControl.tick`.
    fn default_tick_move_control(&self) {
        let move_control = {
            let mut controls = self.mob_base().controls().lock();
            let move_control = controls.move_control;
            if matches!(move_control.operation(), MoveControlOperation::MoveTo) {
                controls.move_control.set_wait();
            }
            move_control
        };

        match move_control.operation() {
            MoveControlOperation::Wait => {
                if let MoveControlKind::Flying {
                    hovers_in_place, ..
                } = self.move_control_kind()
                {
                    // Vanilla parity: the else branch of `FlyingMoveControl.tick`,
                    // which drops a hovering flier back onto gravity.
                    if !hovers_in_place {
                        self.set_no_gravity(false);
                    }
                    self.set_travel_input(LivingTravelInput::new(0.0, 0.0, 0.0));
                    return;
                }

                let input = self.travel_input();
                self.set_travel_input(LivingTravelInput::new(
                    input.sideways(),
                    input.vertical(),
                    0.0,
                ));
            }
            MoveControlOperation::MoveTo => match self.move_control_kind() {
                MoveControlKind::Ground => self.tick_move_to_control(
                    move_control.wanted_position(),
                    move_control.speed_modifier(),
                ),
                MoveControlKind::Flying { max_turn, .. } => self.tick_flying_move_to_control(
                    move_control.wanted_position(),
                    move_control.speed_modifier(),
                    max_turn,
                ),
            },
            MoveControlOperation::Strafe => {
                self.tick_strafe_control(
                    move_control.strafe_forward(),
                    move_control.strafe_right(),
                );
            }
            MoveControlOperation::Jumping => {
                self.tick_jumping_control(move_control.speed_modifier());
            }
        }
    }

    /// Returns which move control this mob installs.
    ///
    /// Vanilla parity: the `MoveControl` subclass a mob's constructor assigns.
    fn move_control_kind(&self) -> MoveControlKind {
        MoveControlKind::Ground
    }

    /// Steers a flier toward its wanted position.
    ///
    /// Vanilla parity: `FlyingMoveControl.tick`. The gravity flag is the part
    /// that matters: a flier only stays up because the move control turns
    /// gravity off for as long as it has somewhere to be.
    fn tick_flying_move_to_control(
        &self,
        wanted_position: DVec3,
        speed_modifier: f64,
        max_turn: f32,
    ) {
        self.set_no_gravity(true);

        let position = self.position();
        let xd = wanted_position.x - position.x;
        let yd = wanted_position.y - position.y;
        let zd = wanted_position.z - position.z;
        if xd.mul_add(xd, yd.mul_add(yd, zd * zd)) < MOVE_CONTROL_MIN_SPEED_SQR {
            self.set_travel_input(LivingTravelInput::new(0.0, 0.0, 0.0));
            return;
        }

        let y_rot = (zd.atan2(xd) as f32).to_degrees() - 90.0;
        let (yaw, pitch) = self.rotation();
        self.set_rotation((rotlerp(yaw, y_rot, MOVE_CONTROL_MAX_TURN), pitch));

        let attribute = if self.on_ground() {
            vanilla_attributes::MOVEMENT_SPEED
        } else {
            vanilla_attributes::FLYING_SPEED
        };
        let speed = (speed_modifier * self.attributes().lock().required_value(attribute)) as f32;
        self.set_mob_speed(speed);

        let horizontal = xd.hypot(zd);
        if yd.abs() <= f64::from(MOVE_CONTROL_MIN_FLYING_DELTA)
            && horizontal <= f64::from(MOVE_CONTROL_MIN_FLYING_DELTA)
        {
            return;
        }

        let x_rot = -(yd.atan2(horizontal) as f32).to_degrees();
        let (yaw, pitch) = self.rotation();
        self.set_rotation((yaw, rotlerp(pitch, x_rot, max_turn)));
        let vertical = if yd > 0.0 { speed } else { -speed };
        let input = self.travel_input();
        self.set_travel_input(LivingTravelInput::new(
            input.sideways(),
            vertical,
            input.forward(),
        ));
    }

    fn tick_move_to_control(&self, wanted_position: DVec3, speed_modifier: f64) {
        let position = self.position();
        let xd = wanted_position.x - position.x;
        let yd = wanted_position.y - position.y;
        let zd = wanted_position.z - position.z;
        let dd = xd * xd + yd * yd + zd * zd;
        if dd < MOVE_CONTROL_MIN_SPEED_SQR {
            let input = self.travel_input();
            self.set_travel_input(LivingTravelInput::new(
                input.sideways(),
                input.vertical(),
                0.0,
            ));
            return;
        }

        let y_rot = (zd.atan2(xd) as f32 * 180.0 / PI) - 90.0;
        let (_, pitch) = self.rotation();
        self.set_rotation((
            rotlerp(self.rotation().0, y_rot, MOVE_CONTROL_MAX_TURN),
            pitch,
        ));
        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        self.set_mob_speed((speed_modifier * movement_speed) as f32);

        if should_jump_to_wanted_position(self, xd, yd, zd) {
            self.jump_control_jump();
            self.mob_base().controls().lock().move_control.set_jumping();
        }
    }

    fn tick_strafe_control(&self, forward: f32, right: f32) {
        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED) as f32;
        let speed = movement_speed * 0.25;
        let mut strafe_forward = forward;
        let mut strafe_right = right;

        let mut distance = strafe_forward
            .mul_add(strafe_forward, strafe_right * strafe_right)
            .sqrt();
        if distance < 1.0 {
            distance = 1.0;
        }
        distance = speed / distance;
        let xa = strafe_forward * distance;
        let za = strafe_right * distance;
        let yaw_radians = self.rotation().0 * PI / 180.0;
        let sin = yaw_radians.sin();
        let cos = yaw_radians.cos();
        let dx = xa.mul_add(cos, -(za * sin));
        let dz = za.mul_add(cos, xa * sin);
        if !self.is_strafe_walkable(dx, dz) {
            strafe_forward = 1.0;
            strafe_right = 0.0;
        }

        self.set_speed(speed);
        self.set_travel_input(LivingTravelInput::new(strafe_right, 0.0, strafe_forward));
        self.mob_base().controls().lock().move_control.set_wait();
    }

    fn is_strafe_walkable(&self, dx: f32, dz: f32) -> bool {
        let Some(world) = self.level() else {
            return true;
        };
        let position = self.position();
        let pos = BlockPos::new(
            fast_floor(position.x + f64::from(dx)),
            fast_floor(position.y),
            fast_floor(position.z + f64::from(dz)),
        );
        let mut context = PathfindingContext::new(world.as_ref(), self.block_position());
        WalkPathEvaluator::path_type_static(&mut context, pos) == PathType::Walkable
    }

    fn tick_jumping_control(&self, speed_modifier: f64) {
        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        self.set_mob_speed((speed_modifier * movement_speed) as f32);
        if self.on_ground()
            || (self.is_in_water() || self.is_in_lava()) && self.is_affected_by_fluids()
        {
            self.mob_base().controls().lock().move_control.set_wait();
        }
    }

    /// Override this to gate the look control, and call
    /// [`Self::default_tick_look_control`] from the override.
    fn tick_look_control(&self) {
        self.default_tick_look_control();
    }

    /// Returns whether an idle look control levels the mob's pitch out.
    ///
    /// Vanilla parity: `LookControl.resetXRotOnTick`, which only the fox
    /// overrides -- to keep a crouching or pouncing nose down.
    fn look_control_resets_pitch(&self) -> bool {
        true
    }

    /// The shared part of vanilla `LookControl.tick`.
    fn default_tick_look_control(&self) {
        let look_control = {
            let mut controls = self.mob_base().controls().lock();
            let look_control = controls.look_control;
            controls.look_control.tick_cooldown();
            look_control
        };

        let mut rotation = self.rotation();
        if self.look_control_resets_pitch() {
            rotation.1 = 0.0;
        }
        if look_control.is_looking_at_target() {
            let position = self.position();
            let wanted_position = look_control.wanted_position();
            let xd = wanted_position.x - position.x;
            let yd = wanted_position.y - self.get_eye_y();
            let zd = wanted_position.z - position.z;
            let horizontal = xd.hypot(zd);
            if horizontal.abs() > 1.0e-5 || yd.abs() > 1.0e-5 {
                let target_pitch = -(yd.atan2(horizontal)) as f32 * 180.0 / PI;
                rotation.1 =
                    rotate_towards(rotation.1, target_pitch, look_control.x_max_rot_angle());
            }
            if zd.abs() > 1.0e-5 || xd.abs() > 1.0e-5 {
                let target_yaw = (zd.atan2(xd) as f32 * 180.0 / PI) - 90.0;
                self.set_y_head_rot(rotate_towards(
                    self.y_head_rot(),
                    target_yaw,
                    look_control.y_max_rot_speed(),
                ));
            }
        } else {
            self.set_y_head_rot(rotate_towards(self.y_head_rot(), self.y_body_rot(), 10.0));
        }

        self.set_rotation(rotation);
        self.clamp_head_rotation_to_body_when_pathing();
    }

    fn clamp_head_rotation_to_body_when_pathing(&self) {
        if self.mob_base().navigation().lock().is_done() {
            return;
        }

        self.set_y_head_rot(rotate_if_necessary(
            self.y_head_rot(),
            self.y_body_rot(),
            self.max_head_y_rot(),
        ));
    }

    fn tick_jump_control(&self) {
        self.default_tick_jump_control();
    }

    /// The body of [`Self::tick_jump_control`], callable from an override.
    ///
    /// Rust has no `super`, so a mob that only adds a condition calls this for
    /// the rest.
    fn default_tick_jump_control(&self) {
        let jumping = self.mob_base().controls().lock().jump_control.tick();
        self.set_jumping(jumping);
    }

    fn update_control_flags(&self) {
        let no_controller = self
            .controlling_passenger()
            .is_none_or(|passenger| !passenger.is_mob());
        let not_in_boat = self
            .vehicle()
            .is_none_or(|vehicle| !vehicle.entity_type().is_abstract_boat);

        let mut selector = self.mob_base().goal_selector().lock();
        selector.set_control(GoalControl::Move, no_controller);
        selector.set_control(GoalControl::Jump, no_controller && not_in_boat);
        selector.set_control(GoalControl::Look, no_controller);
    }

    /// Override this to change how the head follows the body, and call
    /// [`Self::default_tick_body_rotation_control`] from the override; Rust has
    /// no `super`, so the base body lives in its own method.
    fn tick_body_rotation_control(&self) {
        self.default_tick_body_rotation_control();
    }

    /// The body of [`Self::tick_body_rotation_control`], callable from an
    /// override.
    ///
    /// Vanilla parity: `BodyRotationControl.clientTick`, which despite its name
    /// is what `Mob.tickHeadTurn` runs on the server too.
    fn default_tick_body_rotation_control(&self) {
        let moving = {
            let delta = self.position() - self.old_position();
            delta.x.mul_add(delta.x, delta.z * delta.z) > BODY_ROTATION_MOVING_DISTANCE_SQR
        };
        let carrying_mob_passenger = self
            .first_passenger()
            .is_some_and(|passenger| passenger.is_mob());
        let input = BodyRotationInput::new(
            moving,
            carrying_mob_passenger,
            self.rotation().0,
            self.y_body_rot(),
            self.y_head_rot(),
            self.max_head_y_rot(),
        );
        let update = self
            .mob_base()
            .controls()
            .lock()
            .body_rotation_control
            .tick(input);
        self.set_y_body_rot(update.y_body_rot());
        self.set_y_head_rot(update.y_head_rot());
    }
}

fn can_attempt_equipment_drop(drop_chance: f32, preserve: bool, killed_by_player: bool) -> bool {
    drop_chance != 0.0 && (killed_by_player || preserve)
}

fn default_attack_reach() -> f64 {
    f64::from(DEFAULT_ATTACK_REACH_BASE).sqrt() - f64::from(DEFAULT_ATTACK_REACH_OFFSET)
}

fn should_jump_to_wanted_position<M: Mob + ?Sized>(mob: &M, xd: f64, yd: f64, zd: f64) -> bool {
    let max_up_step = f64::from(mob.max_up_step());
    if yd > max_up_step && xd * xd + zd * zd < mob.bounding_box().width().max(1.0) {
        return true;
    }

    let Some(world) = mob.level() else {
        return false;
    };
    let pos = mob.block_position();
    let block_state = world.get_block_state(pos);
    let behavior = BLOCK_BEHAVIORS.get_behavior(block_state.get_block());
    let shape = behavior.get_collision_shape(
        block_state,
        world.as_ref(),
        pos,
        BlockCollisionContext::empty(),
    );
    let shape_top = position_shape_top(pos, shape.max(Axis::Y));
    let block = block_state.get_block();
    !shape.is_empty()
        && mob.position().y < shape_top
        && !block.has_tag(&BlockTag::DOORS)
        && !block.has_tag(&BlockTag::FENCES)
}

fn position_shape_top(pos: BlockPos, local_y: f64) -> f64 {
    f64::from(pos.y()) + local_y
}

fn block_pos_distance_sqr(a: BlockPos, b: BlockPos) -> f64 {
    let dx = f64::from(a.x() - b.x());
    let dy = f64::from(a.y() - b.y());
    let dz = f64::from(a.z() - b.z());
    dx.mul_add(dx, dy.mul_add(dy, dz * dz))
}

fn block_center_distance_sqr(pos: BlockPos, target: DVec3) -> f64 {
    let (x, y, z) = pos.get_center();
    DVec3::new(x, y, z).distance_squared(target)
}

fn home_radius_sqr(radius: i32) -> f64 {
    let radius = f64::from(radius);
    radius * radius
}

pub(crate) fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let mut diff = wrap_degrees(b - a);
    if diff > max {
        diff = max;
    }
    if diff < -max {
        diff = -max;
    }

    let mut result = a + diff;
    if result < 0.0 {
        result += 360.0;
    } else if result > 360.0 {
        result -= 360.0;
    }
    result
}

/// Wraps an angle into the quarter turn nearest zero.
///
/// Vanilla parity: `Mth.wrapDegrees90`, which the happy ghast uses to square
/// itself up with the world while it holds still for its riders.
pub(crate) fn wrap_degrees_90(angle: f32) -> f32 {
    let normalized = angle % 90.0;
    if normalized >= 45.0 {
        return normalized - 90.0;
    }
    if normalized < -45.0 {
        return normalized + 90.0;
    }
    normalized
}

pub(crate) fn wrap_degrees(mut degrees: f32) -> f32 {
    degrees %= 360.0;
    if degrees >= 180.0 {
        degrees -= 360.0;
    }
    if degrees < -180.0 {
        degrees += 360.0;
    }
    degrees
}

#[cfg(test)]
mod tests;
