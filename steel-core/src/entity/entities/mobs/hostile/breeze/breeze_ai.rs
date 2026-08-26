//! The breeze's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.breeze.BreezeAi`.
//!
//! The fight activity is gated on `WALK_TARGET` being *absent*, which is what
//! makes the breeze alternate: [`super::behaviors::Slide`] runs in the fight
//! activity and sets a walk target, that immediately invalidates the fight
//! activity, and the idle activity's `SlideToTargetSink` is what actually walks
//! the breeze there. Losing that condition would leave a breeze rooted to the
//! spot.

use std::cell::Cell;

use super::behaviors::{
    LongJump, SPEED_MULTIPLIER_WHEN_SLIDING, Shoot, ShootWhenStuck, Slide, SlideToTargetSink,
};
use crate::entity::SharedEntity;
use crate::entity::ai::brain::behavior::{
    Behavior, DoNothing, LookAtTargetSink, OneShot, RandomStroll, RunOne, StartAttacking,
    StopAttackingIfTargetInvalid, Swim,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::{SensorType, is_entity_attackable};
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};

/// Vanilla parity: the `Swim<>(0.8F)` of the core activity.
const SWIM_CHANCE: f32 = 0.8;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `DoNothing(20, 100)` of the idle gate.
const IDLE_DO_NOTHING_MIN: i32 = 20;
const IDLE_DO_NOTHING_MAX: i32 = 100;
/// Vanilla parity: `BreezeAi.TICKS_TO_REMEMBER_SEEN_TARGET`.
const TICKS_TO_REMEMBER_SEEN_TARGET: i32 = 100;

/// The sensors a breeze runs.
///
/// Vanilla parity: the sensor list of `Breeze.BRAIN_PROVIDER`.
pub const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::HurtBy,
    SensorType::NearestPlayers,
    SensorType::BreezeAttackEntity,
];

/// Builds a breeze's brain.
///
/// Vanilla parity: `Breeze.makeBrain`, which sets and immediately uses `FIGHT`
/// as the default activity -- a breeze starts a fight the moment its sensor
/// hands it something attackable, without an idle tick in between.
#[must_use]
pub fn make_brain() -> Brain {
    let brain = Brain::new(
        SENSORS,
        vec![core_activity(), idle_activity(), fight_activity()],
    );
    brain.set_default_activity(Activity::Fight);
    brain.use_default_activity();
    brain
}

/// Vanilla parity: `BreezeAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(Swim::new(SWIM_CHANCE)),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
        ],
    )
}

/// Vanilla parity: `BreezeAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                0,
                OneShot::boxed(StartAttacking::new(|ctx| {
                    ctx.brain()
                        .get_memory(memory_module_types::NEAREST_ATTACKABLE)
                        .and_then(|remembered| remembered.get())
                })),
            ),
            (
                1,
                OneShot::boxed(StartAttacking::new(hurt_by_living_entity)),
            ),
            (2, Behavior::boxed(SlideToTargetSink::new())),
            (
                3,
                Box::new(RunOne::unconditional(vec![
                    (
                        Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
                        1,
                    ),
                    (
                        OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_SLIDING)),
                        2,
                    ),
                ])),
            ),
        ],
    )
}

/// Vanilla parity: `BreezeAi.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Fight,
        vec![
            (
                0,
                OneShot::boxed(
                    StopAttackingIfTargetInvalid::new().when(was_not_attackable_recently()),
                ),
            ),
            (1, Behavior::boxed(Shoot::new())),
            (2, Behavior::boxed(LongJump::new())),
            (3, Behavior::boxed(ShootWhenStuck::new())),
            (4, Behavior::boxed(Slide::new())),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::ATTACK_TARGET.id(),
            MemoryStatus::ValuePresent,
        ),
        (
            memory_module_types::WALK_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: `BreezeAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
}

/// Vanilla parity: `Breeze.getHurtBy`, which turns the damage source the
/// `HURT_BY` sensor stored back into whoever caused it.
fn hurt_by_living_entity(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    let source = ctx.brain().get_memory(memory_module_types::HURT_BY)?;
    let attacker = ctx.world().get_entity_by_id(source.causing_entity_id?)?;
    attacker.as_living_entity()?;
    Some(attacker)
}

/// Vanilla parity: the negation of
/// `Sensor.wasEntityAttackableLastNTicks(breeze, 100)`.
///
/// `rememberPositives` keeps a countdown rather than a timestamp: a target that
/// tests attackable resets it to a hundred, and every call after that spends
/// one. So a breeze keeps a target it has lost sight of for a hundred more
/// brain ticks and only then gives up, which is what stops it forgetting a
/// player who steps behind a pillar. The counter belongs to this one predicate,
/// exactly as vanilla's captured `AtomicInteger` belongs to one `Breeze`.
fn was_not_attackable_recently()
-> impl Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send + 'static {
    let positives_left = Cell::new(0);
    move |ctx, target| {
        let attackable = target
            .as_living_entity()
            .is_some_and(|living| is_entity_attackable(ctx.world(), ctx.mob(), living));
        if attackable {
            positives_left.set(TICKS_TO_REMEMBER_SEEN_TARGET);
            return false;
        }
        let left = positives_left.get() - 1;
        positives_left.set(left);
        left < 0
    }
}
