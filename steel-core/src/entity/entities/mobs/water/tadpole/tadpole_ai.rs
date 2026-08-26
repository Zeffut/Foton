//! The tadpole's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.frog.TadpoleAi`. Two
//! activities and nothing else: a tadpole panics, looks around, drifts, and
//! follows a player holding what a frog eats.

use steel_registry::vanilla_entities;
use steel_utils::value_providers::UniformIntProvider;

use crate::entity::PathfinderMob;
use crate::entity::ai::brain::behavior::{
    AnimalPanic, Behavior, BehaviorControl, CountDownCooldownTicks, FollowTemptation, GateBehavior,
    LookAtTargetSink, MoveToTargetSink, OneShot, OrderPolicy, RandomStroll, RunningPolicy,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, TriggerIf,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};

/// Vanilla parity: `TadpoleAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 2.0;
/// Vanilla parity: `TadpoleAi.SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER`.
const SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER: f64 = 0.5;
/// Vanilla parity: `TadpoleAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 1.25;

/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;

/// Vanilla parity: the `SetEntityLookTargetSometimes.create(PLAYER, 6.0F, UniformInt.of(30, 60))`.
const GAZE_RANGE: f64 = 6.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};

/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(0.5F, 3)` of the idle gate.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;

/// Weights of the idle gate's three entries, in vanilla's order.
const STROLL_WEIGHT: i32 = 2;
const WALK_TO_LOOK_TARGET_WEIGHT: i32 = 3;
const STAY_IN_WATER_WEIGHT: i32 = 5;

/// Vanilla parity: the sensor list of `Tadpole.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::HurtBy,
    SensorType::FrogTemptations,
];

/// Vanilla parity: `Tadpole.BRAIN_PROVIDER` plus `TadpoleAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(SENSORS, vec![core_activity(), idle_activity()])
}

/// Vanilla parity: `TadpoleAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING)),
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

/// Vanilla parity: `TadpoleAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                0,
                OneShot::boxed(SetEntityLookTargetSometimes::of_type(
                    &vanilla_entities::PLAYER,
                    GAZE_RANGE,
                    GAZE_INTERVAL,
                )),
            ),
            (
                1,
                Behavior::boxed(FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)),
            ),
            (2, idle_gate()),
        ],
    )
}

/// Vanilla parity: the `GateBehavior` of `TadpoleAi.initIdleActivity`, which
/// only runs while there is no walk target and tries each entry in turn.
fn idle_gate() -> Box<dyn BehaviorControl> {
    Box::new(GateBehavior::new(
        vec![(
            memory_module_types::WALK_TARGET.id(),
            MemoryStatus::ValueAbsent,
        )],
        Vec::new(),
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (
                OneShot::boxed(RandomStroll::swim(SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER)),
                STROLL_WEIGHT,
            ),
            (
                OneShot::boxed(SetWalkTargetFromLookTarget::new(
                    SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER,
                    WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                )),
                WALK_TO_LOOK_TARGET_WEIGHT,
            ),
            (
                // Vanilla parity: `BehaviorBuilder.triggerIf(Entity::isInWater)`,
                // which is what stops a tadpole in water from picking either of
                // the other two most of the time -- it simply stays put.
                OneShot::boxed(TriggerIf::new(
                    "StayInWater",
                    <dyn PathfinderMob>::is_in_water,
                )),
                STAY_IN_WATER_WEIGHT,
            ),
        ],
    ))
}

/// Vanilla parity: `TadpoleAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Idle]);
}
