//! Creeper entity.
//!
//! Vanilla parity: `Creeper` and `SwellGoal`. A creeper stalks the player in
//! silence, swells while it is close enough, and detonates when the fuse fills.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::CreeperEntityData;
use steel_registry::vanilla_game_rules::{MOB_EXPLOSION_DROP_DECAY, MOB_GRIEFING};
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::SharedEntity;
use crate::entity::ai::goal::{
    FloatGoal, Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::{AreaEffectCloudEntity, CREEPER_CLOUD_DURATION_SCALE};
use crate::entity::next_entity_id;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob, RemovalReason,
};
use crate::world::World;
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};
use std::sync::Arc;
use steel_registry::vanilla_entities;
use steel_utils::BlockPos;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Ticks the fuse takes to fill, unless the compound says otherwise.
///
/// Vanilla parity: the `maxSwell = 30` initialiser, and the default of the
/// `Fuse` read. Vanilla's field is an `int`, but it is only ever written from a
/// short, so the narrower type is the actual range.
const DEFAULT_MAX_SWELL: i16 = 30;

/// Blast radius of an ordinary creeper.
///
/// Vanilla parity: the `explosionRadius = 3` initialiser, and the default of
/// the `ExplosionRadius` read. Vanilla's field is an `int`, but it is only ever
/// written from a byte, so the narrower type is the actual range.
const DEFAULT_EXPLOSION_RADIUS: i8 = 3;

/// Squared distance within which a creeper starts swelling.
///
/// Vanilla parity: the `49.0` threshold in `SwellGoal`.
const SWELL_RANGE_SQR: f64 = 49.0;

/// Speed multiplier while chasing.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Speed multiplier while wandering.
const STROLL_SPEED_MODIFIER: f64 = 0.8;

/// Distance at which a creeper watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// A creeper.
#[entity_behavior(class = "Creeper")]
pub struct CreeperEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CreeperEntityData>,
    /// How full the fuse is, from 0 to [`Self::max_swell`].
    swell: SyncMutex<i32>,
    /// How full the fuse has to get before this one detonates.
    ///
    /// Vanilla parity: `Creeper.maxSwell`, which the `Fuse` key sets.
    max_swell: SyncMutex<i16>,
    /// How large a hole this one leaves.
    ///
    /// Vanilla parity: `Creeper.explosionRadius`, which the `ExplosionRadius`
    /// key sets.
    explosion_radius: SyncMutex<i8>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CreeperEntity`.
unsafe impl DowncastType for CreeperEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/creeper");
}

impl CreeperEntity {
    /// Creates a creeper at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a creeper from saved base data.
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
        let mut entity_data = CreeperEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        entity_data.swell_dir.set(-1);

        {
            // Keep vanilla Creeper goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(2, SwellGoal);
            goals.add_goal(4, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(6, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(6, RandomLookAroundGoal::new());
            // TODO: vanilla also flees ocelots and cats at priority 3.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(
                1,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
            targets.add_goal(2, HurtByTargetGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            swell: SyncMutex::new(0),
            max_swell: SyncMutex::new(DEFAULT_MAX_SWELL),
            explosion_radius: SyncMutex::new(DEFAULT_EXPLOSION_RADIUS),
        }
    }

    /// Returns the fuse direction: 1 while swelling, -1 while relaxing.
    #[must_use]
    pub fn swell_dir(&self) -> i32 {
        *self.entity_data.lock().swell_dir.get()
    }

    /// Sets the fuse direction.
    pub fn set_swell_dir(&self, dir: i32) {
        self.entity_data.lock().swell_dir.set(dir);
    }

    /// Returns whether this creeper was struck by lightning.
    #[must_use]
    pub fn is_powered(&self) -> bool {
        *self.entity_data.lock().is_powered.get()
    }

    /// Returns whether the creeper was lit by hand and can no longer stop.
    #[must_use]
    pub fn is_ignited(&self) -> bool {
        *self.entity_data.lock().is_ignited.get()
    }

    /// Lights the fuse for good.
    ///
    /// Vanilla parity: `Creeper.ignite`. There is no way back from it: the
    /// swell goal stops mattering and the fuse fills on its own.
    pub fn ignite(&self) {
        self.entity_data.lock().is_ignited.set(true);
    }

    /// Advances the fuse and detonates when it fills.
    ///
    /// Vanilla parity: the swell block of `Creeper.tick`.
    fn tick_swell(&self) {
        if !Entity::is_alive(self) {
            return;
        }
        if self.is_ignited() {
            self.set_swell_dir(1);
        }

        let dir = self.swell_dir();
        let max_swell = i32::from(*self.max_swell.lock());
        let reached_max = {
            let mut swell = self.swell.lock();
            if dir > 0
                && *swell == 0
                && let Some(world) = self.level()
            {
                world.play_sound_at(
                    &sound_events::ENTITY_CREEPER_PRIMED,
                    SoundSource::Hostile,
                    self.position(),
                    1.0,
                    0.5,
                    None,
                );
            }

            *swell = (*swell + dir).max(0);
            if *swell >= max_swell {
                *swell = max_swell;
                true
            } else {
                false
            }
        };

        if reached_max {
            self.explode();
        }
    }

    /// Detonates and removes the creeper.
    ///
    /// Vanilla parity: `Creeper.explodeCreeper`. A charged creeper doubles its
    /// radius.
    fn explode(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let multiplier = if self.is_powered() { 2.0 } else { 1.0 };
        let radius = f32::from(*self.explosion_radius.lock());
        // A creeper is its own cause: vanilla's `getIndirectSourceEntity`
        // returns the source unchanged when it is a living entity.
        world.explode(
            ExplosionSpec::new(
                Some(self.id()),
                Some(self.id()),
                None,
                radius * multiplier,
                false,
                // Vanilla `ExplosionInteraction.MOB`: mob griefing decides whether
                // blocks break at all, and the decay rule whether their drops thin.
                if world.get_game_rule(&MOB_GRIEFING) {
                    world.explosion_destroy_type(&MOB_EXPLOSION_DROP_DECAY)
                } else {
                    ExplosionBlockInteraction::Keep
                },
            ),
            self.position(),
        );
        self.spawn_lingering_cloud(&world);
        self.set_removed(RemovalReason::Killed);
    }

    /// Leaves behind a cloud of whatever this creeper was carrying.
    ///
    /// Vanilla parity: `Creeper.spawnLingeringCloud`. A creeper that walked
    /// through a lingering potion carries it into its own explosion, which is
    /// what an ominous raid's creepers are for.
    fn spawn_lingering_cloud(&self, world: &Arc<World>) {
        let effects = self
            .active_mob_effects()
            .into_iter()
            .map(|active| {
                let duration = if active.is_infinite_duration() {
                    active.duration()
                } else {
                    active.duration() / CREEPER_CLOUD_DURATION_SCALE
                };
                (active.effect(), duration, active.amplifier())
            })
            .collect::<Vec<_>>();
        if effects.is_empty() {
            return;
        }

        let cloud = Arc::new(AreaEffectCloudEntity::new(
            &vanilla_entities::AREA_EFFECT_CLOUD,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        ));
        cloud.configure_as_creeper_cloud(effects);
        if let Err(error) = world.try_add_entity(cloud as SharedEntity) {
            log::debug!("failed to spawn a creeper's lingering cloud: {error}");
        }
    }
}

/// Drives the creeper's fuse from how close its target is.
///
/// Vanilla parity: `SwellGoal`.
struct SwellGoal;

impl Goal for SwellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(creeper) = mob.downcast_ref::<CreeperEntity>() else {
            return false;
        };
        if creeper.swell_dir() > 0 {
            return true;
        }
        mob.target().is_some_and(|target| {
            target.position().distance_squared(mob.position()) < SWELL_RANGE_SQR
        })
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(creeper) = mob.downcast_ref::<CreeperEntity>() else {
            return;
        };
        let close_enough = mob.target().is_some_and(|target| {
            target.position().distance_squared(mob.position()) <= SWELL_RANGE_SQR
        });
        creeper.set_swell_dir(if close_enough { 1 } else { -1 });
    }
}

impl Entity for CreeperEntity {
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
        self.tick_swell();
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Creeper.addAdditionalSaveData`, whose own contribution
    /// is the charge, the fuse length, the blast radius and the hand-lit flag.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("powered", i8::from(self.is_powered()));
        nbt.insert("Fuse", *self.max_swell.lock());
        nbt.insert("ExplosionRadius", *self.explosion_radius.lock());
        nbt.insert("ignited", i8::from(self.is_ignited()));
    }

    /// Vanilla parity: `Creeper.readAdditionalSaveData`. A missing `ignited`
    /// leaves the creeper unlit rather than putting an already lit fuse out,
    /// which is why this only ever calls [`CreeperEntity::ignite`].
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.entity_data
            .lock()
            .is_powered
            .set(nbt.byte("powered").is_some_and(|value| value != 0));
        *self.max_swell.lock() = nbt.short("Fuse").unwrap_or(DEFAULT_MAX_SWELL);
        *self.explosion_radius.lock() = nbt
            .byte("ExplosionRadius")
            .unwrap_or(DEFAULT_EXPLOSION_RADIUS);
        if nbt.byte("ignited").is_some_and(|value| value != 0) {
            self.ignite();
        }
    }

    /// Vanilla parity: `Creeper.thunderHit`, which takes the damage and the
    /// singeing like anything else and then stays charged for good.
    fn thunder_hit(&self, world: &World, _bolt: &dyn Entity) {
        self.entity_thunder_hit(world);
        self.entity_data.lock().is_powered.set(true);
    }
}

impl LivingEntity for CreeperEntity {
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
        Some(&sound_events::ENTITY_CREEPER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_CREEPER_DEATH)
    }
}

impl Mob for CreeperEntity {
    /// Vanilla parity: `Creeper` derives from `Monster`.
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

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for CreeperEntity {}

impl Enemy for CreeperEntity {}

#[cfg(test)]
mod lingering_cloud_tests {
    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::AreaEffectCloudEntity;
    use crate::entity::{LivingEntity, MobEffectInstance};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use steel_registry::{init_vanilla_registry, vanilla_mob_effects};
    use steel_utils::{ChunkPos, WorldAabb};

    /// Ticks of poison the creeper is carrying when it goes off.
    const CARRIED_POISON_TICKS: i32 = 400;

    fn creeper_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn creeper_at(world: &Arc<World>) -> Arc<CreeperEntity> {
        let creeper = Arc::new(CreeperEntity::new(
            &vanilla_entities::CREEPER,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(Arc::clone(&creeper) as SharedEntity)
            .expect("the test chunk is loaded");
        creeper
    }

    fn clouds(world: &Arc<World>) -> Vec<SharedEntity> {
        let everywhere = WorldAabb::new(-256.0, -64.0, -256.0, 256.0, 320.0, 256.0);
        world.get_entities_in_aabb_matching(&everywhere, |entity| {
            entity.downcast_ref::<AreaEffectCloudEntity>().is_some()
        })
    }

    #[test]
    fn a_creeper_carrying_nothing_leaves_no_cloud() {
        let world = creeper_world("creeper_plain_leaves_nothing");
        let creeper = creeper_at(&world);

        creeper.explode();

        assert!(clouds(&world).is_empty());
    }

    #[test]
    fn a_creeper_carrying_an_effect_leaves_it_behind() {
        let world = creeper_world("creeper_leaves_its_effect");
        let creeper = creeper_at(&world);
        assert!(creeper.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::POISON,
            CARRIED_POISON_TICKS,
            0,
        )));

        creeper.explode();

        let left = clouds(&world);
        assert_eq!(left.len(), 1, "a creeper with an effect leaves one cloud");
        let cloud = left[0]
            .as_ref()
            .downcast_ref::<AreaEffectCloudEntity>()
            .expect("filtered above");
        assert!(
            (cloud.radius() - 2.5).abs() < 1.0e-6,
            "a creeper's cloud is smaller than a potion's, got {}",
            cloud.radius()
        );
    }
}
