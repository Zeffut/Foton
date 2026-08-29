//! The armadillo's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.armadillo.ArmadilloAi`.
//! Three activities: the shared core, an ordinary idle set, and a panic set
//! that is nothing but the ball-up -- which is where all the interesting
//! behaviour is.

use foton_registry::entity_data::ArmadilloState;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::{sound_events, vanilla_entities};
use foton_utils::Downcast as _;
use foton_utils::value_providers::UniformIntProvider;

use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, AnimalPanic, BabyFollowAdult, Behavior, CountDownCooldownTicks, DoNothing,
    FollowTemptation, LookAtTargetSink, MemoryModuleId, MemoryStatus, MoveToTargetSink, OnStart,
    OneShot, RandomLookAround, RandomStroll, RunOne, SetEntityLookTargetSometimes,
    SetWalkTargetFromLookTarget, Swim, TimedBehavior, Trigger,
};
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};
use crate::entity::entities::ArmadilloEntity;
use crate::entity::{AgeableMob, Entity as _};

/// Vanilla parity: `ArmadilloAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 2.0;
/// Vanilla parity: `ArmadilloAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 1.0;
/// Vanilla parity: `ArmadilloAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 1.25;
/// Vanilla parity: `ArmadilloAi.SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f64 = 1.25;
/// Vanilla parity: `ArmadilloAi.SPEED_MULTIPLIER_WHEN_MAKING_LOVE`.
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f64 = 1.0;
/// Vanilla parity: `ArmadilloAi.DEFAULT_CLOSE_ENOUGH_DIST`.
const DEFAULT_CLOSE_ENOUGH_DIST: f64 = 2.0;
/// Vanilla parity: `ArmadilloAi.BABY_CLOSE_ENOUGH_DIST`.
const BABY_CLOSE_ENOUGH_DIST: f64 = 1.0;
/// Vanilla parity: `ArmadilloAi.ADULT_FOLLOW_RANGE`.
const ADULT_FOLLOW_RANGE: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5,
    max_inclusive: 16,
};
/// Vanilla parity: the `0.8F` of the core activity's `Swim`.
const SWIM_CHANCE: f32 = 0.8;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `AnimalMakeLove(ARMADILLO, 1.0F, 1)` of the idle set.
const MAKE_LOVE_CLOSE_ENOUGH: i32 = 1;
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
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(1.0F, 3)` of the gate.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;
/// Vanilla parity: the `DoNothing(30, 60)` of the gate.
const DO_NOTHING_MIN: i32 = 30;
const DO_NOTHING_MAX: i32 = 60;

/// Vanilla parity: `ArmadilloBallUp.BALL_UP_STAY_IN_STATE`, five minutes.
const BALL_UP_STAY_IN_STATE: i32 = 5 * 60 * 20;
/// Vanilla parity: `ArmadilloBallUp.DANGER_DETECTED_RECENTLY_DANGER_THRESHOLD`.
const DANGER_THRESHOLD: i64 = 75;
/// Vanilla parity: the `nextIntBetweenInclusive(100, 400)` between peeks.
const PEEK_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 100,
    max_inclusive: 400,
};

/// Vanilla parity: the sensor list of `Armadillo.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::HurtBy,
    SensorType::FoodTemptations,
    SensorType::NearestAdult,
    SensorType::ArmadilloScareDetected,
];

/// Vanilla parity: `Armadillo.BRAIN_PROVIDER` plus `ArmadilloAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![core_activity(), idle_activity(), scared_activity()],
    )
}

/// Vanilla parity: `ArmadilloAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(Swim::new(SWIM_CHANCE)),
            // Vanilla parity: `ArmadilloAi.ArmadilloPanic`, whose only addition
            // is unrolling before it runs -- an armadillo cannot flee a fire in
            // a ball.
            Behavior::boxed(OnStart::new(
                AnimalPanic::with_damage_types(
                    SPEED_MULTIPLIER_WHEN_PANICKING,
                    DamageTypeTag::PANIC_ENVIRONMENTAL_CAUSES,
                ),
                |ctx| with_armadillo(ctx, ArmadilloEntity::roll_out),
            )),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            // Vanilla parity: the anonymous `MoveToTargetSink` subclass, which
            // refuses to walk anywhere while the armadillo is balled up.
            Behavior::boxed(MoveToTargetSink::new().with_extra_condition(|mob| {
                mob.downcast_ref::<ArmadilloEntity>()
                    .is_none_or(|armadillo| !armadillo.is_scared())
            })),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::TEMPTATION_COOLDOWN_TICKS,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::GAZE_COOLDOWN_TICKS,
            )),
            OneShot::boxed(ArmadilloRollingOut),
        ],
    )
}

/// Vanilla parity: `ArmadilloAi.initIdleActivity`.
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
                Behavior::boxed(AnimalMakeLove::new(
                    &vanilla_entities::ARMADILLO,
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
                                        DEFAULT_CLOSE_ENOUGH_DIST
                                    }
                                }),
                        ),
                        1,
                    ),
                    (
                        OneShot::boxed(BabyFollowAdult::new(
                            ADULT_FOLLOW_RANGE,
                            SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                        )),
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
                            OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                            1,
                        ),
                        (
                            OneShot::boxed(SetWalkTargetFromLookTarget::new(
                                SPEED_MULTIPLIER_WHEN_IDLING,
                                WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
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

/// Vanilla parity: `ArmadilloAi.initScaredActivity`.
fn scared_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Panic,
        vec![(0, Behavior::boxed(ArmadilloBallUp::new()))],
    )
    .with_conditions(vec![
        (
            memory_module_types::DANGER_DETECTED_RECENTLY.id(),
            MemoryStatus::ValuePresent,
        ),
        (
            memory_module_types::IS_PANICKING.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: `ArmadilloAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Panic, Activity::Idle]);
}

/// Runs `visit` on the body when it is an armadillo.
fn with_armadillo(ctx: &BrainContext<'_>, visit: impl FnOnce(&ArmadilloEntity)) {
    if let Some(armadillo) = ctx.mob().downcast_ref::<ArmadilloEntity>() {
        visit(armadillo);
    }
}

/// Vanilla parity: `ArmadilloAi.ARMADILLO_ROLLING_OUT`, which lives in the core
/// activity so it keeps running while the panic activity holds the idle one out.
struct ArmadilloRollingOut;

impl Trigger for ArmadilloRollingOut {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::DANGER_DETECTED_RECENTLY.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx
            .brain()
            .has_memory_value(memory_module_types::DANGER_DETECTED_RECENTLY.id())
        {
            return false;
        }
        let Some(armadillo) = ctx.mob().downcast_ref::<ArmadilloEntity>() else {
            return false;
        };
        if !armadillo.is_scared() {
            return false;
        }
        armadillo.roll_out();
        true
    }

    fn debug_name(&self) -> &'static str {
        "ArmadilloRollingOut"
    }
}

/// Vanilla parity: `ArmadilloAi.ArmadilloBallUp`.
///
/// The whole shell sequence: roll up, land, peek every so often while the
/// danger holds, and start unrolling when the danger memory is nearly out --
/// which is why an armadillo opens up gradually rather than popping open.
struct ArmadilloBallUp {
    next_peek_timer: i32,
    danger_was_around: bool,
}

impl ArmadilloBallUp {
    const fn new() -> Self {
        Self {
            next_peek_timer: 0,
            danger_was_around: false,
        }
    }

    fn pick_next_peek_timer() -> i32 {
        i32::try_from(super::armadillo_state_animation_duration(
            ArmadilloState::Scared,
        ))
        .unwrap_or(0)
            + rand::random_range(PEEK_INTERVAL.min_inclusive..=PEEK_INTERVAL.max_inclusive)
    }
}

impl TimedBehavior for ArmadilloBallUp {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn duration(&self) -> (i32, i32) {
        (BALL_UP_STAY_IN_STATE, BALL_UP_STAY_IN_STATE)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob().on_ground()
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob()
            .downcast_ref::<ArmadilloEntity>()
            .is_some_and(|armadillo| super::armadillo_state_is_threatened(armadillo.state()))
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        with_armadillo(ctx, ArmadilloEntity::roll_up);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        with_armadillo(ctx, |armadillo| {
            if !armadillo.can_stay_rolled_up() {
                armadillo.roll_out();
            }
        });
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        if self.next_peek_timer > 0 {
            self.next_peek_timer -= 1;
        }
        let Some(armadillo) = ctx.mob().downcast_ref::<ArmadilloEntity>() else {
            return;
        };

        if armadillo.should_switch_to_scared_state() {
            armadillo.switch_to_state(ArmadilloState::Scared);
            if armadillo.on_ground() {
                armadillo.play_sound(&sound_events::ENTITY_ARMADILLO_LAND, 1.0, 1.0);
            }
            return;
        }

        let state = armadillo.state();
        let danger_ticks = ctx
            .brain()
            .time_until_expiry(memory_module_types::DANGER_DETECTED_RECENTLY);
        let danger_is_around = danger_ticks > DANGER_THRESHOLD;
        if danger_is_around != self.danger_was_around {
            self.next_peek_timer = Self::pick_next_peek_timer();
        }
        self.danger_was_around = danger_is_around;

        let unrolling_duration =
            super::armadillo_state_animation_duration(ArmadilloState::Unrolling);
        if state == ArmadilloState::Scared {
            if self.next_peek_timer == 0 && armadillo.on_ground() && danger_is_around {
                armadillo.broadcast_peek();
                self.next_peek_timer = Self::pick_next_peek_timer();
            }
            if danger_ticks < unrolling_duration {
                armadillo.play_sound(&sound_events::ENTITY_ARMADILLO_UNROLL_START, 1.0, 1.0);
                armadillo.switch_to_state(ArmadilloState::Unrolling);
            }
        } else if state == ArmadilloState::Unrolling && danger_ticks > unrolling_duration {
            armadillo.switch_to_state(ArmadilloState::Scared);
        }
    }

    fn debug_name(&self) -> &'static str {
        "ArmadilloBallUp"
    }
}
