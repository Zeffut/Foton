//! The allay's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.allay.AllayAi`. Two
//! activities and no fight set at all: an allay never attacks anything. What it
//! does instead is find the item it was handed a copy of, and carry it back to
//! whoever handed it over -- or to the note block it last heard, which is what
//! turns a pair of allays into a sorting machine.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::item_stack::ItemStack;
use foton_registry::{sound_events, vanilla_blocks};
use foton_utils::value_providers::UniformIntProvider;
use foton_utils::{BlockPos, GlobalPos, Identifier};

use crate::entity::ai::brain::behavior::utils::block_closer_than;
use crate::entity::ai::brain::behavior::{
    AnimalPanic, Behavior, CountDownCooldownTicks, DoNothing, GoAndGiveItemsToTarget,
    GoToWantedItem, LookAtTargetSink, MoveToTargetSink, OneShot, RandomStroll, RunOne,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, StayCloseToTarget, Swim,
};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::{Entity as _, PathfinderMob, SharedEntity};
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `AllayAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 1.0;
/// Vanilla parity: `AllayAi.SPEED_MULTIPLIER_WHEN_FOLLOWING_DEPOSIT_TARGET`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_DEPOSIT_TARGET: f64 = 2.25;
/// Vanilla parity: `AllayAi.SPEED_MULTIPLIER_WHEN_RETRIEVING_ITEM`.
const SPEED_MULTIPLIER_WHEN_RETRIEVING_ITEM: f64 = 1.75;
/// Vanilla parity: `AllayAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 2.5;
/// Vanilla parity: the `0.8F` of the core activity's `Swim`.
const SWIM_CHANCE: f32 = 0.8;

/// Vanilla parity: `AllayAi.CLOSE_ENOUGH_TO_TARGET`.
const CLOSE_ENOUGH_TO_TARGET: i32 = 4;
/// Vanilla parity: `AllayAi.TOO_FAR_FROM_TARGET`.
const TOO_FAR_FROM_TARGET: i32 = 16;
/// Vanilla parity: `AllayAi.MAX_LOOK_DISTANCE`.
const MAX_LOOK_DISTANCE: f64 = 6.0;
/// Vanilla parity: `AllayAi.MIN_WAIT_DURATION` and `MAX_WAIT_DURATION`.
const WAIT_DURATION: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};
/// Vanilla parity: `AllayAi.TIME_TO_FORGET_NOTEBLOCK`.
pub const TIME_TO_FORGET_NOTEBLOCK: i32 = 600;
/// Vanilla parity: `AllayAi.DISTANCE_TO_WANTED_ITEM`.
const DISTANCE_TO_WANTED_ITEM: i32 = 32;
/// Vanilla parity: `AllayAi.GIVE_ITEM_TIMEOUT_DURATION`.
const GIVE_ITEM_TIMEOUT_DURATION: i32 = 20;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(1.0F, 3)` of the
/// idle gate.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;
/// Vanilla parity: `Allay.MAX_NOTEBLOCK_DISTANCE`.
pub const MAX_NOTEBLOCK_DISTANCE: i32 = 1024;

/// How far a liked player may wander before an allay gives up following.
///
/// Vanilla parity: the `closerThan(allay, 64.0)` of `AllayAi.getLikedPlayer`.
const LIKED_PLAYER_MAX_DISTANCE: f64 = 64.0;

/// Vanilla parity: the sensor list of `Allay.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::HurtBy,
    SensorType::NearestItems,
];

/// The three memories no sensor writes, which the brain must still register.
///
/// Vanilla parity: the `memoryTypes` argument of `Allay.BRAIN_PROVIDER`.
const MEMORIES: &[MemoryModuleId] = &[
    memory_module_types::LIKED_PLAYER.id(),
    memory_module_types::LIKED_NOTEBLOCK_POSITION.id(),
    memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS.id(),
];

/// Vanilla parity: `Allay.BRAIN_PROVIDER` plus `AllayAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new_with_memories(SENSORS, MEMORIES, vec![core_activity(), idle_activity()])
}

/// Vanilla parity: `AllayAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(Swim::new(SWIM_CHANCE)),
            Behavior::boxed(AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING)),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            Behavior::boxed(MoveToTargetSink::new()),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::ITEM_PICKUP_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `AllayAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::Idle,
        0,
        vec![
            OneShot::boxed(GoToWantedItem::new(
                SPEED_MULTIPLIER_WHEN_RETRIEVING_ITEM,
                true,
                DISTANCE_TO_WANTED_ITEM,
            )),
            Behavior::boxed(GoAndGiveItemsToTarget::new(
                item_deposit_position,
                SPEED_MULTIPLIER_WHEN_FOLLOWING_DEPOSIT_TARGET,
                GIVE_ITEM_TIMEOUT_DURATION,
                on_item_thrown,
            )),
            OneShot::boxed(StayCloseToTarget::new(
                item_deposit_position,
                |body| !has_wanted_item(body),
                CLOSE_ENOUGH_TO_TARGET,
                TOO_FAR_FROM_TARGET,
                SPEED_MULTIPLIER_WHEN_FOLLOWING_DEPOSIT_TARGET,
            )),
            OneShot::boxed(SetEntityLookTargetSometimes::any_within(
                MAX_LOOK_DISTANCE,
                WAIT_DURATION,
            )),
            Box::new(RunOne::unconditional(vec![
                (
                    OneShot::boxed(RandomStroll::fly(SPEED_MULTIPLIER_WHEN_IDLING)),
                    2,
                ),
                (
                    OneShot::boxed(SetWalkTargetFromLookTarget::new(
                        SPEED_MULTIPLIER_WHEN_IDLING,
                        WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                    )),
                    2,
                ),
                (
                    Box::new(DoNothing::new(
                        WAIT_DURATION.min_inclusive,
                        WAIT_DURATION.max_inclusive,
                    )),
                    1,
                ),
            ])),
        ],
    )
}

/// Vanilla parity: `AllayAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Idle]);
}

/// Remembers the note block the allay just heard.
///
/// Vanilla parity: `AllayAi.hearNoteblock`. A second note block does not steal
/// the allay: only the one it already likes refreshes the ten-second clock, and
/// the clock is what stops an allay serving a note block nobody plays any more.
pub fn hear_noteblock(brain: &Brain, world: &Arc<World>, pos: BlockPos) {
    let global_pos = GlobalPos::new(world.key.clone(), pos);
    let liked = brain.get_memory(memory_module_types::LIKED_NOTEBLOCK_POSITION);

    match liked {
        None => {
            brain.set_memory(
                memory_module_types::LIKED_NOTEBLOCK_POSITION,
                global_pos.clone(),
            );
            brain.set_memory(
                memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS,
                TIME_TO_FORGET_NOTEBLOCK,
            );
        }
        Some(existing) if existing == global_pos => {
            brain.set_memory(
                memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS,
                TIME_TO_FORGET_NOTEBLOCK,
            );
        }
        Some(_) => {}
    }
}

/// Vanilla parity: `AllayAi.hasWantedItem`.
fn has_wanted_item(body: &dyn PathfinderMob) -> bool {
    body.brain().is_some_and(|brain| {
        brain.has_memory_value(memory_module_types::NEAREST_VISIBLE_WANTED_ITEM.id())
    })
}

/// Where the allay should take what it is carrying.
///
/// Vanilla parity: `AllayAi.getItemDepositPosition`. The note block wins over
/// the player when it is still valid, and an invalid one is forgotten on the
/// spot so the allay goes back to its owner rather than to a hole in the ground.
fn item_deposit_position(body: &dyn PathfinderMob) -> Option<PositionTracker> {
    let brain = body.brain()?;
    if let Some(liked_noteblock) = brain.get_memory(memory_module_types::LIKED_NOTEBLOCK_POSITION) {
        if should_deposit_items_at_liked_noteblock(body, brain, &liked_noteblock) {
            return Some(PositionTracker::of_block(liked_noteblock.pos.above()));
        }
        brain.erase_memory(memory_module_types::LIKED_NOTEBLOCK_POSITION.id());
    }

    liked_player_position_tracker(body)
}

/// Vanilla parity: `AllayAi.shouldDepositItemsAtLikedNoteblock`.
fn should_deposit_items_at_liked_noteblock(
    body: &dyn PathfinderMob,
    brain: &Brain,
    liked_noteblock: &GlobalPos,
) -> bool {
    let Some(world) = body.level() else {
        return false;
    };
    is_close_enough(liked_noteblock, &world.key, body.block_position())
        && world.get_block_state(liked_noteblock.pos).get_block() == &vanilla_blocks::NOTE_BLOCK
        && brain.has_memory_value(memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS.id())
}

/// Vanilla parity: `GlobalPos.isCloseEnough`.
fn is_close_enough(global_pos: &GlobalPos, dimension: &Identifier, pos: BlockPos) -> bool {
    &global_pos.dimension == dimension
        && block_closer_than(global_pos.pos, pos, f64::from(MAX_NOTEBLOCK_DISTANCE))
}

/// Vanilla parity: `AllayAi.getLikedPlayerPositionTracker`.
fn liked_player_position_tracker(body: &dyn PathfinderMob) -> Option<PositionTracker> {
    let player = liked_player(body)?;
    Some(PositionTracker::of_entity(&player, true))
}

/// Returns the player this allay is fetching for, when they are still around.
///
/// Vanilla parity: `AllayAi.getLikedPlayer`, which drops a player who has left,
/// gone into spectator, or wandered more than sixty-four blocks off.
pub fn liked_player(body: &dyn PathfinderMob) -> Option<SharedEntity> {
    let brain = body.brain()?;
    let world = body.level()?;
    let liked = brain.get_memory(memory_module_types::LIKED_PLAYER)?;
    let entity = world.get_entity_by_uuid(&liked)?;
    let player = entity.as_player()?;
    if player.is_spectator() {
        return None;
    }
    if entity.position().distance(body.position()) >= LIKED_PLAYER_MAX_DISTANCE {
        return None;
    }
    Some(entity)
}

/// Vanilla parity: `AllayAi.onItemThrown`, the throw sound with its sixteen
/// tuned pitches.
///
/// TODO: vanilla also fires the `ALLAY_DROP_ITEM_ON_BLOCK` criterion; Foton has
/// no advancement triggers.
fn on_item_thrown(ctx: &BrainContext<'_>, _item: &ItemStack, _target_pos: BlockPos) {
    if ctx.game_time() % THROW_SOUND_INTERVAL != 0 || rand::random::<f64>() >= THROW_SOUND_CHANCE {
        return;
    }
    let pitch = THROW_SOUND_PITCHES[rand::random_range(0..THROW_SOUND_PITCHES.len())];
    ctx.mob()
        .play_sound(&sound_events::ENTITY_ALLAY_ITEM_THROWN, 1.0, pitch);
}

/// Vanilla parity: the `level.getGameTime() % 7L == 0L` of `onItemThrown`.
const THROW_SOUND_INTERVAL: i64 = 7;
/// Vanilla parity: the `nextDouble() < 0.9` of `onItemThrown`.
const THROW_SOUND_CHANCE: f64 = 0.9;

/// Vanilla parity: `Allay.THROW_SOUND_PITCHES`, which is a musical scale rather
/// than a random spread -- an allay sorting a chest plays a tune.
const THROW_SOUND_PITCHES: [f32; 16] = [
    0.5625, 0.625, 0.75, 0.9375, 1.0, 1.0, 1.125, 1.25, 1.5, 1.875, 2.0, 2.25, 2.5, 3.0, 3.75, 4.0,
];
