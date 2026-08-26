//! The zoglin's brain.
//!
//! Vanilla parity: the static `getActivities`, `initCoreActivity`,
//! `initIdleActivity` and `initFightActivity` of
//! `net.minecraft.world.entity.monster.Zoglin`. Vanilla keeps them on the mob
//! class rather than in a separate `Ai` helper; Steel splits them out so the
//! entity file stays about the entity.

use steel_registry::vanilla_entities;
use steel_utils::value_providers::UniformIntProvider;

use crate::entity::ai::brain::behavior::{
    Behavior, DoNothing, LookAtTargetSink, MeleeAttack, MoveToTargetSink, OneShot, RandomStroll,
    RunOne, SetEntityLookTargetSometimes, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, StopAttackingIfTargetInvalid, utils,
};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::{SensorType, is_entity_attackable};
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::{LivingEntity, SharedEntity};

/// Vanilla parity: `Zoglin.ATTACK_INTERVAL`.
const ATTACK_INTERVAL: i64 = 40;
/// Vanilla parity: `Zoglin.BABY_ATTACK_INTERVAL`.
const BABY_ATTACK_INTERVAL: i64 = 15;
/// Vanilla parity: `Zoglin.MOVEMENT_SPEED_WHEN_FIGHTING`, the `1.0F` of the
/// fight activity's walk behavior.
const SPEED_MULTIPLIER_WHEN_CHASING: f64 = 1.0;
/// Vanilla parity: `Zoglin.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 0.4;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `SetEntityLookTargetSometimes.create(8.0F, UniformInt.of(30, 60))`
/// of the idle activity.
const GAZE_RANGE: f64 = 8.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};
/// Vanilla parity: the `DoNothing(30, 60)` of the idle gate.
const IDLE_DO_NOTHING_MIN: i32 = 30;
const IDLE_DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(0.4F, 3)` of the same gate.
const IDLE_LOOK_WALK_CLOSE_ENOUGH: i32 = 3;

/// The sensors a zoglin runs.
///
/// Vanilla parity: the sensor list of `Zoglin.BRAIN_PROVIDER`, which is the
/// shortest of any brain mob: it needs to know what is nearby, and nothing else.
pub const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
];

/// Builds a zoglin's brain.
///
/// Vanilla parity: `Zoglin.BRAIN_PROVIDER` feeding `Zoglin.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![core_activity(), idle_activity(), fight_activity()],
    )
}

/// Vanilla parity: `Zoglin.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            Behavior::boxed(MoveToTargetSink::new()),
        ],
    )
}

/// Vanilla parity: `Zoglin.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            OneShot::boxed(StartAttacking::new(find_nearest_valid_attack_target)),
            OneShot::boxed(SetEntityLookTargetSometimes::any_within(
                GAZE_RANGE,
                GAZE_INTERVAL,
            )),
            Box::new(RunOne::unconditional(vec![
                (
                    OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                    2,
                ),
                (
                    OneShot::boxed(SetWalkTargetFromLookTarget::new(
                        SPEED_MULTIPLIER_WHEN_IDLING,
                        IDLE_LOOK_WALK_CLOSE_ENOUGH,
                    )),
                    2,
                ),
                (
                    Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
                    1,
                ),
            ])),
        ],
    )
}

/// Vanilla parity: `Zoglin.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        10,
        vec![
            OneShot::boxed(SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(
                SPEED_MULTIPLIER_WHEN_CHASING,
            )),
            OneShot::boxed(MeleeAttack::conditional(
                |mob| !mob.is_baby(),
                ATTACK_INTERVAL,
            )),
            OneShot::boxed(MeleeAttack::conditional(
                LivingEntity::is_baby,
                BABY_ATTACK_INTERVAL,
            )),
            OneShot::boxed(StopAttackingIfTargetInvalid::new()),
        ],
    )
    .gated_by(memory_module_types::ATTACK_TARGET.id())
}

/// Vanilla parity: the private `Zoglin.findNearestValidAttackTarget`, which
/// attacks anything visible that is neither another zoglin nor a creeper.
fn find_nearest_valid_attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    ctx.brain()
        .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)?
        .find_closest(|candidate| {
            let raw = candidate.as_entity_event_source();
            !utils::is_of_type(raw, &vanilla_entities::ZOGLIN)
                && !utils::is_of_type(raw, &vanilla_entities::CREEPER)
                && is_entity_attackable(ctx.world(), ctx.mob(), candidate)
        })
}
