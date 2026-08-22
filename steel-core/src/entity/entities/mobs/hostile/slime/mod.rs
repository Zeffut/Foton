//! Slime entity.
//!
//! Vanilla parity: `Slime` and the `AbstractCubeMob` it inherits nearly
//! everything from. A slime does not walk: it hops, steering by turning in
//! place between hops, and it is the size that decides everything else -- its
//! health, its speed, its hitbox, whether it hurts on contact, and how many
//! smaller slimes it leaves behind.
//!
//! Steel has no swappable move control, so vanilla's `CubeMobMoveControl` lives
//! here as an override of [`Mob::tick_move_control`] over state the goals set.
//! The shape is the same: the goals ask for a direction and a speed, and the
//! control decides when to jump.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_biome_tags::BiomeTag;
use steel_registry::vanilla_entity_data::SlimeEntityData;
use steel_registry::{sound_events, vanilla_attributes};
use steel_utils::locks::SyncMutex;
use steel_utils::random::Random as _;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::{BlockPos, ChunkPos, Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{Goal, GoalControls, HurtByTargetGoal, NearestAttackableTargetGoal};
use crate::entity::damage::DamageSource;
use crate::entity::living_base::LivingTravelInput;
use crate::entity::mob::rotlerp;
use crate::entity::spawn_rules::check_mob_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData, next_entity_id,
};
use crate::world::{LevelReader as _, World};

/// Largest size a slime may be set to.
///
/// Vanilla parity: the `Mth.clamp(size, 1, 127)` of `setSize`.
const MAX_SIZE: i32 = 127;

/// Size at or below which a slime is harmless and squeaks.
///
/// Vanilla parity: `AbstractCubeMob.isTiny`.
const TINY_SIZE: i32 = 1;

/// Speed a slime of size one moves at.
///
/// Vanilla parity: the `0.2F` base of `setSize`.
const BASE_SPEED: f64 = 0.2;

/// Extra speed each size step adds.
///
/// Vanilla parity: the `0.1F * size` of `setSize`. A big slime is genuinely
/// faster than a small one, which is why they are dangerous in the open.
const SPEED_PER_SIZE: f64 = 0.1;

/// How far a slime turns toward its target each tick while chasing.
///
/// Vanilla parity: the `10.0F` of `lookAt(target, 10.0F, 10.0F)`.
const LOOK_TURN_RATE: f32 = 10.0;

/// How far a slime turns toward its chosen heading each tick.
///
/// Vanilla parity: the `90.0F` of `CubeMobMoveControl.tick`.
const TURN_RATE: f32 = 90.0;

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

/// The hopping state vanilla splits between the mob and its move control.
#[derive(Debug, Default)]
struct CubeState {
    /// Whether the slime was on the ground last tick, for the squash sound.
    was_on_ground: bool,
    /// Heading the goals have asked for, in degrees.
    wanted_y_rot: f32,
    /// Whether the slime is chasing something, which shortens the hop delay.
    is_aggressive: bool,
    /// Speed multiplier the goals have asked for, if any.
    wanted_movement: Option<f64>,
    /// Ticks until the next hop.
    jump_delay: i32,
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
            goals.add_goal(1, SlimeFloatGoal);
            goals.add_goal(2, SlimeAttackGoal::default());
            goals.add_goal(4, SlimeRandomDirectionGoal::default());
            goals.add_goal(5, SlimeKeepOnJumpingGoal);
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

    /// Returns this slime's size.
    #[must_use]
    pub fn size(&self) -> i32 {
        *self.entity_data.lock().abstract_cube_mob().id_size.get()
    }

    /// Returns whether this slime is the smallest kind.
    ///
    /// Vanilla parity: `AbstractCubeMob.isTiny`.
    #[must_use]
    pub fn is_tiny(&self) -> bool {
        self.size() <= TINY_SIZE
    }

    /// Sets the size, and with it the health, speed and hitbox it decides.
    ///
    /// Vanilla parity: `AbstractCubeMob.setSize`.
    pub fn set_size(&self, size: i32, update_health: bool) {
        let size = size.clamp(1, MAX_SIZE);
        self.entity_data
            .lock()
            .abstract_cube_mob_mut()
            .id_size
            .set(size);
        self.refresh_dimensions();

        {
            let mut attributes = self.attributes().lock();
            attributes.set_base_value(vanilla_attributes::MAX_HEALTH, f64::from(size * size));
            attributes.set_base_value(
                vanilla_attributes::MOVEMENT_SPEED,
                SPEED_PER_SIZE.mul_add(f64::from(size), BASE_SPEED),
            );
        }

        if update_health {
            self.set_health(self.get_max_health());
        }
    }

    /// Returns whether this slime hurts what it touches.
    ///
    /// Vanilla parity: `AbstractCubeMob.isDealsDamage`. A tiny slime is
    /// harmless, which is the whole reason players let them pile up.
    #[must_use]
    fn deals_damage(&self) -> bool {
        !self.is_tiny()
    }

    /// Hurts a target the slime is touching.
    ///
    /// Vanilla parity: `AbstractCubeMob.dealDamage`.
    fn deal_damage(&self, world: &Arc<World>, target: &crate::entity::SharedEntity) {
        if !Entity::is_alive(self) {
            return;
        }
        let Some(living) = target.as_living_entity() else {
            return;
        };
        if !self.is_within_melee_attack_range(living) || !self.has_line_of_sight(target.as_ref()) {
            return;
        }
        if self.mob_do_hurt_target(world, target) {
            self.play_sound(&sound_events::ENTITY_SLIME_ATTACK, 1.0, 1.0);
        }
    }

    /// Splits into smaller slimes.
    ///
    /// Vanilla parity: the `remove` override of `AbstractCubeMob`. The children
    /// are placed on the corners of the parent's footprint, which is why a big
    /// slime bursts outward rather than stacking.
    fn split_on_death(&self, world: &Arc<World>) {
        let size = self.size();
        if size <= 1 {
            return;
        }

        let half = size / 2;
        // Vanilla parity: `getSplitCount`, two to four children.
        let count = 2 + rand::random_range(0..3);
        let width = f64::from(self.dimensions_for_pose(self.pose()).width);
        let offset = width / 2.0;
        let origin = self.position();

        for index in 0..count {
            let dx = (f64::from(index % 2) - 0.5) * offset;
            let dz = (f64::from(index / 2) - 0.5) * offset;
            let position = DVec3::new(origin.x + dx, origin.y + 0.5, origin.z + dz);

            let child = Arc::new(Self::new(
                self.entity_type,
                next_entity_id(),
                position,
                Arc::downgrade(world),
            ));
            child.set_size(half, true);
            child.set_rotation((rand::random::<f32>() * 360.0, 0.0));

            if let Err(error) = world.try_add_entity(child) {
                log::debug!("slime split rejected: {error}");
            }
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
    if world.difficulty() == steel_utils::types::Difficulty::Peaceful {
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

/// Hops out of water or lava.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobFloatGoal`.
struct SlimeFloatGoal;

impl Goal for SlimeFloatGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.is_in_water() || mob.is_in_lava()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if rand::random::<f32>() < 0.8 {
            mob.jump_control_jump();
        }
        if let Some(slime) = mob.downcast_ref::<SlimeEntity>() {
            slime.cube.lock().wanted_movement = Some(1.2);
        }
    }
}

/// Faces whatever the slime is chasing and hops at it.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobAttackGoal`.
#[derive(Default)]
struct SlimeAttackGoal {
    grow_tired_timer: i32,
}

impl Goal for SlimeAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.grow_tired_timer = 300;
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if mob.target().is_none() {
            return false;
        }
        self.grow_tired_timer -= 1;
        self.grow_tired_timer > 0
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };
        let Some(slime) = mob.downcast_ref::<SlimeEntity>() else {
            return;
        };

        // Vanilla parity: `lookAt(target, 10.0F, 10.0F)`, a rate-limited turn
        // rather than a snap, and then the move control is handed whatever yaw
        // that reached. Snapping here would let a slime pivot instantly.
        let to_target = target.position() - slime.position();
        let wanted = -(to_target.x.atan2(to_target.z).to_degrees() as f32);
        let (yaw, pitch) = slime.rotation();
        let turned = rotlerp(yaw, wanted, LOOK_TURN_RATE);
        slime.set_rotation((turned, pitch));

        let mut cube = slime.cube.lock();
        cube.wanted_y_rot = turned;
        cube.is_aggressive = slime.deals_damage();
    }
}

/// Picks a new heading every couple of seconds.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobRandomDirectionGoal`.
#[derive(Default)]
struct SlimeRandomDirectionGoal {
    chosen_degrees: f32,
    next_randomize_time: i32,
}

impl Goal for SlimeRandomDirectionGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_none() && (mob.on_ground() || mob.is_in_water() || mob.is_in_lava())
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.next_randomize_time -= 1;
        if self.next_randomize_time <= 0 {
            self.next_randomize_time = 40 + rand::random_range(0..60);
            self.chosen_degrees = rand::random_range(0..360) as f32;
        }

        if let Some(slime) = mob.downcast_ref::<SlimeEntity>() {
            let mut cube = slime.cube.lock();
            cube.wanted_y_rot = self.chosen_degrees;
            cube.is_aggressive = false;
        }
    }
}

/// Keeps the slime hopping when nothing else asks it to.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobKeepOnJumpingGoal`.
struct SlimeKeepOnJumpingGoal;

impl Goal for SlimeKeepOnJumpingGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !mob.is_passenger()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if let Some(slime) = mob.downcast_ref::<SlimeEntity>() {
            let mut cube = slime.cube.lock();
            if cube.wanted_movement.is_none() {
                cube.wanted_movement = Some(1.0);
            }
        }
    }
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

    /// Vanilla parity: the hitbox of `AbstractCubeMob.getDefaultDimensions`,
    /// which is the type's own scaled by the size.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        #[expect(
            clippy::cast_precision_loss,
            reason = "size is clamped to 127 and scales a hitbox"
        )]
        let scale = self.size() as f32;
        self.entity_type.dimensions.scale(scale)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);

        // Vanilla parity: the squash on landing of `AbstractCubeMob.tick`. The
        // sound is the observable half; the squish animation is client-side.
        let on_ground = self.on_ground();
        let landed = {
            let mut cube = self.cube.lock();
            let landed = on_ground && !cube.was_on_ground;
            cube.was_on_ground = on_ground;
            landed
        };
        if landed {
            let sound = if self.is_tiny() {
                &sound_events::ENTITY_SLIME_SQUISH_SMALL
            } else {
                &sound_events::ENTITY_SLIME_SQUISH
            };
            self.play_sound(sound, self.sound_volume(), self.sound_pitch());
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `AbstractCubeMob.playerTouch`.
    fn player_touch(self: Arc<Self>, player: &Arc<crate::player::Player>) {
        if !self.deals_damage() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        let target: crate::entity::SharedEntity = player.clone();
        self.deal_damage(&world, &target);
    }
}

impl SlimeEntity {
    /// Vanilla parity: `AbstractCubeMob.getSoundVolume`.
    fn sound_volume(&self) -> f32 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "size is clamped to 127 and scales a volume"
        )]
        let size = self.size() as f32;
        0.4 * size
    }

    /// Vanilla parity: `AbstractCubeMob.getSoundPitch`.
    fn sound_pitch(&self) -> f32 {
        let adjuster = if self.is_tiny() { 1.4 } else { 0.8 };
        (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0) * adjuster
    }

    /// Vanilla parity: `AbstractCubeMob.getJumpDelay`.
    fn jump_delay(&self) -> i32 {
        rand::random_range(0..20) + 10
    }

    /// Clears the walking input, which is how a slime waits between hops.
    ///
    /// Vanilla parity: the `xxa = 0; zza = 0` of `CubeMobMoveControl.tick`.
    fn stop_traveling(&self) {
        let input = self.travel_input();
        self.set_travel_input(LivingTravelInput::new(0.0, input.vertical(), 0.0));
    }
}

impl LivingEntity for SlimeEntity {
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
            self.split_on_death(&world);
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

    /// Turns in place and hops, instead of walking a path.
    ///
    /// Vanilla parity: `AbstractCubeMob.CubeMobMoveControl.tick`. A slime only
    /// moves while airborne, so the hop cadence is its speed; chasing shortens
    /// the delay to a third, which is what makes a hunting slime close in.
    fn tick_move_control(&self) {
        let (wanted_y_rot, is_aggressive, wanted_movement) = {
            let mut cube = self.cube.lock();
            (
                cube.wanted_y_rot,
                cube.is_aggressive,
                cube.wanted_movement.take(),
            )
        };

        let (yaw, pitch) = self.rotation();
        let turned = rotlerp(yaw, wanted_y_rot, TURN_RATE);
        self.set_rotation((turned, pitch));
        self.set_y_head_rot(turned);
        self.set_y_body_rot(turned);

        let Some(speed_modifier) = wanted_movement else {
            self.stop_traveling();
            return;
        };

        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "movement speed is a small attribute value"
        )]
        let speed = (speed_modifier * movement_speed) as f32;

        if !self.on_ground() {
            self.set_mob_speed(speed);
            return;
        }

        let jump_now = {
            let mut cube = self.cube.lock();
            cube.jump_delay -= 1;
            if cube.jump_delay <= 0 {
                cube.jump_delay = self.jump_delay();
                if is_aggressive {
                    cube.jump_delay /= 3;
                }
                true
            } else {
                false
            }
        };

        if jump_now {
            self.set_mob_speed(speed);
            self.jump_control_jump();
            let sound = if self.is_tiny() {
                &sound_events::ENTITY_SLIME_JUMP_SMALL
            } else {
                &sound_events::ENTITY_SLIME_JUMP
            };
            self.play_sound(sound, self.sound_volume(), self.sound_pitch());
        } else {
            self.stop_traveling();
            self.set_mob_speed(0.0);
        }
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

        let mut size_scale = rand::random_range(0..3);
        let difficulty = world.get_current_difficulty_at(self.block_position());
        if size_scale < 2 && rand::random::<f32>() < 0.5 * difficulty.special_multiplier() {
            size_scale += 1;
        }
        self.set_size(1 << size_scale, true);

        result
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SlimeEntity {}

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
