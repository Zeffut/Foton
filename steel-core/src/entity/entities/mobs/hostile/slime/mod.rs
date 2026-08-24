//! Slime entity.
//!
//! Vanilla parity: `Slime`, which is `AbstractCubeMob` plus four sounds and a
//! spawn rule. The hopping, the size arithmetic and the splitting all live in
//! [`super::cube_common`], which is what makes this file short.
//!
//! What is genuinely the slime's own is where it appears: swamps at night, and
//! one chunk in ten deep underground. The second is why slime farms are dug
//! where they are.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_biome_tags::BiomeTag;
use steel_registry::vanilla_entity_data::SlimeEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::random::Random as _;
use steel_utils::random::legacy_random::LegacyRandom;

use super::cube_common::{
    self, CubeAttackGoal, CubeFloatGoal, CubeKeepOnJumpingGoal, CubeLike, CubeRandomDirectionGoal,
    CubeState,
};
use crate::entity::SharedEntity;
use crate::entity::ai::goal::{HurtByTargetGoal, NearestAttackableTargetGoal};
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_mob_spawn_rules;
use steel_utils::{BlockPos, ChunkPos, DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData, next_entity_id,
};
use crate::player::Player;
use crate::world::{LevelReader as _, World};
use steel_utils::types::Difficulty;

/// Height below which a slime spawns in a slime chunk.
///
/// Vanilla parity: the `pos.getY() < 40` of `checkSlimeSpawnRules`.
const SLIME_CHUNK_MAX_Y: i32 = 40;

/// The band a swamp slime spawns in.
///
/// Vanilla parity: the `getY() > 50 && getY() < 70` of `checkSlimeSpawnRules`.
const SURFACE_MIN_Y: i32 = 50;
/// Upper end of that band.
const SURFACE_MAX_Y: i32 = 70;

/// Salt vanilla mixes into the slime-chunk hash.
///
/// Vanilla parity: the `987234911L` of `checkSlimeSpawnRules`.
const SLIME_CHUNK_SALT: i64 = 987_234_911;

/// Chance a swamp slime passes its roll.
///
/// Vanilla parity: `EnvironmentAttributes.SURFACE_SLIME_SPAWN_CHANCE`.
///
/// TODO: read the per-position environment attribute once Steel has them; this
/// is the overworld default the attribute replaced.
const SURFACE_SPAWN_CHANCE: f32 = 0.5;

/// A slime.
#[entity_behavior(class = "Slime")]
pub struct SlimeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SlimeEntityData>,
    cube: SyncMutex<CubeState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SlimeEntity`.
unsafe impl DowncastType for SlimeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/slime");
}

impl SlimeEntity {
    /// Creates a slime at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a slime from saved base data.
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
        let mut entity_data = SlimeEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        entity_data.abstract_cube_mob_mut().id_size.set(1);

        {
            // Vanilla parity: the goal order of `AbstractCubeMob.registerGoals`
            // with `Slime.addBehaviourGoals` slotted in at two.
            let mut goals = mob_base.goal_selector().lock();
            let hooks = cube_common::hooks_for::<Self>();
            goals.add_goal(1, CubeFloatGoal::new(hooks));
            goals.add_goal(2, CubeAttackGoal::new(hooks));
            goals.add_goal(4, CubeRandomDirectionGoal::new(hooks));
            goals.add_goal(5, CubeKeepOnJumpingGoal::new(hooks));
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            // Vanilla parity: a slime only takes a player within four blocks of
            // its own height, so one in a pit does not aggro the surface.
            targets.add_goal(
                1,
                NearestAttackableTargetGoal::new_for_players(true, |slime, target, _| {
                    slime.is_some_and(|slime| {
                        (target.position().y - slime.position().y).abs() <= 4.0
                    })
                }),
            );
            // TODO: vanilla also targets iron golems at priority 3; the golem is
            // not implemented.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            cube: SyncMutex::new(CubeState::default()),
        }
    }
}

/// Returns whether a slime may appear at `pos`.
///
/// Vanilla parity: `Slime.checkSlimeSpawnRules`. Two entirely separate routes
/// lead here: the swamp surface at night, and one chunk in ten deep
/// underground. The second is why slime farms are dug where they are.
#[must_use]
fn check_slime_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    if world.difficulty() == Difficulty::Peaceful {
        return false;
    }
    if spawn_reason.is_spawner() {
        return check_mob_spawn_rules(world, spawn_reason, pos);
    }

    let in_swamp_band = pos.y() > SURFACE_MIN_Y && pos.y() < SURFACE_MAX_Y;
    if in_swamp_band
        && world
            .biome_at(pos)
            .is_some_and(|biome| biome.has_tag(&BiomeTag::ALLOWS_SURFACE_SLIME_SPAWNS))
        && rand::random::<f32>() < SURFACE_SPAWN_CHANCE
        && world.max_local_raw_brightness(pos, world.sky_darkening()) <= rand::random_range(0..8)
    {
        return check_mob_spawn_rules(world, spawn_reason, pos);
    }

    if pos.y() < SLIME_CHUNK_MAX_Y
        && rand::random_range(0..10) == 0
        && is_slime_chunk(ChunkPos::from_block_pos(pos), world.seed())
    {
        return check_mob_spawn_rules(world, spawn_reason, pos);
    }

    false
}

/// Returns whether this chunk is one of the one-in-ten that breed slimes.
///
/// Vanilla parity: `WorldgenRandom.seedSlimeChunk` followed by the
/// `nextInt(10) == 0` of `checkSlimeSpawnRules`. The hash is part of the world
/// seed's contract with players: the same seed always has the same slime
/// chunks, which is what makes them findable.
#[must_use]
fn is_slime_chunk(chunk: ChunkPos, seed: i64) -> bool {
    let x = i64::from(chunk.0.x);
    let z = i64::from(chunk.0.y);
    let hash = seed
        .wrapping_add(x.wrapping_mul(x).wrapping_mul(4_987_142))
        .wrapping_add(x.wrapping_mul(5_947_611))
        .wrapping_add(z.wrapping_mul(z).wrapping_mul(4_392_871))
        .wrapping_add(z.wrapping_mul(389_711))
        ^ SLIME_CHUNK_SALT;

    #[expect(
        clippy::cast_sign_loss,
        reason = "the hash is reinterpreted, matching Java's setSeed(long)"
    )]
    let mut random = LegacyRandom::from_seed(hash as u64);
    random.next_i32_bounded(10) == 0
}

impl Entity for SlimeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        cube_common::dimensions_for_size(self)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
        cube_common::tick_landing(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        let target: SharedEntity = player.clone();
        cube_common::player_touch(self.as_ref(), &target);
    }
}

impl LivingEntity for SlimeEntity {
    fn cube_loot_size(&self) -> Option<i32> {
        Some(CubeLike::size(self))
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
        Some(if self.is_tiny() {
            &sound_events::ENTITY_SLIME_HURT_SMALL
        } else {
            &sound_events::ENTITY_SLIME_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_tiny() {
            &sound_events::ENTITY_SLIME_DEATH_SMALL
        } else {
            &sound_events::ENTITY_SLIME_DEATH
        })
    }

    /// Splits before dying.
    ///
    /// Vanilla parity: the `remove` override of `AbstractCubeMob`, which only
    /// splits when the cube is dying; a slime that despawns leaves nothing
    /// behind. Steel has a death hook and no removal hook, so the split hangs
    /// off death directly, which is the same condition stated more plainly.
    fn die(&self, source: &DamageSource) {
        if self.is_removed() {
            return;
        }
        if let Some(world) = self.level() {
            cube_common::split_on_death(self, &world);
        }
        self.living_die(source);
    }
}

impl Mob for SlimeEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn tick_move_control(&self) {
        cube_common::tick_move_control(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_slime_spawn_rules(world, spawn_reason, pos)
    }

    /// Rolls the size a naturally spawned slime appears at.
    ///
    /// Vanilla parity: `AbstractCubeMob.setSpawnSize`. Harder difficulties tilt
    /// the roll upward, so a hard-mode swamp has more big slimes in it.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let result = self.finalize_spawn_mob_base(world, spawn_reason, group_data);
        cube_common::set_spawn_size(self, world);
        result
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl CubeLike for SlimeEntity {
    fn cube_state(&self) -> &SyncMutex<CubeState> {
        &self.cube
    }

    fn size(&self) -> i32 {
        *self.entity_data.lock().abstract_cube_mob().id_size.get()
    }

    fn store_size(&self, size: i32) {
        self.entity_data
            .lock()
            .abstract_cube_mob_mut()
            .id_size
            .set(size);
    }

    fn jump_sound(&self) -> SoundEventRef {
        if self.is_tiny() {
            &sound_events::ENTITY_SLIME_JUMP_SMALL
        } else {
            &sound_events::ENTITY_SLIME_JUMP
        }
    }

    fn squish_sound(&self) -> SoundEventRef {
        if self.is_tiny() {
            &sound_events::ENTITY_SLIME_SQUISH_SMALL
        } else {
            &sound_events::ENTITY_SLIME_SQUISH
        }
    }

    fn split_child(&self, position: DVec3, world: &Arc<World>) -> SharedEntity {
        let child = Arc::new(Self::new(
            self.entity_type,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        child.set_size(self.size() / 2, true);
        child.set_rotation((rand::random::<f32>() * 360.0, 0.0));
        child
    }
}

impl PathfinderMob for SlimeEntity {}

impl Enemy for SlimeEntity {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slime_chunks_are_about_one_in_ten() {
        // The exact chunks are the seed's business; the rate is the player's.
        let seed = 1_234_567;
        let total = 40 * 40;
        let slime = (0..40)
            .flat_map(|x| (0..40).map(move |z| ChunkPos::new(x, z)))
            .filter(|chunk| is_slime_chunk(*chunk, seed))
            .count();

        assert!(
            (total / 20..=total / 5).contains(&slime),
            "{slime} of {total} chunks is not near one in ten"
        );
    }

    #[test]
    fn the_same_seed_always_picks_the_same_chunks() {
        // Slime farms depend on this: a chunk that bred slimes yesterday has to
        // breed them tomorrow.
        let chunk = ChunkPos::new(7, -13);
        let first = is_slime_chunk(chunk, 99);
        for _ in 0..8 {
            assert_eq!(is_slime_chunk(chunk, 99), first);
        }
    }

    #[test]
    fn different_seeds_disagree_somewhere() {
        let differing = (0..200)
            .filter(|x| {
                let chunk = ChunkPos::new(*x, 0);
                is_slime_chunk(chunk, 1) != is_slime_chunk(chunk, 2)
            })
            .count();
        assert!(differing > 0, "two seeds produced identical slime chunks");
    }
}
