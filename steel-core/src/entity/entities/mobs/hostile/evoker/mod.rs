//! Evoker entity.
//!
//! Vanilla parity: `Evoker`. The illager that never touches you: it backs away
//! from anything that gets close and answers with fangs from the floor, an
//! escort of vexes, and -- with nothing to fight -- a blue sheep turned red.
//! All three run on the [`crate::entity::SpellcasterIllager`] rhythm.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::EvokerEntityData;
use steel_registry::{sound_events, vanilla_entities};
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::abstract_illager::{AbstractIllager, IllagerArmPose};
use crate::entity::ai::goal::{
    AvoidEntityGoal, FloatGoal, HurtByTargetGoal, LongDistancePatrolGoal, LookAtPlayerGoal,
    NearestAttackableTargetGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal,
    RaiderCelebrationGoal, RaiderMoveThroughVillageGoal, RandomStrollGoal,
    SpellcasterCastingSpellGoal, no_creative_or_spectator,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::VexEntity;
use crate::entity::patrolling_monster::{
    PatrolState, PatrollingMonster, read_patrol_state, write_patrol_state,
};
use crate::entity::raider::{
    Raider, RaiderState, finalize_spawn_raider, read_raider_state, write_raider_state,
};
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::spellcaster_illager::{
    SpellcasterIllager, SpellcasterState, read_spellcaster_state, write_spellcaster_state,
};
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::world::World;

mod spells;

use spells::{EvokerAttackSpellGoal, EvokerSummonSpellGoal, EvokerWololoSpellGoal};

/// Experience an evoker drops.
///
/// Vanilla parity: the `xpReward = 10` of the constructor.
const XP_REWARD: i32 = 10;

/// How close a player may get before the evoker backs away.
///
/// Vanilla parity: the `8.0F` of `AvoidEntityGoal`.
const AVOID_PLAYER_DISTANCE: f32 = 8.0;

/// Speed the evoker walks away at.
const AVOID_WALK_SPEED: f64 = 0.6;

/// Speed the evoker runs away at.
const AVOID_SPRINT_SPEED: f64 = 1.0;

/// Speed an evoker wanders at.
const STROLL_SPEED_MODIFIER: f64 = 0.6;

/// Distance at which an evoker watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 3.0;

/// How often an evoker bothers to watch a player.
const LOOK_AT_PLAYER_PROBABILITY: f32 = 1.0;

/// Distance at which an evoker watches another mob.
const LOOK_AT_MOB_RANGE: f64 = 8.0;

/// How often an evoker bothers to watch another mob.
const LOOK_AT_MOB_PROBABILITY: f32 = 0.02;

/// How long a caster keeps chasing a target it has lost sight of.
///
/// Vanilla parity: the `setUnseenMemoryTicks(300)` of the target goals.
const UNSEEN_MEMORY_TICKS: i32 = 300;

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

/// An evoker.
#[entity_behavior(class = "Evoker")]
pub struct EvokerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<EvokerEntityData>,
    patrol_state: PatrolState,
    raider_state: RaiderState,
    spellcaster_state: SpellcasterState,
    /// The sheep this evoker is in the middle of recoloring.
    wololo_target: SyncMutex<Option<Weak<dyn Entity>>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EvokerEntity`.
unsafe impl DowncastType for EvokerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/evoker");
}

impl EvokerEntity {
    /// Creates an evoker at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an evoker from saved base data.
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
        let mut entity_data = EvokerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Evoker.registerGoals`, over
            // the ones `PatrollingMonster` and `Raider` add.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(
                1,
                SpellcasterCastingSpellGoal::new().with_look_target(casting_look_target),
            );
            goals.add_goal(
                2,
                AvoidEntityGoal::with_selector(
                    AVOID_PLAYER_DISTANCE,
                    AVOID_WALK_SPEED,
                    AVOID_SPRINT_SPEED,
                    |_, target, _| target.as_player().is_some() && no_creative_or_spectator(target),
                ),
            );
            goals.add_goal(4, EvokerSummonSpellGoal::new());
            goals.add_goal(
                4,
                LongDistancePatrolGoal::new(PATROL_SPEED, PATROL_LEADER_SPEED),
            );
            goals.add_goal(5, EvokerAttackSpellGoal::new());
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
            goals.add_goal(6, EvokerWololoSpellGoal::new());
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
            // Vanilla also flees a creaking at priority 3, and adds three
            // raid-only goals. Steel has no creaking and no raid.
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
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true)
                    .with_unseen_memory_ticks(UNSEEN_MEMORY_TICKS),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new(false, |_, target, _| {
                    target.entity_type() == &vanilla_entities::IRON_GOLEM
                }),
            );
            // Vanilla also hunts villagers at priority 3; Steel has none.
        }

        let evoker = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            patrol_state: PatrolState::new(),
            raider_state: RaiderState::new(),
            spellcaster_state: SpellcasterState::new(),
            wololo_target: SyncMutex::new(None),
        };
        evoker.set_xp_reward(XP_REWARD);
        evoker
    }

    /// Returns the sheep this evoker is recoloring.
    ///
    /// Vanilla parity: `Evoker.getWololoTarget`.
    #[must_use]
    pub fn wololo_target(&self) -> Option<SharedEntity> {
        self.wololo_target.lock().as_ref().and_then(Weak::upgrade)
    }

    /// Sets the sheep this evoker is recoloring.
    ///
    /// Vanilla parity: `Evoker.setWololoTarget`.
    pub fn set_wololo_target(&self, target: Option<&SharedEntity>) {
        *self.wololo_target.lock() = target.map(Arc::downgrade);
    }
}

/// Returns what an evoker watches while casting.
///
/// Vanilla parity: `Evoker.EvokerCastingSpellGoal.tick`, which is the only
/// reason the class exists: an evoker mid-wololo has no target, and without
/// this it would stare past the sheep it is recoloring.
fn casting_look_target(mob: &dyn PathfinderMob) -> Option<SharedEntity> {
    if let Some(target) = mob.target() {
        return Some(target);
    }
    mob.downcast_ref::<EvokerEntity>()
        .and_then(EvokerEntity::wololo_target)
}

impl Entity for EvokerEntity {
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

    /// Vanilla parity: `Evoker.considersEntityAsAlly`. An evoker counts its
    /// own vexes as friends, which is what keeps its fangs off the escort it
    /// just summoned.
    fn is_allied_to(&self, other: &dyn Entity) -> bool {
        if other.uuid() == self.uuid() || self.considers_entity_as_ally_illager(other) {
            return true;
        }
        let Some(vex) = other.downcast_ref::<VexEntity>() else {
            return false;
        };
        // Vanilla walks `getRootOwner`; Steel's vex keeps one owner, and a vex
        // is never owned by another vex, so one hop is the whole chain.
        vex.owner().is_some_and(|owner| {
            owner.uuid() == self.uuid() || self.considers_entity_as_ally_illager(owner.as_ref())
        })
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        write_patrol_state(self, nbt);
        write_raider_state(self, nbt);
        write_spellcaster_state(self, nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        read_patrol_state(self, nbt);
        read_raider_state(self, nbt);
        read_spellcaster_state(self, nbt);
    }
}

impl LivingEntity for EvokerEntity {
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
        Some(&sound_events::ENTITY_EVOKER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_EVOKER_DEATH)
    }
}

impl Mob for EvokerEntity {
    /// Vanilla parity: `Evoker` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` `SpawnPlacements`
    /// registers for the evoker.
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
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `SpellcasterIllager.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        self.spellcaster_custom_server_ai_step();
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
        Some(&sound_events::ENTITY_EVOKER_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for EvokerEntity {}

impl PatrollingMonster for EvokerEntity {
    fn patrol_state(&self) -> &PatrolState {
        &self.patrol_state
    }

    fn can_join_patrol(&self) -> bool {
        self.can_join_patrol_raider()
    }
}

impl Raider for EvokerEntity {
    fn raider_state(&self) -> &RaiderState {
        &self.raider_state
    }

    /// Vanilla parity: `Evoker.applyRaidBuffs`, which is empty.
    fn apply_raid_buffs(&self, _wave: i32, _is_captain: bool) {}

    fn celebrate_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_EVOKER_CELEBRATE
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

impl AbstractIllager for EvokerEntity {
    /// Vanilla parity: `SpellcasterIllager.getArmPose`.
    fn arm_pose(&self) -> IllagerArmPose {
        if self.is_casting_spell() {
            return IllagerArmPose::Spellcasting;
        }
        if self.is_celebrating() {
            return IllagerArmPose::Celebrating;
        }
        IllagerArmPose::Crossed
    }
}

impl SpellcasterIllager for EvokerEntity {
    fn spellcaster_state(&self) -> &SpellcasterState {
        &self.spellcaster_state
    }

    fn set_synced_spell_id(&self, id: i8) {
        self.entity_data
            .lock()
            .spellcaster_illager_mut()
            .spell_casting
            .set(id);
    }

    fn casting_sound_event(&self) -> SoundEventRef {
        &sound_events::ENTITY_EVOKER_CAST_SPELL
    }
}

impl Enemy for EvokerEntity {}

#[cfg(test)]
mod tests;
