//! The ghastling's brain.
//!
//! Vanilla parity:
//! `net.minecraft.world.entity.animal.happyghast.HappyGhastAi`. Only the baby
//! runs it -- an adult happy ghast is driven by goals -- so everything here is
//! about a ghastling drifting after whoever it has decided to follow.

use foton_utils::value_providers::UniformIntProvider;

use crate::entity::ai::brain::behavior::{
    AnimalPanic, BabyFollowAdult, Behavior, CountDownCooldownTicks, FollowTemptation,
    LookAtTargetSink, MoveToTargetSink, OneShot, RandomStroll, RunOne, SetWalkTargetFromLookTarget,
    Swim,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};

/// Vanilla parity: `HappyGhast.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 2.0;
/// Vanilla parity: `HappyGhastAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 1.0;
/// Vanilla parity: `HappyGhastAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 1.25;
/// Vanilla parity: `HappyGhastAi.SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f64 = 1.1;
/// Vanilla parity: `HappyGhastAi.BABY_GHAST_CLOSE_ENOUGH_DIST`.
const BABY_GHAST_CLOSE_ENOUGH_DIST: f64 = 3.0;
/// Vanilla parity: `HappyGhastAi.ADULT_FOLLOW_RANGE`.
const ADULT_FOLLOW_RANGE: UniformIntProvider = UniformIntProvider {
    min_inclusive: 3,
    max_inclusive: 16,
};
/// Vanilla parity: the `0` flight height of the core activity's `AnimalPanic`,
/// which sends a fleeing ghastling sideways rather than up.
const PANIC_FLYING_HEIGHT: i32 = 0;
/// Vanilla parity: the `0.8F` of the core activity's `Swim`.
const SWIM_CHANCE: f32 = 0.8;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(1.0F, 3)` of the
/// idle gate.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;

/// Vanilla parity: the sensor list of `HappyGhast.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::HurtBy,
    SensorType::FoodTemptations,
    SensorType::NearestAdultAnyType,
    SensorType::NearestPlayers,
];

/// Vanilla parity: `HappyGhast.BRAIN_PROVIDER` plus `HappyGhastAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![core_activity(), idle_activity(), panic_activity()],
    )
}

/// Vanilla parity: `HappyGhastAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(Swim::new(SWIM_CHANCE)),
            Behavior::boxed(AnimalPanic::flying(
                SPEED_MULTIPLIER_WHEN_PANICKING,
                PANIC_FLYING_HEIGHT,
            )),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            Behavior::boxed(MoveToTargetSink::new()),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::TEMPTATION_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `HappyGhastAi.initIdleActivity`.
///
/// The two `BabyFollowAdult` entries are the whole of a ghastling's social
/// life: the first trails the nearest visible player, the second any adult on
/// the `followable_friendly_mobs` list, and a player outranks the adult.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                1,
                Behavior::boxed(
                    FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)
                        .with_close_enough_distance(|_| BABY_GHAST_CLOSE_ENOUGH_DIST)
                        .looking_in_the_eyes(),
                ),
            ),
            (
                2,
                OneShot::boxed(
                    BabyFollowAdult::variable(ADULT_FOLLOW_RANGE, |_| {
                        SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT
                    })
                    .following(memory_module_types::NEAREST_VISIBLE_PLAYER)
                    .targeting_eye(),
                ),
            ),
            (
                3,
                OneShot::boxed(
                    BabyFollowAdult::variable(ADULT_FOLLOW_RANGE, |_| {
                        SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT
                    })
                    .following(memory_module_types::NEAREST_VISIBLE_ADULT)
                    .targeting_eye(),
                ),
            ),
            (
                4,
                Box::new(RunOne::unconditional(vec![
                    (
                        OneShot::boxed(RandomStroll::fly(SPEED_MULTIPLIER_WHEN_IDLING)),
                        1,
                    ),
                    (
                        OneShot::boxed(SetWalkTargetFromLookTarget::new(
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                        )),
                        1,
                    ),
                ])),
            ),
        ],
    )
}

/// Vanilla parity: `HappyGhastAi.initPanicActivity`, which holds no behaviors
/// at all. Its whole job is to be the activity a panicking ghastling is in, so
/// the idle set stops running while `AnimalPanic` from the core set drives.
fn panic_activity() -> ActivityData {
    ActivityData::with_priorities(Activity::Panic, Vec::new()).with_conditions(vec![(
        memory_module_types::IS_PANICKING.id(),
        MemoryStatus::ValuePresent,
    )])
}

/// Vanilla parity: `HappyGhastAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Panic, Activity::Idle]);
}
