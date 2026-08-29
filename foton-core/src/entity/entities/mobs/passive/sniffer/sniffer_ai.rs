//! The sniffer's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.sniffer.SnifferAi`. Four
//! activities and a six-step state machine: a sniffer scents, sniffs, walks to
//! the spot it picked, searches it, digs, rises, and is happy about it. Every
//! step is one behavior, and each one moves the synced `SnifferState` the client
//! animates from.

use foton_registry::entity_data::SnifferState;
use foton_registry::vanilla_entities;
use foton_utils::Downcast as _;

use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, AnimalPanic, Behavior, CountDownCooldownTicks, DoNothing, FollowTemptation,
    LookAtTargetSink, MoveToTargetSink, OnStart, OneShot, RandomStroll, RunOne,
    SetEntityLookTarget, SetWalkTargetFromLookTarget, Swim, TimedBehavior,
};
use crate::entity::ai::brain::memory::{
    MemoryModuleId, MemoryStatus, Unit, WalkTarget, memory_module_types,
};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::entities::SnifferEntity;
use crate::entity::{AgeableMob, Animal};

/// Vanilla parity: `SnifferAi.SNIFFING_COOLDOWN_TICKS`.
const SNIFFING_COOLDOWN_TICKS: i64 = 9600;
/// Vanilla parity: `SnifferAi.MAX_LOOK_DISTANCE`.
const MAX_LOOK_DISTANCE: f64 = 6.0;
/// Vanilla parity: `SnifferAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 1.0;
/// Vanilla parity: `SnifferAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 2.0;
/// Vanilla parity: `SnifferAi.SPEED_MULTIPLIER_WHEN_SNIFFING`.
const SPEED_MULTIPLIER_WHEN_SNIFFING: f64 = 1.25;
/// Vanilla parity: `SnifferAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 1.25;
/// Vanilla parity: the `0.8F` of the core activity's `Swim`.
const SWIM_CHANCE: f32 = 0.8;

/// Vanilla parity: the `MoveToTargetSink(500, 700)` of the core activity, which
/// gives a sniffer far longer than the shared mob to reach where it is going.
const MOVE_TO_TARGET_MIN: i32 = 500;
const MOVE_TO_TARGET_MAX: i32 = 700;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the idle activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;

/// Vanilla parity: the `AnimalMakeLove(EntityTypes.SNIFFER)` defaults.
const MAKE_LOVE_SPEED_MODIFIER: f64 = 1.0;
const MAKE_LOVE_CLOSE_ENOUGH: i32 = 1;

/// Vanilla parity: the `sniffer -> sniffer.isBaby() ? 2.5 : 3.5` of the idle
/// activity's `FollowTemptation`.
const TEMPTED_CLOSE_ENOUGH_BABY: f64 = 2.5;
const TEMPTED_CLOSE_ENOUGH_ADULT: f64 = 3.5;

/// Vanilla parity: the durations of `FeelingHappy`, `Scenting` and `Sniffing`.
const FEELING_HAPPY_MIN: i32 = 40;
const FEELING_HAPPY_MAX: i32 = 100;
const SCENTING_MIN: i32 = 40;
const SCENTING_MAX: i32 = 80;
const SNIFFING_MIN: i32 = 40;
const SNIFFING_MAX: i32 = 80;
/// Vanilla parity: the `Digging(160, 180)` of the dig activity.
const DIGGING_MIN: i32 = 160;
const DIGGING_MAX: i32 = 180;
/// Vanilla parity: the `FinishedDigging(40)`.
const FINISHED_DIGGING_DURATION: i32 = 40;
/// Vanilla parity: the `Searching`'s `600` timeout.
const SEARCHING_TIMEOUT: i32 = 600;
/// Vanilla parity: the `DoNothing(5, 20)` of the idle gate.
const DO_NOTHING_MIN: i32 = 5;
const DO_NOTHING_MAX: i32 = 20;
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(1.0F, 3)`.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;

/// Weights of the idle gate, in vanilla's order.
const WALK_TO_LOOK_WEIGHT: i32 = 2;
const SCENTING_WEIGHT: i32 = 1;
const SNIFFING_WEIGHT: i32 = 1;
const LOOK_AT_PLAYER_WEIGHT: i32 = 1;
const STROLL_WEIGHT: i32 = 1;
const DO_NOTHING_WEIGHT: i32 = 2;

/// Vanilla parity: the sensor list of `Sniffer.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::HurtBy,
    SensorType::NearestPlayers,
    SensorType::FoodTemptations,
];

/// Vanilla parity: `Sniffer.BRAIN_PROVIDER` plus `SnifferAi.getActivities`.
///
/// The provider also registers `SNIFFER_EXPLORED_POSITIONS` up front, because
/// nothing in the activity list requires it and a sniffer that never registered
/// it would forget every hole it had already dug.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new_with_memories(
        SENSORS,
        &[memory_module_types::SNIFFER_EXPLORED_POSITIONS.id()],
        vec![
            core_activity(),
            idle_activity(),
            sniffing_activity(),
            dig_activity(),
        ],
    )
}

/// Vanilla parity: `SnifferAi.resetSniffing`.
fn reset_sniffing(ctx: &BrainContext<'_>) {
    ctx.brain()
        .erase_memory(memory_module_types::SNIFFER_DIGGING.id());
    ctx.brain()
        .erase_memory(memory_module_types::SNIFFER_SNIFFING_TARGET.id());
    if let Some(sniffer) = sniffer_of(ctx) {
        sniffer.transition_to(SnifferState::Idling);
    }
}

fn sniffer_of<'a>(ctx: &'a BrainContext<'_>) -> Option<&'a SnifferEntity> {
    ctx.mob().downcast_ref::<SnifferEntity>()
}

/// Vanilla parity: `SnifferAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(Swim::new(SWIM_CHANCE)),
            // Vanilla subclasses `AnimalPanic` here purely to reset the sniffing
            // state first, which Foton does with a start hook instead.
            Behavior::boxed(OnStart::new(
                AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING),
                reset_sniffing,
            )),
            Behavior::boxed(MoveToTargetSink::with_timeout(
                MOVE_TO_TARGET_MIN,
                MOVE_TO_TARGET_MAX,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::TEMPTATION_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `SnifferAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                0,
                Behavior::boxed(OnStart::new(
                    AnimalMakeLove::new(
                        &vanilla_entities::SNIFFER,
                        MAKE_LOVE_SPEED_MODIFIER,
                        MAKE_LOVE_CLOSE_ENOUGH,
                    ),
                    reset_sniffing,
                )),
            ),
            (
                1,
                Behavior::boxed(OnStart::new(
                    FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)
                        .with_close_enough_distance(|mob| {
                            let baby = mob.as_ageable_mob().is_some_and(AgeableMob::is_baby);
                            if baby {
                                TEMPTED_CLOSE_ENOUGH_BABY
                            } else {
                                TEMPTED_CLOSE_ENOUGH_ADULT
                            }
                        }),
                    reset_sniffing,
                )),
            ),
            (
                2,
                Behavior::boxed(LookAtTargetSink::new(
                    LOOK_AT_TARGET_MIN_DURATION,
                    LOOK_AT_TARGET_MAX_DURATION,
                )),
            ),
            (3, Behavior::boxed(FeelingHappy::new())),
            (
                4,
                Box::new(RunOne::unconditional(vec![
                    (
                        OneShot::boxed(SetWalkTargetFromLookTarget::new(
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                        )),
                        WALK_TO_LOOK_WEIGHT,
                    ),
                    (Behavior::boxed(Scenting::new()), SCENTING_WEIGHT),
                    (Behavior::boxed(Sniffing::new()), SNIFFING_WEIGHT),
                    (
                        OneShot::boxed(SetEntityLookTarget::of_type(
                            &vanilla_entities::PLAYER,
                            MAX_LOOK_DISTANCE,
                        )),
                        LOOK_AT_PLAYER_WEIGHT,
                    ),
                    (
                        OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        STROLL_WEIGHT,
                    ),
                    (
                        Box::new(DoNothing::new(DO_NOTHING_MIN, DO_NOTHING_MAX)),
                        DO_NOTHING_WEIGHT,
                    ),
                ])),
            ),
        ],
    )
    .with_conditions(vec![(
        memory_module_types::SNIFFER_DIGGING.id(),
        MemoryStatus::ValueAbsent,
    )])
}

/// Vanilla parity: `SnifferAi.initSniffingActivity`.
fn sniffing_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Sniff,
        vec![(0, Behavior::boxed(Searching::new()))],
    )
    .with_conditions(vec![
        (
            memory_module_types::IS_PANICKING.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::SNIFFER_SNIFFING_TARGET.id(),
            MemoryStatus::ValuePresent,
        ),
        (
            memory_module_types::WALK_TARGET.id(),
            MemoryStatus::ValuePresent,
        ),
    ])
}

/// Vanilla parity: `SnifferAi.initDigActivity`.
fn dig_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Dig,
        vec![
            (0, Behavior::boxed(Digging::new())),
            (0, Behavior::boxed(FinishedDigging::new())),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::IS_PANICKING.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::WALK_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::SNIFFER_DIGGING.id(),
            MemoryStatus::ValuePresent,
        ),
    ])
}

/// Vanilla parity: `SnifferAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Dig, Activity::Sniff, Activity::Idle]);
}

/// Vanilla parity: `SnifferAi.FeelingHappy`.
struct FeelingHappy;

const FEELING_HAPPY_ENTRY: &[(MemoryModuleId, MemoryStatus)] = &[(
    memory_module_types::SNIFFER_HAPPY.id(),
    MemoryStatus::ValuePresent,
)];

impl FeelingHappy {
    const fn new() -> Self {
        Self
    }
}

impl TimedBehavior for FeelingHappy {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        FEELING_HAPPY_ENTRY
    }

    fn duration(&self) -> (i32, i32) {
        (FEELING_HAPPY_MIN, FEELING_HAPPY_MAX)
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::FeelingHappy);
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Idling);
        }
        ctx.brain()
            .erase_memory(memory_module_types::SNIFFER_HAPPY.id());
    }

    fn debug_name(&self) -> &'static str {
        "SnifferFeelingHappy"
    }
}

/// Vanilla parity: `SnifferAi.Scenting`.
struct Scenting;

const SCENTING_ENTRY: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::IS_PANICKING.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFFER_DIGGING.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFFER_SNIFFING_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFFER_HAPPY.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::BREED_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
];

impl Scenting {
    const fn new() -> Self {
        Self
    }
}

impl TimedBehavior for Scenting {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        SCENTING_ENTRY
    }

    fn duration(&self) -> (i32, i32) {
        (SCENTING_MIN, SCENTING_MAX)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        sniffer_of(ctx).is_some_and(|sniffer| !sniffer.is_tempted())
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Scenting);
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Idling);
        }
    }

    fn debug_name(&self) -> &'static str {
        "SnifferScenting"
    }
}

/// Vanilla parity: `SnifferAi.Sniffing`, which is where a sniffer picks the spot
/// it will walk to and dig.
struct Sniffing;

const SNIFFING_ENTRY: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFFER_SNIFFING_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFF_COOLDOWN.id(),
        MemoryStatus::ValueAbsent,
    ),
];

impl Sniffing {
    const fn new() -> Self {
        Self
    }
}

impl TimedBehavior for Sniffing {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        SNIFFING_ENTRY
    }

    fn duration(&self) -> (i32, i32) {
        (SNIFFING_MIN, SNIFFING_MAX)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        sniffer_of(ctx).is_some_and(|sniffer| !sniffer.is_baby() && sniffer.can_sniff())
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        sniffer_of(ctx).is_some_and(SnifferEntity::can_sniff)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Sniffing);
        }
    }

    fn stop_with_timeout(&mut self, ctx: &BrainContext<'_>, timed_out: bool) {
        let Some(sniffer) = sniffer_of(ctx) else {
            return;
        };
        sniffer.transition_to(SnifferState::Idling);
        // Vanilla only picks a dig spot when the sniff ran its full length; a
        // sniff cut short by a player walking past finds nothing.
        if !timed_out {
            return;
        }

        let Some(position) = sniffer.calculate_dig_position() else {
            return;
        };
        ctx.brain()
            .set_memory(memory_module_types::SNIFFER_SNIFFING_TARGET, position);
        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_block(position, SPEED_MULTIPLIER_WHEN_SNIFFING, 0),
        );
    }

    fn debug_name(&self) -> &'static str {
        "SnifferSniffing"
    }
}

/// Vanilla parity: `SnifferAi.Searching`, the walk to the spot the sniff chose.
struct Searching;

const SEARCHING_ENTRY: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::IS_PANICKING.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFFER_SNIFFING_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
];

impl Searching {
    const fn new() -> Self {
        Self
    }
}

impl TimedBehavior for Searching {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        SEARCHING_ENTRY
    }

    fn duration(&self) -> (i32, i32) {
        (SEARCHING_TIMEOUT, SEARCHING_TIMEOUT)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        sniffer_of(ctx).is_some_and(SnifferEntity::can_sniff)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(sniffer) = sniffer_of(ctx) else {
            return false;
        };
        if !sniffer.can_sniff() {
            sniffer.transition_to(SnifferState::Idling);
            return false;
        }

        // The walk target has to still be the spot the sniff chose; anything
        // that retargeted it means the search is over.
        let walk_target = ctx
            .brain()
            .get_memory(memory_module_types::WALK_TARGET)
            .and_then(|target| target.target().current_block_position());
        let sniffing_target = ctx
            .brain()
            .get_memory(memory_module_types::SNIFFER_SNIFFING_TARGET);
        matches!((walk_target, sniffing_target), (Some(walk), Some(sniff)) if walk == sniff)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Searching);
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx)
            && sniffer.can_dig()
            && sniffer.can_sniff()
        {
            ctx.brain()
                .set_memory(memory_module_types::SNIFFER_DIGGING, true);
        }
        ctx.brain()
            .erase_memory(memory_module_types::WALK_TARGET.id());
        ctx.brain()
            .erase_memory(memory_module_types::SNIFFER_SNIFFING_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "SnifferSearching"
    }
}

/// Vanilla parity: `SnifferAi.Digging`.
struct Digging;

const DIGGING_ENTRY: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::IS_PANICKING.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFFER_DIGGING.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::SNIFF_COOLDOWN.id(),
        MemoryStatus::ValueAbsent,
    ),
];

impl Digging {
    const fn new() -> Self {
        Self
    }
}

impl TimedBehavior for Digging {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        DIGGING_ENTRY
    }

    fn duration(&self) -> (i32, i32) {
        (DIGGING_MIN, DIGGING_MAX)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        sniffer_of(ctx).is_some_and(SnifferEntity::can_sniff)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(sniffer) = sniffer_of(ctx) else {
            return false;
        };
        ctx.brain()
            .has_memory_value(memory_module_types::SNIFFER_DIGGING.id())
            && sniffer.can_dig()
            && !sniffer.is_in_love()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Digging);
        }
    }

    fn stop_with_timeout(&mut self, ctx: &BrainContext<'_>, timed_out: bool) {
        // A dig that ran its full length earns the cooldown; one cut short is
        // simply abandoned, and the sniffer starts scenting again.
        if timed_out {
            ctx.brain().set_memory_with_expiry(
                memory_module_types::SNIFF_COOLDOWN,
                Unit,
                SNIFFING_COOLDOWN_TICKS,
            );
        } else {
            reset_sniffing(ctx);
        }
    }

    fn debug_name(&self) -> &'static str {
        "SnifferDigging"
    }
}

/// Vanilla parity: `SnifferAi.FinishedDigging`.
struct FinishedDigging;

const FINISHED_DIGGING_ENTRY: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::IS_PANICKING.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SNIFFER_DIGGING.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::SNIFF_COOLDOWN.id(),
        MemoryStatus::ValuePresent,
    ),
];

impl FinishedDigging {
    const fn new() -> Self {
        Self
    }
}

impl TimedBehavior for FinishedDigging {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        FINISHED_DIGGING_ENTRY
    }

    fn duration(&self) -> (i32, i32) {
        (FINISHED_DIGGING_DURATION, FINISHED_DIGGING_DURATION)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::SNIFFER_DIGGING.id())
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Rising);
        }
    }

    fn stop_with_timeout(&mut self, ctx: &BrainContext<'_>, finished: bool) {
        if let Some(sniffer) = sniffer_of(ctx) {
            sniffer.transition_to(SnifferState::Idling);
            sniffer.on_digging_complete(finished);
        }
        ctx.brain()
            .erase_memory(memory_module_types::SNIFFER_DIGGING.id());
        ctx.brain()
            .set_memory(memory_module_types::SNIFFER_HAPPY, true);
    }

    fn debug_name(&self) -> &'static str {
        "SnifferFinishedDigging"
    }
}
