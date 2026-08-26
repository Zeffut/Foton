//! The axolotl's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.axolotl.AxolotlAi`. Four
//! activities: the shared core, an idle set that swims and courts and hunts, a
//! fight set, and the play-dead set that overrides everything while the clock
//! runs.

use steel_registry::vanilla_entities;
use steel_utils::value_providers::UniformIntProvider;

use crate::entity::PathfinderMob;
use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, BabyFollowAdult, Behavior, BehaviorControl, CountDownCooldownTicks,
    EraseMemoryIf, FollowTemptation, GateBehavior, LookAtTargetSink, MeleeAttack, MoveToTargetSink,
    OneShot, OrderPolicy, PlayDead, RandomStroll, RunOne, RunningPolicy,
    SetEntityLookTargetSometimes, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, StopAttackingIfTargetInvalid, TriggerIf,
    TryFindWater, ValidatePlayDead,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::entities::AxolotlEntity;
use crate::world::LevelReader as _;

use steel_registry::vanilla_fluid_tags::FluidTag;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;

/// Vanilla parity: `AxolotlAi.ADULT_FOLLOW_RANGE`.
const ADULT_FOLLOW_RANGE: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5,
    max_inclusive: 16,
};

/// Vanilla parity: `AxolotlAi.SPEED_MULTIPLIER_WHEN_MAKING_LOVE`.
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f64 = 0.2;
/// Vanilla parity: `AxolotlAi.SPEED_MULTIPLIER_ON_LAND`.
const SPEED_MULTIPLIER_ON_LAND: f64 = 0.15;
/// Vanilla parity: `AxolotlAi.SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER`.
const SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER: f64 = 0.5;
/// Vanilla parity: `AxolotlAi.SPEED_MULTIPLIER_WHEN_CHASING_IN_WATER`.
const SPEED_MULTIPLIER_WHEN_CHASING_IN_WATER: f64 = 0.6;
/// Vanilla parity: `AxolotlAi.SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT_IN_WATER`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT_IN_WATER: f64 = 0.6;

/// Vanilla parity: the `AnimalMakeLove(AXOLOTL, 0.2F, 2)` of the idle activity.
const MAKE_LOVE_CLOSE_ENOUGH: i32 = 2;

/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;

/// Vanilla parity: the `SetEntityLookTargetSometimes.create(PLAYER, 6.0F, UniformInt.of(30, 60))`.
const GAZE_RANGE: f64 = 6.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};

/// Vanilla parity: the `TryFindWater.create(6, 0.15F)` of the idle activity.
const FIND_WATER_RANGE: i32 = 6;

/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(..., 3)` of the idle
/// gate.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;

/// Vanilla parity: the `MeleeAttack.create(20)` of the fight activity.
const MELEE_COOLDOWN: i64 = 20;

/// How long an axolotl leaves the fish alone after a fight.
///
/// Vanilla parity: the `2400L` expiry `AxolotlAi.updateActivity` puts on
/// `HAS_HUNTING_COOLDOWN`.
const HUNTING_COOLDOWN_TICKS: i64 = 2400;

/// Vanilla parity: the sensor list of `Axolotl.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestAdult,
    SensorType::HurtBy,
    SensorType::AxolotlAttackables,
    SensorType::FoodTemptations,
];

/// Vanilla parity: `Axolotl.BRAIN_PROVIDER` plus `AxolotlAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![
            core_activity(),
            idle_activity(),
            fight_activity(),
            play_dead_activity(),
        ],
    )
}

/// Vanilla parity: `AxolotlAi.initCoreActivity`.
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
            OneShot::boxed(ValidatePlayDead),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::TEMPTATION_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `AxolotlAi.initIdleActivity`.
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
                    &vanilla_entities::AXOLOTL,
                    SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                    MAKE_LOVE_CLOSE_ENOUGH,
                )),
            ),
            (
                2,
                Box::new(RunOne::unconditional(vec![
                    (Behavior::boxed(FollowTemptation::new(speed_modifier)), 1),
                    (
                        OneShot::boxed(BabyFollowAdult::variable(
                            ADULT_FOLLOW_RANGE,
                            speed_modifier_following_adult,
                        )),
                        1,
                    ),
                ])),
            ),
            (3, start_attacking_what_it_hunts()),
            (
                3,
                OneShot::boxed(TryFindWater::new(
                    FIND_WATER_RANGE,
                    SPEED_MULTIPLIER_ON_LAND,
                )),
            ),
            (
                4,
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
                            OneShot::boxed(RandomStroll::swim(
                                SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER,
                            )),
                            2,
                        ),
                        (
                            OneShot::boxed(
                                RandomStroll::stroll(SPEED_MULTIPLIER_ON_LAND).not_from_water(),
                            ),
                            2,
                        ),
                        (
                            OneShot::boxed(SetWalkTargetFromLookTarget::conditional(
                                |body| {
                                    can_set_walk_target_from_look_target(body)
                                        .then(|| speed_modifier(body))
                                },
                                WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                            )),
                            3,
                        ),
                        (
                            OneShot::boxed(TriggerIf::new(
                                "StayInWater",
                                <dyn PathfinderMob>::is_in_water,
                            )),
                            5,
                        ),
                        (
                            OneShot::boxed(TriggerIf::new(
                                "OnGround",
                                <dyn PathfinderMob>::on_ground,
                            )),
                            5,
                        ),
                    ],
                )),
            ),
        ],
    )
}

/// Vanilla parity: `AxolotlAi.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        0,
        vec![
            OneShot::boxed(
                StopAttackingIfTargetInvalid::new().on_erased(|ctx, target| {
                    AxolotlEntity::on_stop_attacking(ctx.mob(), target);
                }),
            ),
            OneShot::boxed(SetWalkTargetFromAttackTargetIfTargetOutOfReach::variable(
                speed_modifier_chasing,
            )),
            OneShot::boxed(MeleeAttack::new(MELEE_COOLDOWN)),
            OneShot::boxed(EraseMemoryIf::new(
                is_breeding,
                memory_module_types::ATTACK_TARGET.id(),
            )),
        ],
    )
    .gated_by(memory_module_types::ATTACK_TARGET.id())
}

/// Vanilla parity: `AxolotlAi.initPlayDeadActivity`.
///
/// A courting axolotl stops playing dead: the `EraseMemoryIf` at priority one
/// is what breaks the act off when a partner turns up.
fn play_dead_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::PlayDead,
        vec![
            (0, Behavior::boxed(PlayDead)),
            (
                1,
                OneShot::boxed(EraseMemoryIf::new(
                    is_breeding,
                    memory_module_types::PLAY_DEAD_TICKS.id(),
                )),
            ),
        ],
    )
    .with_conditions(vec![(
        memory_module_types::PLAY_DEAD_TICKS.id(),
        MemoryStatus::ValuePresent,
    )])
    .erasing_when_stopped(vec![memory_module_types::PLAY_DEAD_TICKS.id()])
}

/// Vanilla parity: the `StartAttacking.create(AxolotlAi::findNearestValidAttackTarget)`
/// of the idle activity. An axolotl busy courting picks no fights.
fn start_attacking_what_it_hunts() -> Box<dyn BehaviorControl> {
    OneShot::boxed(StartAttacking::new(|ctx: &BrainContext<'_>| {
        if is_breeding(ctx) {
            return None;
        }
        ctx.brain()
            .get_memory(memory_module_types::NEAREST_ATTACKABLE)
            .and_then(|memory| memory.get())
    }))
}

/// Vanilla parity: `BehaviorUtils.isBreeding`.
fn is_breeding(ctx: &BrainContext<'_>) -> bool {
    ctx.brain()
        .has_memory_value(memory_module_types::BREED_TARGET.id())
}

/// Vanilla parity: `AxolotlAi.canSetWalkTargetFromLookTarget`.
///
/// An axolotl only walks at what it is looking at when the two of them are on
/// the same side of the surface, so a swimming one does not try to walk up a
/// bank at a player and a beached one does not try to swim at one.
fn can_set_walk_target_from_look_target(body: &dyn PathfinderMob) -> bool {
    let Some(world) = body.level() else {
        return false;
    };
    let Some(look_target) = body
        .brain()
        .and_then(|brain| brain.get_memory(memory_module_types::LOOK_TARGET))
    else {
        return false;
    };
    let Some(pos) = look_target.current_block_position() else {
        return false;
    };

    let is_water_at = world
        .get_block_state(pos)
        .get_fluid_state()
        .fluid_id
        .has_tag(&FluidTag::WATER);
    is_water_at == body.is_in_water()
}

/// Vanilla parity: `AxolotlAi.getSpeedModifierChasing`.
fn speed_modifier_chasing(body: &dyn PathfinderMob) -> f64 {
    if body.is_in_water() {
        SPEED_MULTIPLIER_WHEN_CHASING_IN_WATER
    } else {
        SPEED_MULTIPLIER_ON_LAND
    }
}

/// Vanilla parity: `AxolotlAi.getSpeedModifierFollowingAdult`.
fn speed_modifier_following_adult(body: &dyn PathfinderMob) -> f64 {
    if body.is_in_water() {
        SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT_IN_WATER
    } else {
        SPEED_MULTIPLIER_ON_LAND
    }
}

/// Vanilla parity: `AxolotlAi.getSpeedModifier`.
fn speed_modifier(body: &dyn PathfinderMob) -> f64 {
    if body.is_in_water() {
        SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER
    } else {
        SPEED_MULTIPLIER_ON_LAND
    }
}

/// Vanilla parity: `AxolotlAi.updateActivity`.
///
/// Two things happen here that the shared `setActiveActivityToFirstValid` does
/// not do: an axolotl already playing dead is left alone, and one that has just
/// stopped fighting is put off hunting for two minutes.
pub fn update_activity(brain: &Brain) {
    let old_activity = brain.active_non_core_activity();
    if old_activity == Some(Activity::PlayDead) {
        return;
    }

    brain.set_active_activity_to_first_valid(&[
        Activity::PlayDead,
        Activity::Fight,
        Activity::Idle,
    ]);

    if old_activity == Some(Activity::Fight)
        && brain.active_non_core_activity() != Some(Activity::Fight)
    {
        brain.set_memory_with_expiry(
            memory_module_types::HAS_HUNTING_COOLDOWN,
            true,
            HUNTING_COOLDOWN_TICKS,
        );
    }
}
