//! The goat's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.goat.GoatAi`. Four
//! activities: the shared core, an idle set, the long jump, and the ram.
//!
//! A goat is the one vanilla animal with no `registerGoals` at all -- every
//! thing it does lives here, which is why Foton's goat stood still until this
//! landed.

use foton_registry::{sound_events, vanilla_entities};
use foton_utils::value_providers::UniformIntProvider;

use super::GoatEntity;
use super::ram::{PrepareRamNearestTarget, RamTarget};
use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, AnimalPanic, BabyFollowAdult, Behavior, BehaviorControl,
    CountDownCooldownTicks, DoNothing, FollowTemptation, LongJumpMidJump, LongJumpToRandomPos,
    LookAtTargetSink, MoveToTargetSink, OneShot, RandomStroll, RunOne,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, Swim,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};

/// Vanilla parity: `GoatAi.ADULT_FOLLOW_RANGE`.
const ADULT_FOLLOW_RANGE: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5,
    max_inclusive: 16,
};

/// Vanilla parity: `GoatAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 1.0;

/// Vanilla parity: `GoatAi.SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f64 = 1.25;

/// Vanilla parity: `GoatAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 1.25;

/// Vanilla parity: `GoatAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 2.0;

/// Vanilla parity: `GoatAi.SPEED_MULTIPLIER_WHEN_PREPARING_TO_RAM`.
const SPEED_MULTIPLIER_WHEN_PREPARING_TO_RAM: f64 = 1.25;

/// Vanilla parity: `GoatAi.TIME_BETWEEN_LONG_JUMPS`.
const TIME_BETWEEN_LONG_JUMPS: UniformIntProvider = UniformIntProvider {
    min_inclusive: 600,
    max_inclusive: 1200,
};

/// Vanilla parity: `GoatAi.MAX_LONG_JUMP_HEIGHT` and `MAX_LONG_JUMP_WIDTH`.
const MAX_LONG_JUMP_HEIGHT: i32 = 5;
const MAX_LONG_JUMP_WIDTH: i32 = 5;

/// Vanilla parity: `GoatAi.MAX_JUMP_VELOCITY`.
const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;

/// Vanilla parity: `GoatAi.TIME_BETWEEN_RAMS`.
/// Vanilla parity: the low end of `GoatAi.TIME_BETWEEN_RAMS`.
pub const TIME_BETWEEN_RAMS_MIN: i32 = 600;

const TIME_BETWEEN_RAMS: UniformIntProvider = UniformIntProvider {
    min_inclusive: TIME_BETWEEN_RAMS_MIN,
    max_inclusive: 6000,
};

/// Vanilla parity: `GoatAi.TIME_BETWEEN_RAMS_SCREAMER`.
const TIME_BETWEEN_RAMS_SCREAMER: UniformIntProvider = UniformIntProvider {
    min_inclusive: 100,
    max_inclusive: 300,
};

/// Vanilla parity: the `4`/`7` ram distances of `PrepareRamNearestTarget`.
const MIN_RAM_DISTANCE: i32 = 4;
const MAX_RAM_DISTANCE: i32 = 7;

/// Vanilla parity: the `20` ticks a goat stands still before it charges.
const RAM_PREPARE_TIME: i64 = 20;

/// Vanilla parity: the `6.0F` and `UniformInt.of(30, 60)` of the idle gaze.
const GAZE_RANGE: f64 = 6.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};

/// Vanilla parity: the `new DoNothing(30, 60)` of the idle `RunOne`.
const IDLE_DO_NOTHING_MIN: i32 = 30;
const IDLE_DO_NOTHING_MAX: i32 = 60;

/// Vanilla parity: the `3` close-enough distance of `SetWalkTargetFromLookTarget`.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;

/// Vanilla parity: the `0.8F` of `new Swim<>(0.8F)`.
const SWIM_CHANCE: f32 = 0.8;

/// Vanilla parity: the `45`/`90` of `new LookAtTargetSink(45, 90)`.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;

/// Vanilla parity: `Goat.BRAIN_PROVIDER`'s sensor list.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::NearestItems,
    SensorType::NearestAdult,
    SensorType::HurtBy,
    SensorType::FoodTemptations,
];

/// Vanilla parity: `Goat.BRAIN_PROVIDER` plus `GoatAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![
            core_activity(),
            idle_activity(),
            long_jump_activity(),
            ram_activity(),
        ],
    )
}

/// Vanilla parity: `GoatAi.initMemories`, which staggers the first jump and the
/// first ram so a herd does not act in lockstep.
pub fn init_memories(brain: &Brain) {
    brain.set_memory(
        memory_module_types::LONG_JUMP_COOLDOWN_TICKS,
        rand::random_range(
            TIME_BETWEEN_LONG_JUMPS.min_inclusive..=TIME_BETWEEN_LONG_JUMPS.max_inclusive,
        ),
    );
    brain.set_memory(
        memory_module_types::RAM_COOLDOWN_TICKS,
        rand::random_range(TIME_BETWEEN_RAMS.min_inclusive..=TIME_BETWEEN_RAMS.max_inclusive),
    );
}

/// Vanilla parity: `GoatAi.initCoreActivity`.
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
                memory_module_types::TEMPTATION_COOLDOWN_TICKS,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::LONG_JUMP_COOLDOWN_TICKS,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::RAM_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `GoatAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (0, gaze_at_players()),
            (
                0,
                Behavior::boxed(AnimalMakeLove::new(
                    &vanilla_entities::GOAT,
                    SPEED_MULTIPLIER_WHEN_IDLING,
                    1,
                )),
            ),
            (
                1,
                Behavior::boxed(FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)),
            ),
            (
                2,
                OneShot::boxed(BabyFollowAdult::new(
                    ADULT_FOLLOW_RANGE,
                    SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                )),
            ),
            (
                3,
                Box::new(RunOne::unconditional(vec![
                    (
                        OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
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
                        Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
                        1,
                    ),
                ])),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::RAM_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::LONG_JUMP_MID_JUMP.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: `GoatAi.initLongJumpActivity`.
fn long_jump_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::LongJump,
        vec![
            (
                0,
                Behavior::boxed(LongJumpMidJump::new(
                    TIME_BETWEEN_LONG_JUMPS,
                    &sound_events::ENTITY_GOAT_STEP,
                )),
            ),
            (
                1,
                Behavior::boxed(LongJumpToRandomPos::new(
                    TIME_BETWEEN_LONG_JUMPS,
                    MAX_LONG_JUMP_HEIGHT,
                    MAX_LONG_JUMP_WIDTH,
                    MAX_JUMP_VELOCITY_MULTIPLIER,
                    &sound_events::ENTITY_GOAT_LONG_JUMP,
                )),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::TEMPTING_PLAYER.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::BREED_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::WALK_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::LONG_JUMP_COOLDOWN_TICKS.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: `GoatAi.initRamActivity`.
fn ram_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Ram,
        vec![
            (0, Behavior::boxed(RamTarget::new(time_between_rams))),
            (
                1,
                Behavior::boxed(PrepareRamNearestTarget::new(
                    cooldown_on_failed_ram,
                    MIN_RAM_DISTANCE,
                    MAX_RAM_DISTANCE,
                    SPEED_MULTIPLIER_WHEN_PREPARING_TO_RAM,
                    RAM_PREPARE_TIME,
                )),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::TEMPTING_PLAYER.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::BREED_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::RAM_COOLDOWN_TICKS.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: `GoatAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Ram, Activity::LongJump, Activity::Idle]);
}

/// Vanilla parity: the `TIME_BETWEEN_RAMS` a screaming goat halves.
fn time_between_rams(goat: &GoatEntity) -> (i32, i32) {
    let range = if goat.is_screaming_goat() {
        TIME_BETWEEN_RAMS_SCREAMER
    } else {
        TIME_BETWEEN_RAMS
    };
    (range.min_inclusive, range.max_inclusive)
}

/// Vanilla parity: the `minInclusive` a failed ram is punished with.
fn cooldown_on_failed_ram(goat: &GoatEntity) -> i32 {
    time_between_rams(goat).0
}

/// Vanilla parity: the `SetEntityLookTargetSometimes.create(PLAYER, 6.0F, UniformInt.of(30, 60))`.
fn gaze_at_players() -> Box<dyn BehaviorControl> {
    OneShot::boxed(SetEntityLookTargetSometimes::of_type(
        &vanilla_entities::PLAYER,
        GAZE_RANGE,
        GAZE_INTERVAL,
    ))
}
