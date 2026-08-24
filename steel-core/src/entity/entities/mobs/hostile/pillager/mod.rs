//! Pillager entity.
//!
//! Vanilla parity: `Pillager`. The illager that shoots. Everything about it is
//! the crossbow: it stops to wind, walks at half speed while loaded, and holds
//! the shot for a second after the click. It also carries a five-slot bag it
//! only ever uses during a raid, to pick the white banners it needs off the
//! ground.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::PillagerEntityData;
use steel_registry::{sound_events, vanilla_entities, vanilla_items};
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::chunk::light::LightLayer;
use crate::entity::abstract_illager::{AbstractIllager, IllagerArmPose};
use crate::entity::ai::goal::{
    FloatGoal, HoldGroundAttackGoal, HurtByTargetGoal, LongDistancePatrolGoal, LookAtPlayerGoal,
    NearestAttackableTargetGoal, RaiderCelebrationGoal, RandomStrollGoal, RangedCrossbowAttackGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ArrowEntity;
use crate::entity::patrolling_monster::{
    PatrolState, PatrollingMonster, read_patrol_state, write_patrol_state,
};
use crate::entity::raider::{
    Raider, RaiderState, finalize_spawn_raider, read_raider_state, write_raider_state,
};
use crate::entity::spawn_rules::check_any_light_monster_spawn_rules;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::equipment::EquipmentSlot;
use crate::world::World;

/// Slots in a pillager's bag.
///
/// Vanilla parity: `Pillager.INVENTORY_SIZE`.
const INVENTORY_SIZE: usize = 5;

/// NBT key vanilla stores the bag under.
///
/// Vanilla parity: `InventoryCarrier.TAG_INVENTORY`.
const TAG_INVENTORY: &str = "Inventory";

/// Speed a pillager closes the distance at.
///
/// Vanilla parity: the `1.0` of `RangedCrossbowAttackGoal`.
const CROSSBOW_APPROACH_SPEED: f64 = 1.0;

/// Range within which a pillager stops walking and shoots.
///
/// Vanilla parity: the `8.0F` attack radius of the same goal.
const CROSSBOW_ATTACK_RADIUS: f32 = 8.0;

/// Distance at which a patrol stops shouting and charges.
///
/// Vanilla parity: the `10.0F` of `Raider.HoldGroundAttackGoal`.
const HOLD_GROUND_RADIUS: f32 = 10.0;

/// Speed a pillager wanders at.
const STROLL_SPEED_MODIFIER: f64 = 0.6;

/// Distance at which a pillager watches a player.
///
/// Vanilla parity: the `15.0F` of both `LookAtPlayerGoal` entries.
const LOOK_AT_RANGE: f64 = 15.0;

/// How often a pillager bothers to watch a player.
///
/// Vanilla parity: the `1.0F` probability of the player entry -- a pillager
/// always turns to face you, which is most of why it feels aware of you.
const LOOK_AT_PLAYER_PROBABILITY: f32 = 1.0;

/// How often a pillager bothers to watch another mob.
///
/// Vanilla parity: the `DEFAULT_PROBABILITY` of `LookAtPlayerGoal`.
const LOOK_AT_MOB_PROBABILITY: f32 = 0.02;

/// Speed a follower patrols at.
///
/// Vanilla parity: the `0.7` of `PatrollingMonster.registerGoals`.
const PATROL_SPEED: f64 = 0.7;

/// Speed the captain patrols at, so the group keeps up.
const PATROL_LEADER_SPEED: f64 = 0.595;

/// Speed the bolt leaves the crossbow at.
///
/// Vanilla parity: the `1.6F` of `Pillager.performRangedAttack`.
const BOLT_POWER: f32 = 1.6;

/// Spread of a mob-fired bolt before difficulty is applied.
///
/// Vanilla parity: the `14 - difficulty * 4` of `performCrossbowAttack`.
const BOLT_UNCERTAINTY_BASE: f32 = 14.0;

/// How much each difficulty step tightens the spread.
const BOLT_UNCERTAINTY_PER_DIFFICULTY: f32 = 4.0;

/// Fraction of a target's height a bolt is aimed at.
///
/// Vanilla parity: the `getY(0.3333333333333333)` of `shootProjectile`.
const AIM_HEIGHT_FRACTION: f64 = 1.0 / 3.0;

/// Chance a naturally spawned pillager's crossbow is enchanted.
///
/// Vanilla parity: the `random.nextInt(300) == 0` of `enchantSpawnedWeapon`.
const SPAWN_ENCHANT_DENOMINATOR: i32 = 300;

/// Brightest block light a patroller will appear in.
///
/// Vanilla parity: the `getBrightness(LightLayer.BLOCK, pos) > 8` of
/// `PatrollingMonster.checkPatrollingMonsterSpawnRules`. It is block light
/// alone, not the combined brightness a monster's dark check uses, so a patrol
/// may appear in broad daylight but not next to a torch.
const MAX_PATROL_SPAWN_BLOCK_LIGHT: u8 = 8;

/// A pillager.
#[entity_behavior(class = "Pillager")]
pub struct PillagerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<PillagerEntityData>,
    patrol_state: PatrolState,
    raider_state: RaiderState,
    /// The five slots a pillager carries banners in during a raid.
    ///
    /// Vanilla parity: `Pillager.inventory`.
    inventory: SyncMutex<SimpleContainer>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `PillagerEntity`.
unsafe impl DowncastType for PillagerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/pillager");
}

impl PillagerEntity {
    /// Creates a pillager at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a pillager from saved base data.
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
        let mut entity_data = PillagerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Pillager.registerGoals`, over
            // the ones `PatrollingMonster` and `Raider` add.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(2, HoldGroundAttackGoal::new(HOLD_GROUND_RADIUS));
            goals.add_goal(
                3,
                RangedCrossbowAttackGoal::new(
                    CROSSBOW_APPROACH_SPEED,
                    CROSSBOW_ATTACK_RADIUS,
                    set_charging_crossbow,
                    shoot_crossbow,
                ),
            );
            goals.add_goal(
                4,
                LongDistancePatrolGoal::new(PATROL_SPEED, PATROL_LEADER_SPEED),
            );
            goals.add_goal(5, RaiderCelebrationGoal::new());
            goals.add_goal(8, RandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(
                9,
                LookAtPlayerGoal::new_with_probability(LOOK_AT_RANGE, LOOK_AT_PLAYER_PROBABILITY),
            );
            goals.add_goal(
                10,
                LookAtPlayerGoal::new_for_living_entities(
                    LOOK_AT_RANGE,
                    LOOK_AT_MOB_PROBABILITY,
                    |_, target, _| target.as_mob().is_some(),
                ),
            );
            // Vanilla also flees a creaking at priority 1, and adds three
            // raid-only goals -- fetch the leader's banner, path to the raid,
            // walk the village. Steel has no creaking and no raid.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(
                1,
                HurtByTargetGoal::new()
                    .with_ignored_damage_filter(|entity| entity.as_raider().is_some())
                    .set_alert_others([]),
            );
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new(true, |_, target, _| {
                    target.entity_type() == &vanilla_entities::IRON_GOLEM
                }),
            );
            // Vanilla also hunts villagers at priority 3; Steel has none.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            patrol_state: PatrolState::new(),
            raider_state: RaiderState::new(),
            inventory: SyncMutex::new(SimpleContainer::new(INVENTORY_SIZE)),
        }
    }

    /// Returns whether this pillager is winding its crossbow.
    #[must_use]
    pub fn is_charging_crossbow(&self) -> bool {
        *self.entity_data.lock().is_charging_crossbow.get()
    }

    /// Sets whether this pillager is winding its crossbow.
    pub fn set_charging_crossbow(&self, charging: bool) {
        self.entity_data.lock().is_charging_crossbow.set(charging);
    }

    /// Runs the shared body of the two spawn paths.
    ///
    /// Vanilla parity: `Pillager.populateDefaultEquipmentSlots` followed by
    /// `enchantSpawnedWeapon`. Steel has no enchantment providers, so the
    /// one-in-three-hundred enchanted crossbow arrives plain; the roll is kept
    /// so the branch is visible where the provider will go.
    fn populate_default_equipment(&self) {
        self.living_base().equipment().lock().set(
            EquipmentSlot::MainHand,
            ItemStack::new(&vanilla_items::CROSSBOW),
        );
        if rand::random_range(0..SPAWN_ENCHANT_DENOMINATOR) == 0 {
            // TODO: apply the `PILLAGER_SPAWN_CROSSBOW` enchantment provider
            // once Steel has enchantment providers.
        }
    }
}

/// Sets the synced flag the crossbow goal winds the model with.
fn set_charging_crossbow(mob: &dyn PathfinderMob, charging: bool) {
    if let Some(pillager) = mob.downcast_ref::<PillagerEntity>() {
        pillager.set_charging_crossbow(charging);
    }
}

/// Fires the loaded crossbow at `target`.
///
/// Vanilla parity: `Pillager.performRangedAttack` -> `performCrossbowAttack` ->
/// `CrossbowItem.performShooting` with a target override. With one projectile
/// and no Multishot the whole chain reduces to a single arrow aimed a third of
/// the way up the target and lobbed by the same `distance * 0.2`
/// [`ArrowEntity::shoot_at`] already applies. The differences from a bow shot
/// are the power, the spread and the sound.
///
/// Steel's `CrossbowItem` loads and fires from a player's inventory, so the
/// bolt is not drawn from the mob's ammunition and Piercing on the weapon is
/// not applied. Both are the same gap Steel's crossbow already documents.
fn shoot_crossbow(mob: &dyn PathfinderMob, target: &SharedEntity) {
    let Some(pillager) = mob.downcast_ref::<PillagerEntity>() else {
        return;
    };
    let Some(world) = mob.level() else {
        return;
    };

    let target_position = target.position();
    let aim_y = f64::from(target.base().dimensions().height)
        .mul_add(AIM_HEIGHT_FRACTION, target_position.y);
    let difficulty = u8::from(world.difficulty());
    let uncertainty =
        BOLT_UNCERTAINTY_PER_DIFFICULTY.mul_add(-f32::from(difficulty), BOLT_UNCERTAINTY_BASE);

    let bolt = ArrowEntity::shoot_at(
        &world,
        pillager,
        DVec3::new(target_position.x, aim_y, target_position.z),
        BOLT_POWER,
        uncertainty,
    );
    drop(bolt);

    world.play_sound_at(
        &sound_events::ITEM_CROSSBOW_SHOOT,
        pillager.sound_source(),
        pillager.position(),
        1.0,
        1.0,
        None,
    );
    // Vanilla parity: `Pillager.onCrossbowAttackPerformed`, which keeps a
    // shooting pillager from counting as idle.
    pillager.set_no_action_time(0);
}

impl Entity for PillagerEntity {
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

    fn is_allied_to(&self, other: &dyn Entity) -> bool {
        self.considers_entity_as_ally_illager(other)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        write_patrol_state(self, nbt);
        write_raider_state(self, nbt);

        let inventory = self.inventory.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for item in inventory.items() {
            if !item.is_empty()
                && let NbtTag::Compound(item_nbt) = item.clone().to_nbt_tag()
            {
                items.push(item_nbt);
            }
        }
        nbt.insert(TAG_INVENTORY, NbtList::Compound(items));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        read_patrol_state(self, nbt);
        read_raider_state(self, nbt);
        // Vanilla parity: `Pillager.readAdditionalSaveData` turns looting back
        // on unconditionally, which is what lets a reloaded pillager keep
        // collecting banners.
        self.set_can_pick_up_loot(true);

        let mut inventory = self.inventory.lock();
        inventory.items_mut().fill(ItemStack::empty());
        let Some(items) = nbt.list(TAG_INVENTORY).and_then(|list| list.compounds()) else {
            return;
        };
        for compound in items {
            let Some(mut item) = ItemStack::from_borrowed_compound(&compound) else {
                continue;
            };
            inventory.add(&mut item);
        }
    }
}

impl LivingEntity for PillagerEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
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
        Some(&sound_events::ENTITY_PILLAGER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PILLAGER_DEATH)
    }
}

impl Mob for PillagerEntity {
    /// Vanilla parity: `Pillager` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `PatrollingMonster.checkPatrollingMonsterSpawnRules`,
    /// which allows any light level except bright block light.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        world.light_value_at(LightLayer::Block, pos) <= MAX_PATROL_SPAWN_BLOCK_LIGHT
            && check_any_light_monster_spawn_rules(world, spawn_reason, pos)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.populate_default_equipment();
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

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PILLAGER_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for PillagerEntity {
    /// Vanilla parity: `Pillager.getWalkTargetValue`, which flattens the whole
    /// world so a pillager walks wherever a path takes it.
    fn get_walk_target_value(&self, _pos: BlockPos) -> f32 {
        0.0
    }
}

impl PatrollingMonster for PillagerEntity {
    fn patrol_state(&self) -> &PatrolState {
        &self.patrol_state
    }

    fn can_join_patrol(&self) -> bool {
        self.can_join_patrol_raider()
    }
}

impl Raider for PillagerEntity {
    fn raider_state(&self) -> &RaiderState {
        &self.raider_state
    }

    /// Vanilla parity: `Pillager.applyRaidBuffs`. Steel has no enchantment
    /// providers, so the wave-scaled crossbow arrives unenchanted; the wave
    /// thresholds vanilla picks the provider from need a live raid to read and
    /// are left to the raid manager.
    fn apply_raid_buffs(&self, _wave: i32, _is_captain: bool) {
        self.living_base().equipment().lock().set(
            EquipmentSlot::MainHand,
            ItemStack::new(&vanilla_items::CROSSBOW),
        );
    }

    fn celebrate_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_PILLAGER_CELEBRATE
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

impl AbstractIllager for PillagerEntity {
    /// Vanilla parity: `Pillager.getArmPose`.
    fn arm_pose(&self) -> IllagerArmPose {
        if self.is_charging_crossbow() {
            return IllagerArmPose::CrossbowCharge;
        }
        if self.is_holding(&mut |item| item.is(&vanilla_items::CROSSBOW)) {
            return IllagerArmPose::CrossbowHold;
        }
        if self.is_aggressive() {
            return IllagerArmPose::Attacking;
        }
        IllagerArmPose::Neutral
    }
}

impl Enemy for PillagerEntity {}

#[cfg(test)]
mod tests;
