//! The warden's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.warden.WardenAi`. Eight
//! activities, and the order they are tried in is the warden's whole personality: it
//! emerges, then digs away if nothing has happened, then roars, then fights, then goes to
//! look at whatever it heard, then sniffs, then wanders.

use steel_utils::BlockPos;

use crate::entity::ai::brain::behavior::{
    Behavior, DoNothing, GoToTargetLocation, LookAtTargetSink, MeleeAttack, MoveToTargetSink,
    OneShot, RandomStroll, RunOne, SetEntityLookTarget,
    SetWalkTargetFromAttackTargetIfTargetOutOfReach, StopAttackingIfTargetInvalid, Swim,
};
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryStatus, Unit, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::{LivingEntity, SharedEntity};

use super::behaviors;
use super::entity::WardenEntity;

/// Vanilla `WardenAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 0.5;
/// Vanilla `WardenAi.SPEED_MULTIPLIER_WHEN_INVESTIGATING`.
const SPEED_MULTIPLIER_WHEN_INVESTIGATING: f64 = 0.7;
/// Vanilla `WardenAi.SPEED_MULTIPLIER_WHEN_FIGHTING`.
const SPEED_MULTIPLIER_WHEN_FIGHTING: f64 = 1.2;
/// Vanilla `WardenAi.MELEE_ATTACK_COOLDOWN`.
const MELEE_ATTACK_COOLDOWN: i64 = 18;
/// Vanilla `WardenAi.DIGGING_DURATION`, `Mth.ceil(100.0F)`.
const DIGGING_DURATION: i32 = 100;
/// Vanilla `WardenAi.EMERGE_DURATION`, `Mth.ceil(133.59999F)`.
pub const EMERGE_DURATION: i32 = 134;
/// Vanilla `WardenAi.ROAR_DURATION`, `Mth.ceil(84.0F)`.
pub const ROAR_DURATION: i32 = 84;
/// Vanilla `WardenAi.SNIFFING_DURATION`, `Mth.ceil(83.2F)`.
const SNIFFING_DURATION: i32 = 84;
/// Vanilla `WardenAi.DIGGING_COOLDOWN`.
pub const DIGGING_COOLDOWN: i32 = 1200;
/// Vanilla `WardenAi.DISTURBANCE_LOCATION_EXPIRY_TIME`.
const DISTURBANCE_LOCATION_EXPIRY_TIME: i64 = 100;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `DoNothing(30, 60)` of the idle gate.
const IDLE_DO_NOTHING_MIN: i32 = 30;
const IDLE_DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `Swim<>(0.8F)` of the core activity.
const SWIM_CHANCE: f32 = 0.8;
/// Vanilla parity: the `2` close-enough distance of the investigate walk.
const INVESTIGATE_CLOSE_ENOUGH: i32 = 2;
/// Vanilla parity: the `Attributes.FOLLOW_RANGE, 24.0` of `Warden.createAttributes`.
///
/// Vanilla reads this off the warden it is building the brain for; Steel builds the brain
/// before there is a warden to read, and the value is the entity type's base either way.
const FOLLOW_RANGE: f64 = 24.0;

/// The sensors a warden runs.
///
/// Vanilla parity: the sensor list of `Warden.BRAIN_PROVIDER`. A warden has no sight
/// sensor beyond these two: everything else it learns comes through vibrations.
pub const SENSORS: &[SensorType] = &[SensorType::NearestPlayers, SensorType::WardenEntity];

/// The memories vanilla registers on the warden's brain without a sensor writing them.
///
/// Vanilla parity: the `memoryTypes` list of `Warden.BRAIN_PROVIDER`.
const EXTRA_MEMORIES: &[MemoryModuleId] = &[
    memory_module_types::NEAREST_VISIBLE_NEMESIS.id(),
    memory_module_types::RECENT_PROJECTILE.id(),
    memory_module_types::TOUCH_COOLDOWN.id(),
    memory_module_types::VIBRATION_COOLDOWN.id(),
];

/// Builds a warden's brain.
///
/// Vanilla parity: `Warden.BRAIN_PROVIDER` feeding `WardenAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new_with_memories(
        SENSORS,
        EXTRA_MEMORIES,
        vec![
            core_activity(),
            emerge_activity(),
            digging_activity(),
            idle_activity(),
            roar_activity(),
            fight_activity(),
            investigate_activity(),
            sniffing_activity(),
        ],
    )
}

/// Vanilla parity: `WardenAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[
        Activity::Emerge,
        Activity::Dig,
        Activity::Roar,
        Activity::Fight,
        Activity::Investigate,
        Activity::Sniff,
        Activity::Idle,
    ]);
}

/// Vanilla parity: `WardenAi.setDigCooldown`.
///
/// The cooldown is only refreshed, never created: a warden that never had one is a
/// warden that is free to dig away as soon as it runs out of things to do.
pub fn set_dig_cooldown(brain: &Brain) {
    if brain.has_memory_value(memory_module_types::DIG_COOLDOWN.id()) {
        brain.set_memory_with_expiry(
            memory_module_types::DIG_COOLDOWN,
            Unit,
            DIGGING_COOLDOWN.into(),
        );
    }
}

/// Vanilla parity: `SonicBoom.setCooldown`.
pub fn set_sonic_boom_cooldown(brain: &Brain, cooldown: i64) {
    brain.set_memory_with_expiry(memory_module_types::SONIC_BOOM_COOLDOWN, Unit, cooldown);
}

/// Vanilla parity: `WardenAi.setDisturbanceLocation`.
///
/// Not implemented: the world-border bounds check vanilla guards this with, for the
/// reason given on [`WardenEntity::can_target_entity`].
pub fn set_disturbance_location(warden: &WardenEntity, position: BlockPos) {
    let brain = warden.brain_ref();
    if warden.entity_angry_at().is_some()
        || brain.has_memory_value(memory_module_types::ATTACK_TARGET.id())
    {
        return;
    }

    set_dig_cooldown(brain);
    brain.set_memory_with_expiry(
        memory_module_types::SNIFF_COOLDOWN,
        Unit,
        DISTURBANCE_LOCATION_EXPIRY_TIME,
    );
    brain.set_memory_with_expiry(
        memory_module_types::LOOK_TARGET,
        PositionTracker::of_block(position),
        DISTURBANCE_LOCATION_EXPIRY_TIME,
    );
    brain.set_memory_with_expiry(
        memory_module_types::DISTURBANCE_LOCATION,
        position,
        DISTURBANCE_LOCATION_EXPIRY_TIME,
    );
    brain.erase_memory(memory_module_types::WALK_TARGET.id());
}

/// Vanilla parity: `WardenAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(Swim::new(SWIM_CHANCE)),
            OneShot::boxed(behaviors::SetWardenLookTarget),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            Behavior::boxed(MoveToTargetSink::new()),
        ],
    )
}

/// Vanilla parity: `WardenAi.initEmergeActivity`.
fn emerge_activity() -> ActivityData {
    ActivityData::create(
        Activity::Emerge,
        5,
        vec![Behavior::boxed(behaviors::Emerging::new(EMERGE_DURATION))],
    )
    .gated_by(memory_module_types::IS_EMERGING.id())
}

/// Vanilla parity: `WardenAi.initDiggingActivity`.
fn digging_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Dig,
        vec![
            (0, Behavior::boxed(behaviors::ForceUnmount)),
            (
                1,
                Behavior::boxed(behaviors::Digging::new(DIGGING_DURATION)),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::ROAR_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::DIG_COOLDOWN.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: `WardenAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            OneShot::boxed(behaviors::SetRoarTarget::new(WardenEntity::entity_angry_at)),
            OneShot::boxed(behaviors::TryToSniff),
            Box::new(RunOne::gated(
                vec![(
                    memory_module_types::IS_SNIFFING.id(),
                    MemoryStatus::ValueAbsent,
                )],
                vec![
                    (
                        OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        2,
                    ),
                    (
                        Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
                        1,
                    ),
                ],
            )),
        ],
    )
}

/// Vanilla parity: `WardenAi.initInvestigateActivity`.
fn investigate_activity() -> ActivityData {
    ActivityData::create(
        Activity::Investigate,
        5,
        vec![
            OneShot::boxed(behaviors::SetRoarTarget::new(WardenEntity::entity_angry_at)),
            OneShot::boxed(GoToTargetLocation::new(
                memory_module_types::DISTURBANCE_LOCATION,
                INVESTIGATE_CLOSE_ENOUGH,
                SPEED_MULTIPLIER_WHEN_INVESTIGATING,
            )),
        ],
    )
    .gated_by(memory_module_types::DISTURBANCE_LOCATION.id())
}

/// Vanilla parity: `WardenAi.initSniffingActivity`.
fn sniffing_activity() -> ActivityData {
    ActivityData::create(
        Activity::Sniff,
        5,
        vec![
            OneShot::boxed(behaviors::SetRoarTarget::new(WardenEntity::entity_angry_at)),
            Behavior::boxed(behaviors::Sniffing::new(SNIFFING_DURATION)),
        ],
    )
    .gated_by(memory_module_types::IS_SNIFFING.id())
}

/// Vanilla parity: `WardenAi.initRoarActivity`.
fn roar_activity() -> ActivityData {
    ActivityData::create(Activity::Roar, 10, vec![Behavior::boxed(behaviors::Roar)])
        .gated_by(memory_module_types::ROAR_TARGET.id())
}

/// Vanilla parity: `WardenAi.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        10,
        vec![
            behaviors::dig_cooldown_setter(),
            OneShot::boxed(
                StopAttackingIfTargetInvalid::new()
                    .when(|ctx, target| {
                        use steel_utils::Downcast as _;

                        // Vanilla stops when the warden has calmed down or the target has
                        // stopped being something it could attack.
                        ctx.mob()
                            .downcast_ref::<WardenEntity>()
                            .is_none_or(|warden| {
                                !warden.anger_level().is_angry()
                                    || !warden.can_target_entity(Some(target.as_ref()))
                            })
                    })
                    .on_erased(on_target_invalid)
                    .never_tiring(),
            ),
            OneShot::boxed(SetEntityLookTarget::matching_in_context(
                is_attack_target,
                FOLLOW_RANGE,
            )),
            OneShot::boxed(SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(
                SPEED_MULTIPLIER_WHEN_FIGHTING,
            )),
            Behavior::boxed(behaviors::SonicBoom),
            OneShot::boxed(MeleeAttack::new(MELEE_ATTACK_COOLDOWN)),
        ],
    )
    .gated_by(memory_module_types::ATTACK_TARGET.id())
}

/// Vanilla parity: `WardenAi.onTargetInvalid`.
fn on_target_invalid(ctx: &BrainContext<'_>, target: &SharedEntity) {
    use steel_utils::Downcast as _;

    let Some(warden) = ctx.mob().downcast_ref::<WardenEntity>() else {
        return;
    };
    if !warden.can_target_entity(Some(target.as_ref())) {
        warden.clear_anger(target.as_ref());
    }
    set_dig_cooldown(warden.brain_ref());
}

/// Vanilla parity: the `entity -> isTarget(body, entity)` of the fight activity's look.
fn is_attack_target(ctx: &BrainContext<'_>, entity: &dyn LivingEntity) -> bool {
    ctx.brain()
        .get_memory(memory_module_types::ATTACK_TARGET)
        .is_some_and(|memory| memory.id() == entity.id())
}
