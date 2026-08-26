//! Creaking heart block entity.
//!
//! Vanilla parity: `CreakingHeartBlockEntity`. The heart is the whole of the
//! creaking's life cycle: it wakes one at night while a player is near, keeps
//! it on a thirty-four block tether, takes every blow the creaking is dealt --
//! spreading resin and creaking audibly as it does -- and tears the creaking
//! down when it is broken, when the night ends, or when the creaking has wedged
//! itself inside a player.
//!
//! Vanilla holds the protector as `Either<Creaking, UUID>`: the left branch is
//! only a resolved cache of the right, and every read falls back to looking the
//! UUID up in the level. Steel keeps the UUID alone and does that lookup, which
//! is the same behavior with one field instead of two -- including the thirty
//! tick grace period after a load, during which an unresolvable UUID is kept
//! rather than dropped, because the creaking's chunk may not be back yet.

use std::sync::{Arc, Weak};

use glam::DVec3;
use rand::RngExt as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, CreakingHeartState, Direction};
use steel_registry::particle_type::{ParticleData, TrailParticleOption};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_game_rules::SPAWN_MONSTERS;
use steel_registry::{
    sound_events, vanilla_block_entity_types, vanilla_blocks, vanilla_entities,
    vanilla_game_events, vanilla_particle_types,
};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::{Difficulty, UpdateFlags};
use steel_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, RgbColor, UuidExt as _,
};
use uuid::Uuid;

use crate::behavior::blocks::{
    CREAKING_HEART_STATE, CreakingHeartBlock, creaking_heart_awake_or_dormant,
    multiface_face_property,
};
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::damage::DamageSource;
use crate::entity::entities::CreakingEntity;
use crate::entity::spawn_util::{SpawnStrategy, try_spawn_mob};
use crate::entity::{Entity as _, EntitySpawnReason, LivingEntity as _, Mob as _, SharedEntity};
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

/// Vanilla `CreakingHeartBlockEntity.UPDATE_TICKS`.
const UPDATE_TICKS: i32 = 20;
/// Vanilla `CreakingHeartBlockEntity.UPDATE_TICKS_VARIANCE`.
const UPDATE_TICKS_VARIANCE: i32 = 5;
/// Vanilla `CreakingHeartBlockEntity.PLAYER_DETECTION_RANGE`.
const PLAYER_DETECTION_RANGE: f64 = 32.0;
/// Vanilla `CreakingHeartBlockEntity.CREAKING_ROAMING_RADIUS`, which is also the
/// distance the comparator output falls off over.
const CREAKING_ROAMING_RADIUS: f64 = 32.0;
/// Vanilla `CreakingHeartBlockEntity.DISTANCE_CREAKING_TOO_FAR`.
const DISTANCE_CREAKING_TOO_FAR: f64 = 34.0;
/// Vanilla `CreakingHeartBlockEntity.SPAWN_RANGE_XZ`.
const SPAWN_RANGE_XZ: i32 = 16;
/// Vanilla `CreakingHeartBlockEntity.SPAWN_RANGE_Y`.
const SPAWN_RANGE_Y: i32 = 8;
/// Vanilla `CreakingHeartBlockEntity.ATTEMPTS_PER_SPAWN`.
const ATTEMPTS_PER_SPAWN: i32 = 5;
/// Vanilla `CreakingHeartBlockEntity.HURT_CALL_TOTAL_TICKS`.
const HURT_CALL_TOTAL_TICKS: i32 = 100;
/// Vanilla `CreakingHeartBlockEntity.HURT_CALL_INTERVAL`.
const HURT_CALL_INTERVAL: i32 = 10;
/// Vanilla `CreakingHeartBlockEntity.HURT_CALL_PARTICLE_TICKS`.
const HURT_CALL_PARTICLE_TICKS: i32 = 50;
/// Vanilla `CreakingHeartBlockEntity.MAX_DEPTH` for the resin search.
const RESIN_MAX_DEPTH: u32 = 2;
/// Vanilla `CreakingHeartBlockEntity.MAX_COUNT` for the resin search.
const RESIN_MAX_COUNT: usize = 64;
/// Vanilla `CreakingHeartBlockEntity.TICKS_GRACE_PERIOD`.
const TICKS_GRACE_PERIOD: i64 = 30;
/// Vanilla `Creaking.CREAKING_ORANGE`, the colour of the trail running from the
/// heart out to the creaking.
const CREAKING_ORANGE: i32 = 16_545_810;
/// Vanilla `Creaking.CREAKING_GRAY`, the trail running back.
const CREAKING_GRAY: i32 = 6_250_335;
/// Vanilla parity: the `20` particles one `creakingHurt` sends at once.
const HURT_PARTICLE_COUNT: i32 = 20;
/// Vanilla parity: the `15` levels a comparator can read.
const MAX_SIGNAL: f64 = 15.0;

struct CreakingHeartTickerState {
    ticker: i32,
    /// Vanilla parity: the `creakingInfo` `Either`, kept as its UUID half.
    creaking: Option<Uuid>,
    /// Vanilla parity: `ticksExisted`, which only gates the load grace period.
    ticks_existed: i64,
    /// Vanilla parity: `emitter`, the hundred-tick hurt animation countdown.
    emitter: i32,
    /// Vanilla parity: `emitterTarget`.
    emitter_target: Option<DVec3>,
    /// Vanilla parity: `outputSignal`, the last value pushed to comparators.
    output_signal: i32,
}

/// Vanilla `CreakingHeartBlockEntity`.
pub struct CreakingHeartBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<CreakingHeartTickerState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CreakingHeartBlockEntity`.
unsafe impl DowncastType for CreakingHeartBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/creaking_heart");
}

impl CreakingHeartBlockEntity {
    /// Creates creaking heart storage.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::CREAKING_HEART,
                world,
                pos,
                state,
            ),
            state: SyncMutex::new(CreakingHeartTickerState {
                ticker: 0,
                creaking: None,
                ticks_existed: 0,
                emitter: 0,
                emitter_target: None,
                output_signal: 0,
            }),
        }
    }

    /// Vanilla `CreakingHeartBlockEntity.setCreakingInfo`.
    pub fn set_creaking_info(&self, creaking: Uuid) {
        {
            let mut state = self.state.lock();
            state.creaking = Some(creaking);
            state.ticks_existed = 0;
        }
        self.set_changed();
    }

    /// Vanilla `CreakingHeartBlockEntity.clearCreakingInfo`.
    fn clear_creaking_info(&self) {
        self.state.lock().creaking = None;
        self.set_changed();
    }

    /// Vanilla `CreakingHeartBlockEntity.getCreakingProtector`.
    ///
    /// A UUID that will not resolve is kept for the first thirty ticks of the
    /// block entity's life and dropped after that, so a heart loaded before its
    /// creaking's chunk does not orphan itself.
    #[must_use]
    fn creaking_protector(&self, world: &Arc<World>) -> Option<SharedEntity> {
        let creaking = self.state.lock().creaking?;
        let resolved = world.get_entity_by_uuid(&creaking).filter(|entity| {
            entity.downcast_ref::<CreakingEntity>().is_some() && !entity.is_removed()
        });
        if resolved.is_some() {
            return resolved;
        }

        if self.state.lock().ticks_existed >= TICKS_GRACE_PERIOD {
            self.clear_creaking_info();
        }
        None
    }

    /// Returns whether `uuid` names the creaking this heart is keeping alive.
    ///
    /// Vanilla `CreakingHeartBlockEntity.isProtector`, which compares object
    /// identity; the UUID is the same test without resolving the entity.
    #[must_use]
    pub fn is_protector(&self, uuid: Uuid) -> bool {
        self.state.lock().creaking == Some(uuid)
    }

    /// Vanilla `CreakingHeartBlockEntity.distanceToCreaking`.
    fn distance_to_creaking(&self, world: &Arc<World>) -> f64 {
        let Some(creaking) = self.creaking_protector(world) else {
            return 0.0;
        };
        let (x, _, z) = self.get_block_pos().get_center();
        let bottom_center = DVec3::new(x, f64::from(self.get_block_pos().y()), z);
        creaking.position().distance(bottom_center)
    }

    /// Vanilla `CreakingHeartBlockEntity.computeAnalogOutputSignal`, which
    /// falls from fifteen at the heart to zero at the edge of the roam radius.
    #[must_use]
    fn compute_analog_output_signal(&self, world: &Arc<World>) -> i32 {
        if self.creaking_protector(world).is_none() {
            return 0;
        }
        let scaled = self
            .distance_to_creaking(world)
            .clamp(0.0, CREAKING_ROAMING_RADIUS)
            / CREAKING_ROAMING_RADIUS;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a comparator level, between zero and fifteen"
        )]
        let steps = (scaled * MAX_SIGNAL).floor() as i32;
        15 - steps
    }

    /// Returns the comparator level a redstone reader sees.
    ///
    /// Vanilla `CreakingHeartBlockEntity.getAnalogOutputSignal`, which reads the
    /// value the tick last computed rather than measuring again.
    #[must_use]
    pub fn analog_output_signal(&self) -> i32 {
        self.state.lock().output_signal
    }

    /// Vanilla `CreakingHeartBlockEntity.spawnProtector`.
    fn spawn_protector(&self, world: &Arc<World>) -> Option<SharedEntity> {
        let pos = self.get_block_pos();
        let spawned = try_spawn_mob(
            &vanilla_entities::CREAKING,
            EntitySpawnReason::Spawner,
            world,
            pos,
            ATTEMPTS_PER_SPAWN,
            SPAWN_RANGE_XZ,
            SPAWN_RANGE_Y,
            SpawnStrategy::OnTopOfColliderNoLeaves,
            true,
        )?;
        as_creaking(&spawned)?;

        world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            spawned.block_position(),
            &GameEventContext::new(Some(spawned.as_entity_event_source()), None),
        );
        spawned.broadcast_entity_event(EntityStatus::Poof);
        if let Some(creaking) = as_creaking(&spawned) {
            creaking.set_transient(pos);
        }
        Some(spawned)
    }

    /// Vanilla `CreakingHeartBlockEntity.creakingHurt`.
    ///
    /// The creaking itself takes no damage; this is where the blow actually
    /// lands. Two or three resin clumps grow out of the tree while the heart is
    /// awake, and the hundred-tick emitter that follows is the trail of
    /// particles and the repeated hurt noise travelling from the heart out to
    /// the creaking.
    pub fn creaking_hurt(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let Some(creaking) = self.creaking_protector(&world) else {
            return;
        };
        if self.state.lock().emitter > 0 {
            return;
        }

        self.emit_particles(&world, HURT_PARTICLE_COUNT, false);
        if self.get_block_state().get_value(CREAKING_HEART_STATE) == CreakingHeartState::Awake {
            let clumps = rand::rng().random_range(2..=3);
            for _ in 0..clumps {
                let Some(placed) = self.spread_resin(&world) else {
                    continue;
                };
                world.play_sound(
                    &sound_events::BLOCK_RESIN_PLACE,
                    SoundSource::Blocks,
                    placed,
                    1.0,
                    1.0,
                    None,
                );
                world.game_event(
                    &vanilla_game_events::BLOCK_PLACE,
                    placed,
                    &GameEventContext::new(None, Some(self.get_block_state())),
                );
            }
        }

        let mut state = self.state.lock();
        state.emitter = HURT_CALL_TOTAL_TICKS;
        state.emitter_target = Some(bounding_box_center(&creaking));
    }

    /// Vanilla `CreakingHeartBlockEntity.spreadResin`.
    ///
    /// A breadth-first walk out along the pale oak logs the heart is buried in,
    /// looking for the first face with room for a resin clump. Depth two and
    /// sixty-four nodes are vanilla's limits, and they are what keeps the resin
    /// on the tree rather than crawling across a forest.
    fn spread_resin(&self, world: &Arc<World>) -> Option<BlockPos> {
        let start = self.get_block_pos();
        let mut visited = vec![start];
        let mut queue = vec![(start, 0_u32)];
        let mut head = 0;

        while head < queue.len() && visited.len() <= RESIN_MAX_COUNT {
            let (pos, depth) = queue[head];
            head += 1;

            if world
                .get_block_state(pos)
                .get_block()
                .has_tag(&BlockTag::PALE_OAK_LOGS)
                && let Some(placed) = try_place_resin_on(world, pos)
            {
                return Some(placed);
            }

            if depth >= RESIN_MAX_DEPTH {
                continue;
            }
            for direction in shuffled_directions() {
                let neighbor = pos.relative(direction);
                if visited.contains(&neighbor) {
                    continue;
                }
                if !world
                    .get_block_state(neighbor)
                    .get_block()
                    .has_tag(&BlockTag::PALE_OAK_LOGS)
                {
                    continue;
                }
                visited.push(neighbor);
                queue.push((neighbor, depth + 1));
            }
        }

        None
    }

    /// Vanilla `CreakingHeartBlockEntity.emitParticles`.
    fn emit_particles(&self, world: &Arc<World>, count: i32, towards_creaking: bool) {
        let Some(creaking) = self.creaking_protector(world) else {
            return;
        };
        let color = if towards_creaking {
            CREAKING_ORANGE
        } else {
            CREAKING_GRAY
        };
        let bounds = creaking.bounding_box();
        let corner = self.get_block_pos();

        for _ in 0..count {
            let on_creaking = DVec3::new(
                bounds
                    .width()
                    .mul_add(rand::random::<f64>(), bounds.min_x()),
                bounds
                    .height()
                    .mul_add(rand::random::<f64>(), bounds.min_y()),
                bounds
                    .depth()
                    .mul_add(rand::random::<f64>(), bounds.min_z()),
            );
            let on_heart = DVec3::new(
                f64::from(corner.x()) + rand::random::<f64>(),
                f64::from(corner.y()) + rand::random::<f64>(),
                f64::from(corner.z()) + rand::random::<f64>(),
            );
            let (source, destination) = if towards_creaking {
                (on_heart, on_creaking)
            } else {
                (on_creaking, on_heart)
            };

            world.send_particles_with_options(
                ParticleData::new(
                    &vanilla_particle_types::TRAIL,
                    TrailParticleOption::new(
                        destination,
                        RgbColor::new(color),
                        rand::random_range(0..40) + 10,
                    ),
                ),
                true,
                true,
                source,
                1,
                DVec3::ZERO,
                0.0,
            );
        }
    }

    /// Vanilla `CreakingHeartBlockEntity.removeProtector`.
    ///
    /// With no damage source the creaking simply crumbles; with one it dies
    /// properly first, so whoever broke the heart is credited with the kill.
    pub fn remove_protector(&self, source: Option<&DamageSource>) {
        let Some(world) = self.get_level() else {
            return;
        };
        let Some(creaking) = self.creaking_protector(&world) else {
            return;
        };

        let Some(creaking) = as_creaking(&creaking) else {
            return;
        };
        match source {
            None => creaking.tear_down(),
            Some(source) => {
                creaking.creaking_death_effects(source);
                creaking.set_tearing_down();
                creaking.set_health(0.0);
            }
        }
        self.clear_creaking_info();
    }

    /// Vanilla `CreakingHeartBlockEntity.updateCreakingState`.
    ///
    /// A heart that has lost its logs only uproots once it has no creaking left
    /// to keep alive -- breaking the tree out from under a woken heart does not
    /// free you of what it has already sent.
    fn updated_state(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
    ) -> BlockStateId {
        if !CreakingHeartBlock::has_required_logs(state, world.as_ref(), pos)
            && self.state.lock().creaking.is_none()
        {
            return state.set_value(CREAKING_HEART_STATE, CreakingHeartState::Uprooted);
        }

        state.set_value(
            CREAKING_HEART_STATE,
            creaking_heart_awake_or_dormant(world.as_ref()),
        )
    }

    /// Vanilla parity: the `emitter` block of `CreakingHeartBlockEntity.serverTick`.
    fn tick_emitter(&self, world: &Arc<World>) {
        let (emitter, emitter_target) = {
            let state = self.state.lock();
            (state.emitter, state.emitter_target)
        };
        if emitter <= 0 {
            return;
        }

        if emitter > HURT_CALL_PARTICLE_TICKS {
            self.emit_particles(world, 1, true);
            self.emit_particles(world, 1, false);
        }

        if emitter % HURT_CALL_INTERVAL == 0
            && let Some(target) = emitter_target
        {
            // Vanilla re-reads the creaking's centre so the noise follows a
            // creaking that has moved since the blow landed.
            let target = self
                .creaking_protector(world)
                .map_or(target, |creaking| bounding_box_center(&creaking));
            self.state.lock().emitter_target = Some(target);

            let (x, y, z) = self.get_block_pos().get_center();
            let heart = DVec3::new(x, y, z);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a countdown below a hundred, used as an interpolation factor"
            )]
            let progress = 0.8f32.mul_add((HURT_CALL_TOTAL_TICKS - emitter) as f32 / 100.0, 0.2);
            let sound_position = (heart - target) * f64::from(progress) + target;
            let sound_pos =
                BlockPos::containing(sound_position.x, sound_position.y, sound_position.z);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a countdown below a hundred, used as a volume"
            )]
            let volume = emitter as f32 / 2.0 / 100.0 + 0.5;
            world.play_sound(
                &sound_events::BLOCK_CREAKING_HEART_HURT,
                SoundSource::Blocks,
                sound_pos,
                volume,
                1.0,
                None,
            );
        }

        self.state.lock().emitter -= 1;
    }

    /// Vanilla parity: the twenty-tick block of `serverTick` that wakes, keeps
    /// or drops the protector.
    fn tick_protector(&self, world: &Arc<World>, pos: BlockPos) {
        let state = self.get_block_state();
        let updated = self.updated_state(state, world, pos);
        if updated != state {
            world.set_block(pos, updated, UpdateFlags::UPDATE_ALL);
            if updated.get_value(CREAKING_HEART_STATE) == CreakingHeartState::Uprooted {
                return;
            }
        }

        if self.state.lock().creaking.is_none() {
            if updated.get_value(CREAKING_HEART_STATE) != CreakingHeartState::Awake {
                return;
            }
            if !world.get_game_rule(&SPAWN_MONSTERS) || world.difficulty() == Difficulty::Peaceful {
                return;
            }
            let (x, y, z) = pos.get_center();
            // Vanilla's `getNearestPlayer(x, y, z, range, false)` takes any
            // player that is not a spectator; the `false` is `isCreative`
            // being allowed through.
            if world
                .nearest_player(DVec3::new(x, y, z), PLAYER_DETECTION_RANGE, |player| {
                    !player.is_spectator()
                })
                .is_none()
            {
                return;
            }
            let Some(creaking) = self.spawn_protector(world) else {
                return;
            };
            self.set_creaking_info(creaking.uuid());
            if let Some(creaking) = as_creaking(&creaking) {
                creaking.make_sound(Some(&sound_events::ENTITY_CREAKING_SPAWN));
            }
            world.play_sound(
                &sound_events::BLOCK_CREAKING_HEART_SPAWN,
                SoundSource::Blocks,
                pos,
                1.0,
                1.0,
                None,
            );
            return;
        }

        let Some(creaking) = self.creaking_protector(world) else {
            return;
        };
        let Some(creaking) = as_creaking(&creaking) else {
            return;
        };
        let night_is_over = !world.creaking_active() && !creaking.is_persistence_required();
        if night_is_over
            || self.distance_to_creaking(world) > DISTANCE_CREAKING_TOO_FAR
            || creaking.player_is_stuck_in_you()
        {
            self.remove_protector(None);
        }
    }
}

/// Vanilla parity: `AABB.getCenter`.
fn bounding_box_center(creaking: &SharedEntity) -> DVec3 {
    creaking.bounding_box().center()
}

/// Recovers the concrete creaking a `SharedEntity` is known to hold.
fn as_creaking(entity: &SharedEntity) -> Option<&CreakingEntity> {
    entity.downcast_ref::<CreakingEntity>()
}

/// Vanilla parity: `Util.shuffledCopy(Direction.values(), random)`.
fn shuffled_directions() -> [Direction; 6] {
    let mut directions = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];
    for index in (1..directions.len()).rev() {
        directions.swap(index, rand::random_range(0..=index));
    }
    directions
}

/// Vanilla parity: the inner loop of `spreadResin`, which grows a clump on the
/// first free face of the log at `pos`.
fn try_place_resin_on(world: &Arc<World>, pos: BlockPos) -> Option<BlockPos> {
    for direction in shuffled_directions() {
        let neighbor_pos = pos.relative(direction);
        let neighbor_state = world.get_block_state(neighbor_pos);
        let opposite = direction.opposite();

        let base = if neighbor_state.is_air() {
            vanilla_blocks::RESIN_CLUMP.default_state()
        } else if neighbor_state.get_block() == &vanilla_blocks::WATER
            && neighbor_state.get_fluid_state().is_source()
        {
            vanilla_blocks::RESIN_CLUMP
                .default_state()
                .set_value(&BlockStateProperties::WATERLOGGED, true)
        } else {
            neighbor_state
        };

        if base.get_block() != &vanilla_blocks::RESIN_CLUMP {
            continue;
        }
        let face = multiface_face_property(opposite);
        if base.get_value(face) {
            continue;
        }

        world.set_block(
            neighbor_pos,
            base.set_value(face, true),
            UpdateFlags::UPDATE_ALL,
        );
        return Some(neighbor_pos);
    }
    None
}

impl BlockEntity for CreakingHeartBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `CreakingHeartBlockEntity.serverTick`.
    fn tick(&self, world: &Arc<World>) {
        self.state.lock().ticks_existed += 1;

        let pos = self.get_block_pos();
        let computed = self.compute_analog_output_signal(world);
        let changed = {
            let mut state = self.state.lock();
            let changed = state.output_signal != computed;
            state.output_signal = computed;
            changed
        };
        if changed {
            world.update_neighbor_for_output_signal(pos, &vanilla_blocks::CREAKING_HEART);
        }

        self.tick_emitter(world);

        {
            let mut state = self.state.lock();
            state.ticker -= 1;
            if state.ticker >= 0 {
                return;
            }
            state.ticker = rand::rng().random_range(0..UPDATE_TICKS_VARIANCE) + UPDATE_TICKS;
        }

        self.tick_protector(world, pos);
    }

    /// Vanilla `CreakingHeartBlockEntity.preRemoveSideEffects`, which is what
    /// makes breaking the heart the way to be rid of the creaking.
    fn pre_remove_side_effects(&self, _pos: BlockPos, _state: BlockStateId) {
        self.remove_protector(None);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let creaking = view
            .int_array("creaking")
            .and_then(|array| Uuid::from_int_array(&array));
        match creaking {
            Some(creaking) => self.set_creaking_info(creaking),
            None => self.clear_creaking_info(),
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(creaking) = self.state.lock().creaking {
            nbt.insert(
                "creaking",
                NbtTag::IntArray(creaking.to_int_array().to_vec()),
            );
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        Some(self.save_custom_only())
    }
}
