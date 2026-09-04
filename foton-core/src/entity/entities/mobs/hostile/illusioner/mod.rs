//! Illusioner entity.
//!
//! Vanilla parity: `Illusioner`. The illager nothing spawns: it exists only
//! behind a summon command, and it is the only illager that carries a bow. Both
//! of its spells attack a player's information rather than their health -- one
//! blinds, the other splits the illusioner into four decoys the client draws
//! and the server knows nothing about.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::IllusionerEntityData;
use foton_registry::{sound_events, vanilla_entities, vanilla_items};
use foton_utils::BlockPos;
use foton_utils::locks::SyncMutex;
use foton_utils::{Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::LivingEntitySyncedData;
use crate::entity::abstract_illager::{AbstractIllager, IllagerArmPose};
use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LongDistancePatrolGoal, LookAtPlayerGoal,
    NearestAttackableTargetGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal,
    RaiderCelebrationGoal, RaiderMoveThroughVillageGoal, RandomStrollGoal, RangedBowAttackGoal,
    SpellcasterCastingSpellGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ArrowEntity;
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
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::world::World;

mod spells;

use spells::{IllusionerBlindnessSpellGoal, IllusionerMirrorSpellGoal};

/// Experience an illusioner drops.
///
/// Vanilla parity: the `xpReward = 5` of the constructor.
const XP_REWARD: i32 = 5;

/// Ticks between two arrows.
///
/// Vanilla parity: the `20` interval of `RangedBowAttackGoal`.
const BOW_ATTACK_INTERVAL: i32 = 20;

/// Range within which the illusioner shoots rather than approaches.
const BOW_ATTACK_RADIUS: f64 = 15.0;

/// Speed the illusioner closes the distance at.
///
/// Vanilla parity: the `0.5` of `RangedBowAttackGoal`, half a skeleton's.
const BOW_APPROACH_SPEED: f64 = 0.5;

/// Speed the arrow leaves the bow at.
///
/// Vanilla parity: the `1.6F` of `Projectile.spawnProjectileUsingShoot`.
const ARROW_POWER: f32 = 1.6;

/// Spread of a mob-fired arrow before difficulty is applied.
///
/// Vanilla parity: the `14 - difficulty * 4` of `performRangedAttack`.
const ARROW_UNCERTAINTY_BASE: f32 = 14.0;

/// How much each difficulty step tightens the spread.
const ARROW_UNCERTAINTY_PER_DIFFICULTY: f32 = 4.0;

/// Fraction of a target's height an arrow is aimed at.
///
/// Vanilla parity: the `getY(0.3333333333333333)` of `performRangedAttack`.
const AIM_HEIGHT_FRACTION: f64 = 1.0 / 3.0;

/// Speed an illusioner wanders at.
const STROLL_SPEED_MODIFIER: f64 = 0.6;

/// Distance at which an illusioner watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 3.0;

/// How often an illusioner bothers to watch a player.
const LOOK_AT_PLAYER_PROBABILITY: f32 = 1.0;

/// Distance at which an illusioner watches another mob.
const LOOK_AT_MOB_RANGE: f64 = 8.0;

/// How often an illusioner bothers to watch another mob.
const LOOK_AT_MOB_PROBABILITY: f32 = 0.02;

/// How long a caster keeps chasing a target it has lost sight of.
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

/// An illusioner.
#[entity_behavior(class = "Illusioner")]
pub struct IllusionerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<IllusionerEntityData>,
    patrol_state: PatrolState,
    raider_state: RaiderState,
    spellcaster_state: SpellcasterState,
}

// SAFETY: This key is owned by Foton and uniquely identifies `IllusionerEntity`.
unsafe impl DowncastType for IllusionerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/illusioner");
}

impl IllusionerEntity {
    /// Creates an illusioner at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an illusioner from saved base data.
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
        let mut entity_data = IllusionerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Illusioner.registerGoals`,
            // over the ones `PatrollingMonster` and `Raider` add.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(1, SpellcasterCastingSpellGoal::new());
            goals.add_goal(4, IllusionerMirrorSpellGoal::new());
            goals.add_goal(
                4,
                LongDistancePatrolGoal::new(PATROL_SPEED, PATROL_LEADER_SPEED),
            );
            goals.add_goal(5, IllusionerBlindnessSpellGoal::new());
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
            goals.add_goal(
                6,
                RangedBowAttackGoal::new(
                    BOW_ATTACK_INTERVAL,
                    BOW_ATTACK_RADIUS,
                    BOW_APPROACH_SPEED,
                    fire_arrow,
                ),
            );
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
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true)
                    .with_unseen_memory_ticks(UNSEEN_MEMORY_TICKS),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new(false, |_, target, _| {
                    target.entity_type() == &vanilla_entities::IRON_GOLEM
                })
                .with_unseen_memory_ticks(UNSEEN_MEMORY_TICKS),
            );
            // Vanilla also hunts villagers at priority 3; Foton has none.
        }

        let illusioner = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            patrol_state: PatrolState::new(),
            raider_state: RaiderState::new(),
            spellcaster_state: SpellcasterState::new(),
        };
        illusioner.set_xp_reward(XP_REWARD);
        illusioner
    }
}

/// Looses an arrow at `target`.
///
/// Vanilla parity: `Illusioner.performRangedAttack`. Foton's arrow already
/// applies the same `distance * 0.2` lob, so the aim point is the only part
/// worth spelling out. Vanilla draws the projectile from the illusioner's
/// quiver; Foton's mobs have none, so the arrow is a plain one.
fn fire_arrow(mob: &dyn PathfinderMob, target: DVec3) {
    let Some(illusioner) = mob.downcast_ref::<IllusionerEntity>() else {
        return;
    };
    let Some(world) = mob.level() else {
        return;
    };

    // The goal hands over the target's feet; vanilla aims a third of the way
    // up its body instead, which is the difference between hitting a player
    // and hitting the block they are standing on.
    let target = mob.target().map_or(target, |entity| {
        let height = f64::from(entity.base().dimensions().height);
        target.with_y(height.mul_add(AIM_HEIGHT_FRACTION, entity.position().y))
    });

    let difficulty = u8::from(world.difficulty());
    let uncertainty =
        ARROW_UNCERTAINTY_PER_DIFFICULTY.mul_add(-f32::from(difficulty), ARROW_UNCERTAINTY_BASE);
    let arrow = ArrowEntity::shoot_at(&world, illusioner, target, ARROW_POWER, uncertainty);
    drop(arrow);

    world.play_sound_at(
        &sound_events::ENTITY_SKELETON_SHOOT,
        illusioner.sound_source(),
        illusioner.position(),
        1.0,
        0.4f32.mul_add(rand::random::<f32>(), 0.8).recip(),
        None,
    );
}

impl Entity for IllusionerEntity {
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
        write_spellcaster_state(self, nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        read_patrol_state(self, nbt);
        read_raider_state(self, nbt);
        read_spellcaster_state(self, nbt);
    }
}

impl LivingEntity for IllusionerEntity {
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
        Some(&sound_events::ENTITY_ILLUSIONER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ILLUSIONER_DEATH)
    }
}

impl Mob for IllusionerEntity {
    /// Vanilla parity: `Illusioner` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` `SpawnPlacements`
    /// registers for the illusioner. Nothing in vanilla spawns one naturally --
    /// it is in no biome's spawn list -- but the rule is registered and Foton
    /// answers it the same way.
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
        // Vanilla parity: `Illusioner.finalizeSpawn` arms it before the shared
        // raider spawn work.
        self.living_base()
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, ItemStack::new(&vanilla_items::BOW));
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
        Some(&sound_events::ENTITY_ILLUSIONER_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for IllusionerEntity {}

impl PatrollingMonster for IllusionerEntity {
    fn patrol_state(&self) -> &PatrolState {
        &self.patrol_state
    }

    fn can_join_patrol(&self) -> bool {
        self.can_join_patrol_raider()
    }
}

impl Raider for IllusionerEntity {
    fn raider_state(&self) -> &RaiderState {
        &self.raider_state
    }

    /// Vanilla parity: `Illusioner.applyRaidBuffs`, which is empty.
    fn apply_raid_buffs(&self, _wave: i32, _is_captain: bool) {}

    /// Vanilla parity: `Illusioner.getCelebrateSound`, which reuses the ambient
    /// sound rather than having one of its own.
    fn celebrate_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_ILLUSIONER_AMBIENT
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

impl AbstractIllager for IllusionerEntity {
    /// Vanilla parity: `Illusioner.getArmPose`, which unlike the evoker's puts
    /// the bow before the celebration -- an illusioner never celebrates
    /// visibly, because it is never in a raid.
    fn arm_pose(&self) -> IllagerArmPose {
        if self.is_casting_spell() {
            return IllagerArmPose::Spellcasting;
        }
        if self.is_aggressive() {
            return IllagerArmPose::BowAndArrow;
        }
        IllagerArmPose::Crossed
    }
}

impl SpellcasterIllager for IllusionerEntity {
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
        &sound_events::ENTITY_ILLUSIONER_CAST_SPELL
    }
}

impl Enemy for IllusionerEntity {}

#[cfg(test)]
mod tests;
