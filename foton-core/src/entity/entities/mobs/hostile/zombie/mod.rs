//! Zombie entity.
//!
//! Vanilla parity: `Zombie`. The first hostile mob in Foton: it hunts players,
//! closes in and attacks in melee, and retaliates against whatever hurt it.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;
use foton_registry::vanilla_entity_data::ZombieEntityData;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use super::zombie_common;
use crate::entity::Enemy;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::goal::{
    HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::conversion::ConversionReason::Drowned;
use crate::entity::conversion::{ConversionParams, convert_to};
use crate::entity::damage::DamageSource;
use crate::entity::entities::{VillagerEntity, ZombieVillagerEntity};
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::world::World;
use foton_registry::vanilla_entities;
use foton_utils::BlockPos;
use foton_utils::Downcast as _;
use foton_utils::types::Difficulty;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Speed multiplier the zombie uses while chasing.
///
/// Vanilla parity: the `ZombieAttackGoal(this, 1.0, false)` entry.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which a zombie turns to watch a player.
///
/// Vanilla parity: `LookAtPlayerGoal(this, Player.class, 8.0F)`.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Speed multiplier for aimless wandering.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// A zombie.
#[entity_behavior(class = "Zombie")]
pub struct ZombieEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<ZombieEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ZombieEntity`.
unsafe impl DowncastType for ZombieEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/zombie");
}

impl ZombieEntity {
    /// Creates a zombie at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a zombie from saved base data.
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
        let mut entity_data = ZombieEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla Zombie goal priorities in the same order. The goals that
            // need systems Foton lacks are listed in the module TODO instead.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(3, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }

        {
            // Vanilla parity: the zombie's targetSelector.
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
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

    /// Returns whether this zombie is a baby.
    #[must_use]
    pub fn is_baby(&self) -> bool {
        *self.entity_data.lock().baby.get()
    }
}

impl ZombieEntity {
    /// Turns a villager this zombie killed into a zombie villager.
    ///
    /// Vanilla parity: `Zombie.convertVillagerToZombieVillager`. Everything the
    /// villager knew travels with it -- the profession, the trades, the
    /// experience and the gossip -- which is what makes curing it later give
    /// back the same villager rather than a fresh one.
    fn convert_villager_to_zombie_villager(&self, villager: &VillagerEntity) -> bool {
        let villager_type = villager.villager_type();
        let profession = villager.profession();
        let level = villager.villager_level();
        let finalized = villager.villager_data_finalized();
        let gossips = villager.gossips();
        //  rolls the trades if the villager never met a player, so a
        // zombie villager always carries the trades it would have sold.
        let offers = villager.offers();
        let xp = villager.merchant().xp();

        // Vanilla parity: `releaseAllPois` runs on the villager's way out, or
        // its workstation stays claimed by a mob that no longer exists.
        villager.release_all_pois();

        let converted = convert_to(
            villager,
            // Vanilla parity: `ConversionParams.single(villager, true, true)`.
            ConversionParams::single(true, true).with_reason(Drowned),
            |id, position, world| {
                ZombieVillagerEntity::new(&vanilla_entities::ZOMBIE_VILLAGER, id, position, world)
            },
            |zombie_villager| {
                zombie_villager.set_villager_data_finalized(finalized);
                zombie_villager.set_villager_type(villager_type);
                zombie_villager.set_profession(profession);
                zombie_villager.set_villager_level(level);
                zombie_villager.set_gossips(gossips);
                zombie_villager.set_trade_offers(offers);
                zombie_villager.set_villager_xp(xp);
            },
        );

        if converted.is_some()
            && !self.is_silent()
            && let Some(world) = self.level()
        {
            // Vanilla parity: `levelEvent(null, 1026, blockPosition(), 0)`.
            world.level_event(1026, self.block_position(), 0, None);
        }
        converted.is_some()
    }
}

impl Entity for ZombieEntity {
    /// Vanilla parity: `Zombie.killedEntity`, which on Normal and Hard turns a
    /// villager it killed into a zombie villager instead -- and returns false
    /// so the villager drops nothing, because it did not really die.
    fn killed_entity(&self, victim: &dyn LivingEntity, source: &DamageSource) -> bool {
        let perished = true;
        let Some(world) = self.level() else {
            return perished;
        };
        let difficulty = world.difficulty();
        if difficulty != Difficulty::Normal && difficulty != Difficulty::Hard {
            return perished;
        }
        let Some(villager) = victim.downcast_ref::<VillagerEntity>() else {
            return perished;
        };
        // Vanilla parity: on Normal it is a coin flip; on Hard it always happens.
        if difficulty != Difficulty::Hard && rand::random::<bool>() {
            return perished;
        }
        let _ = source;

        if self.convert_villager_to_zombie_villager(villager) {
            return false;
        }
        perished
    }

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

    /// Vanilla parity: `Zombie.addAdditionalSaveData`.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        zombie_common::save_zombie(self, self.is_baby(), nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        zombie_common::load_zombie(self, nbt);
    }
}

impl LivingEntity for ZombieEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Zombie.getBaseExperienceReward`, which is worth reading
    /// twice -- it *mutates* `xpReward` rather than scaling the return, so a
    /// baby zombie's reward compounds if it is ever asked more than once.
    /// Vanilla asks once, at death, and this keeps that shape.
    fn base_experience_reward(&self) -> i32 {
        if self.is_baby() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "vanilla parity: an `(int)` cast of the same product"
            )]
            let scaled = (f64::from(self.xp_reward()) * 2.5) as i32;
            self.set_xp_reward(scaled);
        }
        self.base_experience_reward_mob()
    }

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
        Some(&sound_events::ENTITY_ZOMBIE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_DEATH)
    }
}

impl Mob for ZombieEntity {
    /// Sets whether this zombie is a baby.
    ///
    /// Vanilla parity: `Zombie.setBaby` also swaps in a movement-speed modifier;
    /// Foton only syncs the flag so far.
    fn set_baby(&self, baby: bool) {
        self.entity_data.lock().baby.set(baby);
    }

    /// Vanilla parity: `Zombie` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` this mob is
    /// registered with in `SpawnPlacements`. It only appears in the dark.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
    }

    /// Vanilla parity: `Zombie.canHoldItem`.
    fn can_hold_item(&self, item_stack: &ItemStack) -> bool {
        zombie_common::can_hold_item(self, self.is_baby(), item_stack)
    }

    /// Vanilla parity: `Zombie.wantsToPickUp`.
    fn wants_to_pick_up(&self, world: &World, item_stack: &ItemStack) -> bool {
        zombie_common::wants_to_pick_up(self, world, item_stack)
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
        Some(&sound_events::ENTITY_ZOMBIE_AMBIENT)
    }

    /// Rolls whether this one spawned small.
    ///
    /// Vanilla parity: `Zombie.finalizeSpawn`, which gives one zombie in twenty
    /// the baby form. Foton's zombies are not `AgeableMob`s, so the shared
    /// group-data roll does not reach them and the chance is applied here.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if rand::random::<f32>() < AgeableMobGroupData::DEFAULT_BABY_SPAWN_CHANCE {
            self.set_baby(true);
        }
        // Vanilla parity: the `spawnReason != CONVERSION` guard of
        // `Zombie.finalizeSpawn`. A zombie that drowned into a drowned keeps
        // the flag it already had rather than rerolling it.
        if spawn_reason != EntitySpawnReason::Conversion {
            self.roll_spawn_can_pick_up_loot(world);
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

impl PathfinderMob for ZombieEntity {}

impl Enemy for ZombieEntity {}
