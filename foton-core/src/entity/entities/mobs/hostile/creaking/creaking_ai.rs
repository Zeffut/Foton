//! The creaking's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.creaking.CreakingAi`.
//!
//! Two things here are unlike every other brain mob. The fight activity's melee
//! attack is gated on `canMove`, so a creaking a player is staring at stops
//! mid-swing; and `updateActivity` forces the *default* activity while frozen
//! rather than picking the first valid one, which is what keeps a frozen
//! creaking from quietly re-targeting.

use foton_utils::Downcast as _;
use foton_utils::value_providers::UniformIntProvider;

use crate::entity::LivingEntity;
use crate::entity::ai::brain::behavior::{
    Behavior, BrainContext, DoNothing, LookAtTargetSink, MeleeAttack, MemoryModuleId, MemoryStatus,
    MoveToTargetSink, OneShot, RandomStroll, RunOne, SetEntityLookTargetSometimes,
    SetWalkTargetFromAttackTargetIfTargetOutOfReach, SetWalkTargetFromLookTarget, StartAttacking,
    StopAttackingIfTargetInvalid, Swim, TimedBehavior,
};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};
use crate::entity::{PathfinderMob, SharedEntity};

use super::CreakingEntity;

/// Vanilla parity: `Creaking.ATTACK_INTERVAL`.
const ATTACK_INTERVAL: i64 = 40;
/// Vanilla parity: `Creaking.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 0.3;
/// Vanilla parity: the `1.0F` of the fight activity's walk behavior.
const SPEED_MULTIPLIER_WHEN_CHASING: f64 = 1.0;
/// Vanilla parity: the `Swim<>(0.8F)` of the core activity.
const SWIM_CHANCE: f32 = 0.8;
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
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(0.3F, 3)` of the same gate.
const IDLE_LOOK_WALK_CLOSE_ENOUGH: i32 = 3;

/// The sensors a creaking runs.
///
/// Vanilla parity: the sensor list of `Creaking.BRAIN_PROVIDER`. The player
/// sensor is not optional decoration here: both the freeze and the stuck-player
/// check read `NEAREST_PLAYERS` straight out of the brain.
pub const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
];

/// Builds a creaking's brain.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![core_activity(), idle_activity(), fight_activity()],
    )
}

/// Vanilla parity: `CreakingAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(SwimWhileUnfrozen::new()),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            Behavior::boxed(MoveToTargetSink::new()),
        ],
    )
}

/// Vanilla parity: `CreakingAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            OneShot::boxed(StartAttacking::conditional(
                |ctx| with_creaking(ctx.mob(), CreakingEntity::is_active),
                |ctx| {
                    ctx.brain()
                        .get_memory(memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
                        .and_then(|remembered| remembered.get())
                },
            )),
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

/// Vanilla parity: `CreakingAi.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        10,
        vec![
            OneShot::boxed(SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(
                SPEED_MULTIPLIER_WHEN_CHASING,
            )),
            OneShot::boxed(MeleeAttack::conditional(
                |mob| with_living_creaking(mob, CreakingEntity::can_move),
                ATTACK_INTERVAL,
            )),
            OneShot::boxed(
                StopAttackingIfTargetInvalid::new()
                    .when(|ctx, target| !is_attack_target_still_reachable(ctx, target)),
            ),
        ],
    )
    .with_conditions(vec![(
        memory_module_types::ATTACK_TARGET.id(),
        MemoryStatus::ValuePresent,
    )])
}

/// Vanilla parity: `CreakingAi.updateActivity`.
///
/// A frozen creaking is forced back to the default activity rather than being
/// offered the first valid one, so it cannot pick a new fight while it is being
/// stared at.
pub fn update_activity(brain: &Brain, can_move: bool) {
    if can_move {
        brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
    } else {
        brain.use_default_activity();
    }
}

/// Vanilla parity: the private `CreakingAi.isAttackTargetStillReachable`, which
/// keeps a target only while the player sensor can still see it. That is what
/// makes stepping out of sight enough to lose a creaking, without the usual
/// hundred-tick memory.
fn is_attack_target_still_reachable(ctx: &BrainContext<'_>, target: &SharedEntity) -> bool {
    let Some(visible) = ctx
        .brain()
        .get_memory(memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYERS)
    else {
        return false;
    };
    visible
        .iter()
        .any(|remembered| remembered.id() == target.id())
}

fn with_creaking(mob: &dyn PathfinderMob, read: impl FnOnce(&CreakingEntity) -> bool) -> bool {
    mob.downcast_ref::<CreakingEntity>().is_some_and(read)
}

fn with_living_creaking(
    mob: &dyn LivingEntity,
    read: impl FnOnce(&CreakingEntity) -> bool,
) -> bool {
    mob.downcast_ref::<CreakingEntity>().is_some_and(read)
}

/// Vanilla parity: the anonymous `Swim` subclass of `CreakingAi.initCoreActivity`,
/// whose `checkExtraStartConditions` refuses while the creaking is frozen. A
/// frozen creaking standing in water drowns rather than swimming out, which is
/// the point.
struct SwimWhileUnfrozen {
    inner: Swim,
}

impl SwimWhileUnfrozen {
    const fn new() -> Self {
        Self {
            inner: Swim::new(SWIM_CHANCE),
        }
    }
}

impl TimedBehavior for SwimWhileUnfrozen {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        self.inner.entry_condition()
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        with_creaking(ctx.mob(), CreakingEntity::can_move)
            && self.inner.check_extra_start_conditions(ctx)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.inner.can_still_use(ctx)
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        self.inner.tick(ctx);
    }

    fn debug_name(&self) -> &'static str {
        "Swim"
    }
}
