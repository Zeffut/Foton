//! Zombie entity.
//!
//! Vanilla parity: `Zombie`. The first hostile mob in Steel: it hunts players,
//! closes in and attacks in melee, and retaliates against whatever hurt it.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::ZombieEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::ai::goal::{
    HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::world::World;
use steel_utils::BlockPos;

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

// SAFETY: This key is owned by Steel and uniquely identifies `ZombieEntity`.
unsafe impl DowncastType for ZombieEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/zombie");
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
        let mut entity_data = ZombieEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla Zombie goal priorities in the same order. The goals that
            // need systems Steel lacks are listed in the module TODO instead.
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

    /// Sets whether this zombie is a baby.
    ///
    /// Vanilla parity: `Zombie.setBaby` also swaps in a movement-speed modifier;
    /// Steel only syncs the flag so far.
    pub fn set_baby(&self, baby: bool) {
        self.entity_data.lock().baby.set(baby);
    }
}

impl Entity for ZombieEntity {
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

impl LivingEntity for ZombieEntity {
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
        Some(&sound_events::ENTITY_ZOMBIE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_DEATH)
    }
}

impl Mob for ZombieEntity {
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
    /// the baby form. Steel's zombies are not `AgeableMob`s, so the shared
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
