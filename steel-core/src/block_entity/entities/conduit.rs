//! Conduit block entity implementation.
//!
//! A conduit is inert until it is sealed inside a 3x3x3 pocket of water and
//! ringed by prismarine. It then hands Conduit Power to swimmers nearby, and
//! once the frame is complete it picks a hostile mob out of the water every two
//! seconds and hits it.
//!
//! Everything the player sees spinning and sparking is drawn client-side:
//! vanilla's `animationTick` and `getActiveRotation` never run on a server, and
//! the client re-derives whether a conduit is active by running `updateShape`
//! itself. The one thing the server has to tell it is which mob is being
//! attacked, so the beam has somewhere to point -- which is why `Target` is the
//! whole of this entity's saved and synced state.

use std::sync::{Arc, Weak};

use rand::RngExt as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{
    sound_events, vanilla_block_entity_types, vanilla_blocks, vanilla_damage_types,
    vanilla_mob_effects,
};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, UuidExt as _, WorldAabb};
use uuid::Uuid;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::damage::DamageSource;
use crate::entity::{Entity as _, LivingEntity, MobEffectInstance, SharedEntity};
use crate::fluid::{FluidStateExt as _, get_fluid_state};
use crate::world::{LevelReader as _, World};

/// Frame blocks needed before a conduit switches on.
///
/// Vanilla parity: `ConduitBlockEntity.MIN_ACTIVE_SIZE`.
const MIN_ACTIVE_SIZE: usize = 16;

/// Frame blocks needed before a conduit starts hitting mobs.
///
/// Vanilla parity: `ConduitBlockEntity.MIN_KILL_SIZE`.
const MIN_KILL_SIZE: usize = 42;

/// How far a conduit looks for something to hit.
///
/// Vanilla parity: `ConduitBlockEntity.KILL_RANGE`.
const KILL_RANGE: f64 = 8.0;

/// Vanilla parity: the `260` of `ConduitBlockEntity.applyEffects`, which is
/// longer than the two seconds between applications so the effect never lapses
/// while a player stays in range.
const EFFECT_DURATION_TICKS: i32 = 260;

/// Vanilla parity: the `4.0F` of `updateAndAttackTarget`.
const ATTACK_DAMAGE: f32 = 4.0;

/// How often the whole conduit beat runs.
///
/// Vanilla parity: the `gameTime % 40` of `ConduitBlockEntity.serverTick`.
const BEAT_INTERVAL_TICKS: i64 = 40;

/// Vanilla parity: the `gameTime % 80` ambient sound.
const AMBIENT_INTERVAL_TICKS: i64 = 80;

const TARGET_NBT_KEY: &str = "Target";

/// Vanilla parity: `ConduitBlockEntity.VALID_BLOCKS`, a hardcoded array rather
/// than a block tag.
fn is_frame_block(block: BlockRef) -> bool {
    block == &vanilla_blocks::PRISMARINE
        || block == &vanilla_blocks::PRISMARINE_BRICKS
        || block == &vanilla_blocks::SEA_LANTERN
        || block == &vanilla_blocks::DARK_PRISMARINE
}

/// Vanilla parity: `Vec3i.closerThan`, which compares raw block coordinates.
fn closer_than(pos: BlockPos, other: BlockPos, distance: f64) -> bool {
    let dx = f64::from(pos.x() - other.x());
    let dy = f64::from(pos.y() - other.y());
    let dz = f64::from(pos.z() - other.z());
    dz.mul_add(dz, dx.mul_add(dx, dy * dy)) < distance * distance
}

/// Everything the conduit works out again on each beat, plus the one thing it
/// remembers between them.
struct ConduitState {
    /// Vanilla `isActive`. Kept only to notice the transition that plays the
    /// activate and deactivate sounds.
    active: bool,
    /// How many frame blocks the last beat found. Vanilla keeps the positions
    /// themselves, but only the client's particles ever read them back.
    frame_blocks: usize,
    /// Vanilla `destroyTarget`, an `EntityReference<LivingEntity>`. Steel keeps
    /// the UUID alone and resolves it against the world, the way
    /// `CreakingHeartBlockEntity` does.
    destroy_target: Option<Uuid>,
    /// Vanilla `nextAmbientSoundActivation`, which is deliberately not saved.
    next_ambient_sound: i64,
}

/// A conduit's activation, effect and attack state.
pub struct ConduitBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<ConduitState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ConduitBlockEntity`.
unsafe impl DowncastType for ConduitBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/conduit");
}

impl ConduitBlockEntity {
    /// Creates a conduit block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::CONDUIT, level, pos, state),
            state: SyncMutex::new(ConduitState {
                active: false,
                frame_blocks: 0,
                destroy_target: None,
                next_ambient_sound: 0,
            }),
        }
    }

    /// Returns whether the last beat found a complete pocket and enough frame.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.state.lock().active
    }

    /// Returns how many frame blocks the last beat counted.
    #[must_use]
    pub fn frame_block_count(&self) -> usize {
        self.state.lock().frame_blocks
    }

    /// Returns the mob this conduit is currently hitting.
    #[must_use]
    pub fn destroy_target(&self) -> Option<Uuid> {
        self.state.lock().destroy_target
    }

    fn play(&self, world: &Arc<World>, sound: SoundEventRef) {
        world.play_sound(sound, SoundSource::Blocks, self.base.pos(), 1.0, 1.0, None);
    }

    /// Runs one two-second beat.
    fn beat(&self, world: &Arc<World>, pos: BlockPos) {
        let frame_blocks = count_frame_blocks(world, pos);
        let active = frame_blocks.len() >= MIN_ACTIVE_SIZE;

        let was_active = {
            let mut state = self.state.lock();
            let was_active = state.active;
            state.active = active;
            state.frame_blocks = frame_blocks.len();
            was_active
        };

        if active != was_active {
            self.play(
                world,
                if active {
                    &sound_events::BLOCK_CONDUIT_ACTIVATE
                } else {
                    &sound_events::BLOCK_CONDUIT_DEACTIVATE
                },
            );
        }

        if !active {
            return;
        }
        apply_effects(world, pos, frame_blocks.len());
        self.update_and_attack_target(world, pos, frame_blocks.len() >= MIN_KILL_SIZE);
    }

    /// Vanilla parity: `ConduitBlockEntity.updateAndAttackTarget`.
    fn update_and_attack_target(&self, world: &Arc<World>, pos: BlockPos, hunting: bool) {
        let previous = self.state.lock().destroy_target;
        let (target, resolved) = update_destroy_target(world, pos, previous, hunting);

        if let Some(entity) = resolved
            && let Some(living) = entity.as_living_entity()
        {
            world.play_sound_at(
                &sound_events::BLOCK_CONDUIT_ATTACK_TARGET,
                SoundSource::Blocks,
                entity.position(),
                1.0,
                1.0,
                None,
            );
            living.hurt_server(
                world,
                &DamageSource::environment(&vanilla_damage_types::MAGIC),
                ATTACK_DAMAGE,
            );
        }

        if target == previous {
            return;
        }
        self.state.lock().destroy_target = target;
        // Vanilla's `sendBlockUpdated(pos, state, state, 2)`: clients only, no
        // neighbour update. The client needs the new target to know where to
        // draw the beam, and nothing else about a conduit crosses the wire.
        //
        // Deliberately no `set_changed`: vanilla does not mark the chunk dirty
        // here either. The target is worked out again on the next beat, so a
        // save that misses it costs nothing, and dirtying a chunk every two
        // seconds for a cosmetic pointer would be a real cost.
        world.broadcast_block_entity_if_needed(pos);
    }
}

/// Vanilla parity: `ConduitBlockEntity.updateShape`.
///
/// Returns the frame blocks found, which is empty when the 3x3x3 pocket around
/// the conduit is not all water -- vanilla returns early there and leaves its
/// list cleared, which is what keeps a drained conduit from counting as hunting.
fn count_frame_blocks(world: &Arc<World>, pos: BlockPos) -> Vec<BlockPos> {
    for ox in -1..=1 {
        for oy in -1..=1 {
            for oz in -1..=1 {
                if !is_water_at(world, pos.offset(ox, oy, oz)) {
                    return Vec::new();
                }
            }
        }
    }

    let mut frame = Vec::new();
    for ox in -2..=2_i32 {
        for oy in -2..=2_i32 {
            for oz in -2..=2_i32 {
                if !is_on_frame(ox, oy, oz) {
                    continue;
                }
                let test = pos.offset(ox, oy, oz);
                if is_frame_block(world.get_block_state(test).get_block()) {
                    frame.push(test);
                }
            }
        }
    }
    frame
}

/// Whether an offset lies on one of the three axis-aligned rings of the frame.
///
/// Vanilla parity: the offset test inside `updateShape`'s second loop.
const fn is_on_frame(ox: i32, oy: i32, oz: i32) -> bool {
    let (ax, ay, az) = (ox.abs(), oy.abs(), oz.abs());
    (ax > 1 || ay > 1 || az > 1)
        && (ox == 0 && (ay == 2 || az == 2)
            || oy == 0 && (ax == 2 || az == 2)
            || oz == 0 && (ax == 2 || ay == 2))
}

/// Vanilla parity: `Level.isWaterAt`, which asks only whether the fluid is
/// water -- flowing water counts, and a source is not required.
fn is_water_at(world: &Arc<World>, pos: BlockPos) -> bool {
    get_fluid_state(world, pos).is_water()
}

/// Vanilla parity: `ConduitBlockEntity.applyEffects`.
///
/// Deviation: vanilla collects players from an inflated box and then tests each
/// one with `closerThan`. Steel walks the world's player list instead, the way
/// `BeaconBlockEntity` does, and applies only the `closerThan` test. The box is
/// a superset of that sphere, so the two agree on every player.
fn apply_effects(world: &Arc<World>, pos: BlockPos, frame_blocks: usize) {
    let range = (frame_blocks / 7 * 16) as f64;

    let mut in_range = Vec::new();
    world.players.iter_players(|_, player| {
        if closer_than(pos, player.block_position(), range) && player.is_in_water_or_rain() {
            in_range.push(Arc::clone(player));
        }
        true
    });

    for player in &in_range {
        player.add_mob_effect(
            MobEffectInstance::with_duration(
                vanilla_mob_effects::CONDUIT_POWER,
                EFFECT_DURATION_TICKS,
                0,
            )
            .with_ambient(true),
        );
    }
}

/// Vanilla parity: `ConduitBlockEntity.updateDestroyTarget`.
///
/// Returns the target to keep and the entity it resolves to, if any. Vanilla
/// drops a target that has died or swum away and does not pick a replacement
/// until the next beat, which this preserves.
fn update_destroy_target(
    world: &Arc<World>,
    pos: BlockPos,
    current: Option<Uuid>,
    hunting: bool,
) -> (Option<Uuid>, Option<SharedEntity>) {
    if !hunting {
        return (None, None);
    }

    let Some(current) = current else {
        let selected = select_new_target(world, pos);
        let uuid = selected.as_ref().map(|entity| entity.uuid());
        return (uuid, selected);
    };

    let resolved = world
        .get_entity_by_uuid(&current)
        .filter(|entity| !entity.is_removed());
    let keep = resolved.as_ref().is_some_and(|entity| {
        entity
            .as_living_entity()
            .is_some_and(LivingEntity::is_alive)
            && closer_than(pos, entity.block_position(), KILL_RANGE)
    });
    if keep {
        (Some(current), resolved)
    } else {
        (None, None)
    }
}

/// Vanilla parity: `ConduitBlockEntity.selectNewTarget`, which picks uniformly
/// at random rather than taking the nearest.
fn select_new_target(world: &Arc<World>, pos: BlockPos) -> Option<SharedEntity> {
    let (x, y, z) = (f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
    let aabb = WorldAabb::new(x, y, z, x + 1.0, y + 1.0, z + 1.0).inflate(KILL_RANGE);
    let candidates = world.get_entities_in_aabb_matching(&aabb, |entity| {
        entity.is_enemy() && entity.is_in_water_or_rain() && entity.as_living_entity().is_some()
    });
    if candidates.is_empty() {
        return None;
    }
    let index = rand::rng().random_range(0..candidates.len());
    candidates.get(index).cloned()
}

impl BlockEntity for ConduitBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: `ConduitBlockEntity.serverTick`.
    fn tick(&self, world: &Arc<World>) {
        let game_time = world.game_time();
        let pos = self.get_block_pos();

        if game_time.rem_euclid(BEAT_INTERVAL_TICKS) == 0 {
            self.beat(world, pos);
        }

        if !self.is_active() {
            return;
        }

        if game_time.rem_euclid(AMBIENT_INTERVAL_TICKS) == 0 {
            self.play(world, &sound_events::BLOCK_CONDUIT_AMBIENT);
        }

        let due = {
            let mut state = self.state.lock();
            let due = game_time > state.next_ambient_sound;
            if due {
                state.next_ambient_sound =
                    game_time + 60 + i64::from(rand::rng().random_range(0..40u32));
            }
            due
        };
        if due {
            self.play(world, &sound_events::BLOCK_CONDUIT_AMBIENT_SHORT);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        self.state.lock().destroy_target = view
            .int_array(TARGET_NBT_KEY)
            .and_then(|array| Uuid::from_int_array(&array));
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let Some(target) = self.state.lock().destroy_target else {
            return;
        };
        nbt.insert(
            TARGET_NBT_KEY,
            NbtTag::IntArray(target.to_int_array().to_vec()),
        );
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        Some(self.save_custom_only())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame is three flat 5x5 rings, one per axis plane, overlapping in
    /// pairs. Sixteen blocks each, six shared between two rings, so 42 -- which
    /// is exactly `MIN_KILL_SIZE`, and the reason that number is what it is.
    /// If the offset test ever drifted, this count would move off 42.
    #[test]
    fn a_full_frame_is_forty_two_blocks_on_three_rings() {
        let mut count = 0;
        let mut planes = [0; 3];
        for ox in -2..=2 {
            for oy in -2..=2 {
                for oz in -2..=2 {
                    if !is_on_frame(ox, oy, oz) {
                        continue;
                    }
                    count += 1;
                    // Every accepted offset lies in at least one axis plane.
                    assert!(
                        ox == 0 || oy == 0 || oz == 0,
                        "({ox},{oy},{oz}) is on no axis plane"
                    );
                    planes[0] += i32::from(ox == 0);
                    planes[1] += i32::from(oy == 0);
                    planes[2] += i32::from(oz == 0);
                }
            }
        }
        assert_eq!(
            count, MIN_KILL_SIZE,
            "a full frame is exactly the kill threshold"
        );
        assert_eq!(planes, [16, 16, 16], "each ring is a 5x5 square minus 3x3");

        // A corner of the 5x5x5 cube lies in no axis plane.
        assert!(!is_on_frame(2, 2, 2));
        // Nor does anything else off all three planes.
        assert!(!is_on_frame(2, 2, 1));
        assert!(!is_on_frame(2, 1, 1));
        // Two rings share the middle of each cube edge that touches a plane.
        assert!(is_on_frame(2, 0, 0));
        assert!(is_on_frame(2, 2, 0));
        assert!(is_on_frame(0, 2, 1));
        // Nothing inside the 3x3x3 water pocket counts.
        assert!(!is_on_frame(1, 1, 1));
        assert!(!is_on_frame(0, 0, 0));
        assert!(!is_on_frame(0, 1, 1));
    }

    /// The effect range is a step function of the frame, not a smooth one:
    /// vanilla divides by seven with integer division before multiplying.
    #[test]
    fn the_effect_range_steps_with_every_seventh_block() {
        let range = |blocks: usize| blocks / 7 * 16;
        assert_eq!(range(MIN_ACTIVE_SIZE), 32);
        assert_eq!(range(20), 32, "four spare blocks buy nothing");
        assert_eq!(range(21), 48);
        assert_eq!(range(MIN_KILL_SIZE), 96);
    }

    /// `closerThan` compares raw block coordinates and is strict, so a mob
    /// exactly `KILL_RANGE` away in one axis is out of reach.
    #[test]
    fn the_kill_range_is_a_strict_sphere_on_block_coordinates() {
        let conduit = BlockPos::new(0, 64, 0);
        assert!(closer_than(conduit, BlockPos::new(7, 64, 0), KILL_RANGE));
        assert!(!closer_than(conduit, BlockPos::new(8, 64, 0), KILL_RANGE));
        // Diagonals are measured, not clamped to a box.
        assert!(!closer_than(conduit, BlockPos::new(6, 64, 6), KILL_RANGE));
        assert!(closer_than(conduit, BlockPos::new(0, 71, 0), KILL_RANGE));
    }
}
