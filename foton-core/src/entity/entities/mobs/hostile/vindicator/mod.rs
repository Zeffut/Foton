//! Vindicator entity.
//!
//! Vanilla parity: `Vindicator`. The illager with the axe. Outside a raid it is
//! a plain melee hostile with a patrol; inside one it breaks doors. The one
//! oddity is Johnny: a vindicator renamed with a name tag stops caring what
//! it is attacking and goes after every living thing in range, which is the
//! single most dangerous mob in the game to let loose in a village.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::VindicatorEntityData;
use foton_registry::{sound_events, vanilla_entities, vanilla_items};
use foton_utils::BlockPos;
use foton_utils::locks::SyncMutex;
use foton_utils::text::DisplayResolutor;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use text_components::TextComponent;

use crate::entity::LivingEntitySyncedData;
use crate::entity::abstract_illager::{AbstractIllager, IllagerArmPose};
use crate::entity::ai::goal::{
    FloatGoal, HoldGroundAttackGoal, HurtByTargetGoal, LongDistancePatrolGoal, LookAtPlayerGoal,
    MeleeAttackGoal, NearestAttackableTargetGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal,
    RaiderCelebrationGoal, RaiderMoveThroughVillageGoal, RandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::patrolling_monster::{
    PatrolState, PatrollingMonster, read_patrol_state, write_patrol_state,
};
use crate::entity::raider::{
    Raider, RaiderState, finalize_spawn_raider, read_raider_state, write_raider_state,
};
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::world::World;

mod goals;

use goals::{RaiderOpenDoorGoal, VindicatorBreakDoorGoal, VindicatorJohnnyAttackGoal};

/// NBT key vanilla stores the Johnny flag under.
///
/// Vanilla parity: `Vindicator.TAG_JOHNNY`.
const TAG_JOHNNY: &str = "Johnny";

/// The name that turns a vindicator on everything alive.
const JOHNNY_NAME: &str = "Johnny";

/// Distance at which a patrol stops shouting and charges.
const HOLD_GROUND_RADIUS: f32 = 10.0;

/// Speed a vindicator closes on its target at.
///
/// Vanilla parity: the `1.0` of `MeleeAttackGoal`.
const MELEE_SPEED_MODIFIER: f64 = 1.0;

/// Speed a vindicator wanders at.
const STROLL_SPEED_MODIFIER: f64 = 0.6;

/// Distance at which a vindicator watches a player.
///
/// Vanilla parity: the `3.0F` of the player `LookAtPlayerGoal`.
const LOOK_AT_PLAYER_RANGE: f64 = 3.0;

/// How often a vindicator bothers to watch a player.
const LOOK_AT_PLAYER_PROBABILITY: f32 = 1.0;

/// Distance at which a vindicator watches another mob.
const LOOK_AT_MOB_RANGE: f64 = 8.0;

/// How often a vindicator bothers to watch another mob.
const LOOK_AT_MOB_PROBABILITY: f32 = 0.02;

/// Speed a follower patrols at.
const PATROL_SPEED: f64 = 0.7;

/// Speed the captain patrols at.
const PATROL_LEADER_SPEED: f64 = 0.595;
/// Speed a raider walks the streets of the village it is raiding at.
///
/// Vanilla parity: the `1.05F` of `new RaiderMoveThroughVillageGoal(this, 1.05F, 1)`.
const VILLAGE_WALK_SPEED_MODIFIER: f64 = 1.05;

/// How close to a house counts as having reached it.
///
/// Vanilla parity: the `1` of the same goal.
const VILLAGE_POI_ARRIVAL_DISTANCE: f64 = 1.0;

/// A vindicator.
#[entity_behavior(class = "Vindicator")]
pub struct VindicatorEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<VindicatorEntityData>,
    patrol_state: PatrolState,
    raider_state: RaiderState,
    /// Whether this vindicator has been named Johnny.
    is_johnny: SyncMutex<bool>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `VindicatorEntity`.
unsafe impl DowncastType for VindicatorEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/vindicator");
}

impl VindicatorEntity {
    /// Creates a vindicator at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a vindicator from saved base data.
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
        let mut entity_data = VindicatorEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Vindicator.registerGoals`,
            // over the ones `PatrollingMonster` and `Raider` add.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(2, VindicatorBreakDoorGoal::new());
            goals.add_goal(3, RaiderOpenDoorGoal::new());
            goals.add_goal(4, HoldGroundAttackGoal::new(HOLD_GROUND_RADIUS));
            goals.add_goal(
                4,
                LongDistancePatrolGoal::new(PATROL_SPEED, PATROL_LEADER_SPEED),
            );
            goals.add_goal(5, MeleeAttackGoal::new(MELEE_SPEED_MODIFIER, false));
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
            goals.add_goal(8, RandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(
                9,
                LookAtPlayerGoal::new_with_probability(
                    LOOK_AT_PLAYER_RANGE,
                    LOOK_AT_PLAYER_PROBABILITY,
                ),
            );
            goals.add_goal(
                10,
                LookAtPlayerGoal::new_for_living_entities(
                    LOOK_AT_MOB_RANGE,
                    LOOK_AT_MOB_PROBABILITY,
                    |_, target, _| target.as_mob().is_some(),
                ),
            );
            // Vanilla also flees a creaking at priority 1, and adds three
            // raid-only goals. Foton has no creaking and no raid.
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
            targets.add_goal(4, VindicatorJohnnyAttackGoal::new());
            // Vanilla also hunts villagers at priority 3; Foton has none.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            patrol_state: PatrolState::new(),
            raider_state: RaiderState::new(),
            is_johnny: SyncMutex::new(false),
        }
    }

    /// Returns whether this vindicator answers to Johnny.
    #[must_use]
    pub fn is_johnny(&self) -> bool {
        *self.is_johnny.lock()
    }

    /// Gives this vindicator an iron axe unless a raid is about to arm it.
    ///
    /// Vanilla parity: `Vindicator.populateDefaultEquipmentSlots`.
    fn populate_default_equipment(&self) {
        if self.current_raid_status().is_some() {
            return;
        }
        self.living_base().equipment().lock().set(
            EquipmentSlot::MainHand,
            ItemStack::new(&vanilla_items::IRON_AXE),
        );
    }
}

impl Entity for VindicatorEntity {
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

    /// Latches Johnny the first time the name is applied.
    ///
    /// Vanilla parity: `Vindicator.setCustomName`. The flag is one-way:
    /// renaming a Johnny to anything else does not calm it down.
    fn set_custom_name(&self, custom_name: Option<TextComponent>) {
        let becomes_johnny = !self.is_johnny()
            && custom_name
                .as_ref()
                .is_some_and(|name| name.to_plain(&DisplayResolutor) == JOHNNY_NAME);
        self.entity_set_custom_name(custom_name);
        if becomes_johnny {
            *self.is_johnny.lock() = true;
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        write_patrol_state(self, nbt);
        write_raider_state(self, nbt);
        if self.is_johnny() {
            nbt.insert(TAG_JOHNNY, i8::from(true));
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        read_patrol_state(self, nbt);
        read_raider_state(self, nbt);
        *self.is_johnny.lock() = nbt.byte(TAG_JOHNNY).is_some_and(|value| value != 0);
    }
}

impl LivingEntity for VindicatorEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

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
        Some(&sound_events::ENTITY_VINDICATOR_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_VINDICATOR_DEATH)
    }
}

impl Mob for VindicatorEntity {
    /// Vanilla parity: `Vindicator` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` `SpawnPlacements`
    /// registers for the vindicator -- unlike the pillager, which gets the
    /// patrol rule, a vindicator needs the dark.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        finalize_spawn_raider(self, spawn_reason);
        // Vanilla parity: `Vindicator.finalizeSpawn` turns door opening on for
        // every vindicator, raid or not.
        self.mob_base().navigation().lock().set_can_open_doors(true);
        self.populate_default_equipment();
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `Vindicator.customServerAiStep`, which re-checks door
    /// opening against the raid every tick. Foton has no raid, so a vindicator
    /// that had doors enabled at spawn keeps them; one that did not, does not.
    fn custom_server_ai_step(&self) {
        if self.is_no_ai() {
            return;
        }
        let raided = self.has_active_raid();
        self.mob_base()
            .navigation()
            .lock()
            .set_can_open_doors(raided);
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
        Some(&sound_events::ENTITY_VINDICATOR_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for VindicatorEntity {}

impl PatrollingMonster for VindicatorEntity {
    fn patrol_state(&self) -> &PatrolState {
        &self.patrol_state
    }

    fn can_join_patrol(&self) -> bool {
        self.can_join_patrol_raider()
    }
}

impl Raider for VindicatorEntity {
    fn raider_state(&self) -> &RaiderState {
        &self.raider_state
    }

    /// Vanilla parity: `Vindicator.applyRaidBuffs`. Foton has no enchantment
    /// providers, so the wave-scaled axe arrives plain.
    fn apply_raid_buffs(&self, _wave: i32, _is_captain: bool) {
        self.living_base().equipment().lock().set(
            EquipmentSlot::MainHand,
            ItemStack::new(&vanilla_items::IRON_AXE),
        );
    }

    fn celebrate_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_VINDICATOR_CELEBRATE
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

impl AbstractIllager for VindicatorEntity {
    /// Vanilla parity: `Vindicator.getArmPose`.
    fn arm_pose(&self) -> IllagerArmPose {
        if self.is_aggressive() {
            return IllagerArmPose::Attacking;
        }
        if self.is_celebrating() {
            return IllagerArmPose::Celebrating;
        }
        IllagerArmPose::Crossed
    }
}

impl Enemy for VindicatorEntity {}

#[cfg(test)]
mod tests;
