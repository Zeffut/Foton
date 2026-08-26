//! The camel's brain, shared by the camel and the camel husk.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.camel.CamelAi`. Two
//! activities, and what makes it a camel rather than a horse is `RandomSitting`
//! -- an idle camel that has been in one pose for twenty seconds flips to the
//! other, which is why a wild camel is usually found sitting down.
//!
//! `CamelHusk` inherits this whole brain unchanged, `AnimalMakeLove` on
//! `EntityType.CAMEL` included. That behavior is inert for a husk, which cannot
//! fall in love at all, exactly as it is in vanilla.

use steel_utils::value_providers::UniformIntProvider;

use crate::entity::AgeableMob;
use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, AnimalPanic, BabyFollowAdult, Behavior, BehaviorControl, BehaviorStatus,
    CountDownCooldownTicks, DoNothing, FollowTemptation, LookAtTargetSink, MemoryModuleId,
    MemoryStatus, MoveToTargetSink, OneShot, RandomLookAround, RandomStroll, RunOne,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, Swim, TimedBehavior,
};
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};

use super::camel_common::CamelHooks;

use steel_registry::vanilla_entities;

/// Vanilla parity: `CamelAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 4.0;
/// Vanilla parity: `CamelAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 2.0;
/// Vanilla parity: `CamelAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 2.5;
/// Vanilla parity: `CamelAi.SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f64 = 2.5;
/// Vanilla parity: `CamelAi.SPEED_MULTIPLIER_WHEN_MAKING_LOVE`, the default of
/// the one-argument `AnimalMakeLove`.
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f64 = 1.0;
/// Vanilla parity: the `1` close-enough of the same constructor.
const MAKE_LOVE_CLOSE_ENOUGH: i32 = 1;
/// Vanilla parity: the two `closeEnoughDist` of the camel's `FollowTemptation`.
const BABY_CLOSE_ENOUGH_DIST: f64 = 2.5;
const ADULT_CLOSE_ENOUGH_DIST: f64 = 3.5;
/// Vanilla parity: `CamelAi.ADULT_FOLLOW_RANGE`.
const ADULT_FOLLOW_RANGE: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5,
    max_inclusive: 16,
};
/// Vanilla parity: the `0.8F` of the core activity's `Swim`.
const SWIM_CHANCE: f32 = 0.8;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `SetEntityLookTargetSometimes.create(PLAYER, 6.0F, UniformInt.of(30, 60))`.
const GAZE_RANGE: f64 = 6.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};
/// Vanilla parity: the `RandomLookAround(UniformInt.of(150, 250), 30.0F, 0.0F, 0.0F)`.
const LOOK_AROUND_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 150,
    max_inclusive: 250,
};
const LOOK_AROUND_MAX_YAW: f32 = 30.0;
const LOOK_AROUND_MIN_PITCH: f32 = 0.0;
const LOOK_AROUND_MAX_PITCH: f32 = 0.0;
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(2.0F, 3)` of the gate.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;
/// Vanilla parity: the `DoNothing(30, 60)` of the gate.
const DO_NOTHING_MIN: i32 = 30;
const DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `new RandomSitting(20)` of the gate -- twenty seconds in
/// one pose before it will consider the other.
const RANDOM_SITTING_MINIMAL_POSE_SECONDS: i64 = 20;
const TICKS_PER_SECOND: i64 = 20;

/// Vanilla parity: the sensor list of `Camel.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::HurtBy,
    SensorType::FoodTemptations,
    SensorType::NearestAdult,
];

/// Vanilla parity: `Camel.BRAIN_PROVIDER` plus `CamelAi.getActivities`.
#[must_use]
pub(super) fn make_brain(hooks: CamelHooks) -> Brain {
    Brain::new(SENSORS, vec![core_activity(hooks), idle_activity(hooks)])
}

/// Vanilla parity: `CamelAi.initCoreActivity`.
fn core_activity(hooks: CamelHooks) -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(Swim::new(SWIM_CHANCE)),
            // Vanilla parity: `CamelAi.CamelPanic`, which stands the camel up
            // instantly before it runs and refuses to fire while a player is
            // steering.
            Behavior::boxed(CamelPanic::new(hooks)),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            Behavior::boxed(MoveToTargetSink::new()),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::TEMPTATION_COOLDOWN_TICKS,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::GAZE_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `CamelAi.initIdleActivity`.
fn idle_activity(hooks: CamelHooks) -> ActivityData {
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
                Behavior::boxed(AnimalMakeLove::new(
                    &vanilla_entities::CAMEL,
                    SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                    MAKE_LOVE_CLOSE_ENOUGH,
                )),
            ),
            (
                2,
                Box::new(RunOne::unconditional(vec![
                    (
                        Behavior::boxed(
                            FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)
                                .with_close_enough_distance(|mob| {
                                    if mob.as_ageable_mob().is_some_and(AgeableMob::is_baby) {
                                        BABY_CLOSE_ENOUGH_DIST
                                    } else {
                                        ADULT_CLOSE_ENOUGH_DIST
                                    }
                                }),
                        ),
                        1,
                    ),
                    (
                        while_camel_will_move(
                            hooks,
                            OneShot::boxed(BabyFollowAdult::new(
                                ADULT_FOLLOW_RANGE,
                                SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                            )),
                        ),
                        1,
                    ),
                ])),
            ),
            (
                3,
                Behavior::boxed(RandomLookAround::new(
                    LOOK_AROUND_INTERVAL,
                    LOOK_AROUND_MAX_YAW,
                    LOOK_AROUND_MIN_PITCH,
                    LOOK_AROUND_MAX_PITCH,
                )),
            ),
            (
                4,
                Box::new(RunOne::gated(
                    vec![(
                        memory_module_types::WALK_TARGET.id(),
                        MemoryStatus::ValueAbsent,
                    )],
                    vec![
                        (
                            while_camel_will_move(
                                hooks,
                                OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                            ),
                            1,
                        ),
                        (
                            while_camel_will_move(
                                hooks,
                                OneShot::boxed(SetWalkTargetFromLookTarget::new(
                                    SPEED_MULTIPLIER_WHEN_IDLING,
                                    WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                                )),
                            ),
                            1,
                        ),
                        (
                            Behavior::boxed(RandomSitting::new(
                                hooks,
                                RANDOM_SITTING_MINIMAL_POSE_SECONDS,
                            )),
                            1,
                        ),
                        (Box::new(DoNothing::new(DO_NOTHING_MIN, DO_NOTHING_MAX)), 1),
                    ],
                )),
            ),
        ],
    )
}

/// Vanilla parity: `CamelAi.updateActivity`.
pub(super) fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Idle]);
}

/// Vanilla parity: the `BehaviorBuilder.triggerIf(Predicate.not(Camel::refuseToMove), ...)`
/// three of the camel's idle behaviors are wrapped in -- a sitting camel is not
/// asked to walk anywhere in the first place.
struct WhileCamelWillMove {
    hooks: CamelHooks,
    inner: Box<dyn BehaviorControl>,
}

fn while_camel_will_move(
    hooks: CamelHooks,
    inner: Box<dyn BehaviorControl>,
) -> Box<dyn BehaviorControl> {
    Box::new(WhileCamelWillMove { hooks, inner })
}

impl BehaviorControl for WhileCamelWillMove {
    fn status(&self) -> BehaviorStatus {
        self.inner.status()
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        self.inner.required_memories()
    }

    fn try_start(&mut self, ctx: &BrainContext<'_>) -> bool {
        !(self.hooks.refuses_to_move)(ctx.mob()) && self.inner.try_start(ctx)
    }

    fn tick_or_stop(&mut self, ctx: &BrainContext<'_>) {
        self.inner.tick_or_stop(ctx);
    }

    fn do_stop(&mut self, ctx: &BrainContext<'_>) {
        self.inner.do_stop(ctx);
    }

    fn debug_name(&self) -> &'static str {
        self.inner.debug_name()
    }
}

/// Vanilla parity: `CamelAi.CamelPanic`, which will not fire while a player is
/// steering and stands the camel up before it runs.
struct CamelPanic {
    hooks: CamelHooks,
    inner: AnimalPanic,
}

impl CamelPanic {
    fn new(hooks: CamelHooks) -> Self {
        Self {
            hooks,
            inner: AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING),
        }
    }
}

impl TimedBehavior for CamelPanic {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        self.inner.entry_condition()
    }

    fn duration(&self) -> (i32, i32) {
        self.inner.duration()
    }

    fn times_out(&self) -> bool {
        self.inner.times_out()
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        !(self.hooks.is_mob_controlled)(ctx.mob()) && self.inner.check_extra_start_conditions(ctx)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.inner.can_still_use(ctx)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        (self.hooks.stand_up_instantly)(ctx.mob());
        self.inner.start(ctx);
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        self.inner.tick(ctx);
    }

    fn stop_with_timeout(&mut self, ctx: &BrainContext<'_>, timed_out: bool) {
        self.inner.stop_with_timeout(ctx, timed_out);
    }

    fn debug_name(&self) -> &'static str {
        "CamelPanic"
    }
}

/// Vanilla parity: `CamelAi.RandomSitting`.
///
/// One behavior for both directions: a camel that has held a pose for twenty
/// seconds flips to the other one, which is why an undisturbed camel spends
/// most of its life sitting down.
struct RandomSitting {
    hooks: CamelHooks,
    minimal_pose_ticks: i64,
}

impl RandomSitting {
    const fn new(hooks: CamelHooks, minimal_pose_seconds: i64) -> Self {
        Self {
            hooks,
            minimal_pose_ticks: minimal_pose_seconds * TICKS_PER_SECOND,
        }
    }
}

impl TimedBehavior for RandomSitting {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        (self.hooks.can_random_sit)(ctx.mob(), self.minimal_pose_ticks)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        (self.hooks.random_sit)(ctx.mob());
    }

    fn debug_name(&self) -> &'static str {
        "RandomSitting"
    }
}
