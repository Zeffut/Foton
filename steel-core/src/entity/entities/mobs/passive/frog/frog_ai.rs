//! The frog's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.frog.FrogAi`. Six
//! activities: the shared core, an idle set for land, a swim set for water, the
//! spawn-laying set a bred frog runs, the tongue, and the long jump.

use steel_registry::{sound_events, vanilla_blocks, vanilla_entities};
use steel_utils::BlockPos;
use steel_utils::value_providers::UniformIntProvider;

use crate::entity::PathfinderMob;
use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, AnimalPanic, Behavior, BehaviorControl, CountDownCooldownTicks, Croak,
    FollowTemptation, GateBehavior, LongJumpMidJump, LongJumpToRandomPos, LookAtTargetSink,
    MoveToTargetSink, OneShot, OrderPolicy, RandomStroll, RunOne, RunningPolicy,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, ShootTongue, StartAttacking,
    StopAttackingIfTargetInvalid, TriggerIf, TryFindLand, TryFindLandNearWater,
    TryLaySpawnOnFluidNearLand, default_acceptable_landing_spot, frog_prefer_jump_to,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::ai::path::{PathType, PathfindingContext};
use crate::entity::ai::walk::WalkPathEvaluator;
use crate::world::LevelReader as _;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;

/// Vanilla parity: `FrogAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 2.0;
/// Vanilla parity: `FrogAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 1.0;
/// Vanilla parity: `FrogAi.SPEED_MULTIPLIER_ON_LAND`.
const SPEED_MULTIPLIER_ON_LAND: f64 = 1.0;
/// Vanilla parity: `FrogAi.SPEED_MULTIPLIER_IN_WATER`.
const SPEED_MULTIPLIER_IN_WATER: f64 = 0.75;
/// Vanilla parity: `FrogAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 1.25;
/// Vanilla parity: the `1.5F` of the swim activity's `TryFindLand`.
const SPEED_MULTIPLIER_LEAVING_WATER: f64 = 1.5;

/// Vanilla parity: `FrogAi.TIME_BETWEEN_LONG_JUMPS`.
const TIME_BETWEEN_LONG_JUMPS: UniformIntProvider = UniformIntProvider {
    min_inclusive: 100,
    max_inclusive: 140,
};
/// Vanilla parity: `FrogAi.MAX_LONG_JUMP_HEIGHT`.
const MAX_LONG_JUMP_HEIGHT: i32 = 2;
/// Vanilla parity: `FrogAi.MAX_LONG_JUMP_WIDTH`.
const MAX_LONG_JUMP_WIDTH: i32 = 4;
/// Vanilla parity: `FrogAi.MAX_JUMP_VELOCITY_MULTIPLIER`.
const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;
/// Vanilla parity: the `0.5F` chance of preferring a lily pad to land on.
const PREFERRED_BLOCK_CHANCE: f32 = 0.5;

/// Vanilla parity: the defaults of the one-argument `AnimalMakeLove(EntityType)`.
const MAKE_LOVE_SPEED_MODIFIER: f64 = 1.0;
const MAKE_LOVE_CLOSE_ENOUGH: i32 = 1;

/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;

/// Vanilla parity: the `SetEntityLookTargetSometimes.create(PLAYER, 6.0F, UniformInt.of(30, 60))`.
const GAZE_RANGE: f64 = 6.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};

/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(1.0F, 3)` the idle,
/// swim and lay-spawn gates share.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;

/// Vanilla parity: the `TryFindLand.create(6, 1.0F)` of the idle activity.
const IDLE_FIND_LAND_RANGE: i32 = 6;
/// Vanilla parity: the `TryFindLand.create(8, 1.5F)` of the swim activity.
const SWIM_FIND_LAND_RANGE: i32 = 8;
/// Vanilla parity: the `TryFindLandNearWater.create(8, 1.0F)` of the lay-spawn
/// activity.
const LAY_SPAWN_FIND_LAND_RANGE: i32 = 8;

/// Vanilla parity: the sensor list of `Frog.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::HurtBy,
    SensorType::FrogAttackables,
    SensorType::FrogTemptations,
    SensorType::IsInWater,
];

/// Vanilla parity: `Frog.BRAIN_PROVIDER` plus `FrogAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![
            core_activity(),
            idle_activity(),
            swim_activity(),
            lay_spawn_activity(),
            tongue_activity(),
            jump_activity(),
        ],
    )
}

/// Vanilla parity: `FrogAi.initMemories`, which staggers the first jump so a
/// pond full of frogs does not launch on the same tick.
pub fn init_memories(brain: &Brain) {
    brain.set_memory(
        memory_module_types::LONG_JUMP_COOLDOWN_TICKS,
        rand::random_range(
            TIME_BETWEEN_LONG_JUMPS.min_inclusive..=TIME_BETWEEN_LONG_JUMPS.max_inclusive,
        ),
    );
}

/// Vanilla parity: `FrogAi.initCoreActivity`.
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
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::LONG_JUMP_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `FrogAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (0, gaze_at_players()),
            (
                0,
                Behavior::boxed(AnimalMakeLove::new(
                    &vanilla_entities::FROG,
                    MAKE_LOVE_SPEED_MODIFIER,
                    MAKE_LOVE_CLOSE_ENOUGH,
                )),
            ),
            (
                1,
                Behavior::boxed(FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)),
            ),
            (2, start_attacking_what_it_can_eat()),
            (
                3,
                OneShot::boxed(TryFindLand::new(
                    IDLE_FIND_LAND_RANGE,
                    SPEED_MULTIPLIER_ON_LAND,
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
                        (Behavior::boxed(Croak::new()), 3),
                        (
                            OneShot::boxed(TriggerIf::new(
                                "OnGround",
                                <dyn PathfinderMob>::on_ground,
                            )),
                            2,
                        ),
                    ],
                )),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::LONG_JUMP_MID_JUMP.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::IS_IN_WATER.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: `FrogAi.initSwimActivity`.
fn swim_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Swim,
        vec![
            (0, gaze_at_players()),
            (
                1,
                Behavior::boxed(FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)),
            ),
            (2, start_attacking_what_it_can_eat()),
            (
                3,
                OneShot::boxed(TryFindLand::new(
                    SWIM_FIND_LAND_RANGE,
                    SPEED_MULTIPLIER_LEAVING_WATER,
                )),
            ),
            (
                5,
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
                            OneShot::boxed(RandomStroll::swim(SPEED_MULTIPLIER_IN_WATER)),
                            1,
                        ),
                        (
                            OneShot::boxed(
                                RandomStroll::stroll(SPEED_MULTIPLIER_ON_LAND).not_from_water(),
                            ),
                            1,
                        ),
                        (
                            OneShot::boxed(SetWalkTargetFromLookTarget::new(
                                SPEED_MULTIPLIER_ON_LAND,
                                WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                            )),
                            1,
                        ),
                        (
                            OneShot::boxed(TriggerIf::new(
                                "StayInWater",
                                <dyn PathfinderMob>::is_in_water,
                            )),
                            5,
                        ),
                    ],
                )),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::LONG_JUMP_MID_JUMP.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::IS_IN_WATER.id(),
            MemoryStatus::ValuePresent,
        ),
    ])
}

/// Vanilla parity: `FrogAi.initLaySpawnActivity`, the near end of the frogspawn
/// loop.
fn lay_spawn_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::LaySpawn,
        vec![
            (0, gaze_at_players()),
            (1, start_attacking_what_it_can_eat()),
            (
                2,
                OneShot::boxed(TryFindLandNearWater::new(
                    LAY_SPAWN_FIND_LAND_RANGE,
                    SPEED_MULTIPLIER_ON_LAND,
                )),
            ),
            (
                3,
                OneShot::boxed(TryLaySpawnOnFluidNearLand::new(&vanilla_blocks::FROGSPAWN)),
            ),
            (
                4,
                Box::new(RunOne::unconditional(vec![
                    (
                        OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        2,
                    ),
                    (
                        OneShot::boxed(SetWalkTargetFromLookTarget::new(
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                        )),
                        1,
                    ),
                    (Behavior::boxed(Croak::new()), 2),
                    (
                        OneShot::boxed(TriggerIf::new("OnGround", <dyn PathfinderMob>::on_ground)),
                        1,
                    ),
                ])),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::LONG_JUMP_MID_JUMP.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::IS_PREGNANT.id(),
            MemoryStatus::ValuePresent,
        ),
    ])
}

/// Vanilla parity: `FrogAi.initTongueActivity`.
fn tongue_activity() -> ActivityData {
    ActivityData::create(
        Activity::Tongue,
        0,
        vec![
            OneShot::boxed(StopAttackingIfTargetInvalid::new()),
            Behavior::boxed(ShootTongue::new(
                &sound_events::ENTITY_FROG_TONGUE,
                &sound_events::ENTITY_FROG_EAT,
            )),
        ],
    )
    .gated_by(memory_module_types::ATTACK_TARGET.id())
}

/// Vanilla parity: `FrogAi.initJumpActivity`.
fn jump_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::LongJump,
        vec![
            (
                0,
                Behavior::boxed(LongJumpMidJump::new(
                    TIME_BETWEEN_LONG_JUMPS,
                    &sound_events::ENTITY_FROG_STEP,
                )),
            ),
            (
                1,
                Behavior::boxed(
                    LongJumpToRandomPos::new(
                        TIME_BETWEEN_LONG_JUMPS,
                        MAX_LONG_JUMP_HEIGHT,
                        MAX_LONG_JUMP_WIDTH,
                        MAX_JUMP_VELOCITY_MULTIPLIER,
                        &sound_events::ENTITY_FROG_LONG_JUMP,
                    )
                    .preferring(frog_prefer_jump_to(), PREFERRED_BLOCK_CHANCE)
                    .with_acceptable_landing_spot(is_acceptable_landing_spot),
                ),
            ),
        ],
    )
    .with_conditions(vec![
        (
            memory_module_types::TEMPTING_PLAYER.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::BREED_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::LONG_JUMP_COOLDOWN_TICKS.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::IS_IN_WATER.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: the `SetEntityLookTargetSometimes` every activity opens with.
fn gaze_at_players() -> Box<dyn BehaviorControl> {
    OneShot::boxed(SetEntityLookTargetSometimes::of_type(
        &vanilla_entities::PLAYER,
        GAZE_RANGE,
        GAZE_INTERVAL,
    ))
}

/// Vanilla parity: the `StartAttacking.create(FrogAi::canAttack, NEAREST_ATTACKABLE)`
/// three activities share. A frog busy courting does not hunt.
fn start_attacking_what_it_can_eat() -> Box<dyn BehaviorControl> {
    OneShot::boxed(StartAttacking::new(|ctx: &BrainContext<'_>| {
        if ctx
            .brain()
            .has_memory_value(memory_module_types::BREED_TARGET.id())
        {
            return None;
        }
        ctx.brain()
            .get_memory(memory_module_types::NEAREST_ATTACKABLE)
            .and_then(|memory| memory.get())
    }))
}

/// Vanilla parity: `FrogAi.isAcceptableLandingSpot`.
///
/// A frog will land on a lily pad or a big dripleaf without asking anything
/// else; anywhere else it wants dry ground it could have stood on.
fn is_acceptable_landing_spot(body: &dyn PathfinderMob, target_pos: BlockPos) -> bool {
    let Some(world) = body.level() else {
        return false;
    };
    let below = target_pos.below();

    let dry = world
        .get_block_state(target_pos)
        .get_fluid_state()
        .is_empty()
        && world.get_block_state(below).get_fluid_state().is_empty()
        && world
            .get_block_state(target_pos.above())
            .get_fluid_state()
            .is_empty();
    if !dry {
        return false;
    }

    let state = world.get_block_state(target_pos);
    let below_state = world.get_block_state(below);
    if state.get_block().has_tag(&BlockTag::FROG_PREFER_JUMP_TO)
        || below_state
            .get_block()
            .has_tag(&BlockTag::FROG_PREFER_JUMP_TO)
    {
        return true;
    }

    let mut context = PathfindingContext::new(world.as_ref(), body.block_position());
    let path_type = WalkPathEvaluator::path_type_static(&mut context, target_pos);
    let path_type_below = WalkPathEvaluator::path_type_static(&mut context, below);
    if path_type == PathType::Trapdoor || (state.is_air() && path_type_below == PathType::Trapdoor)
    {
        return true;
    }

    default_acceptable_landing_spot(body, target_pos)
}

/// Vanilla parity: `FrogAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[
        Activity::Tongue,
        Activity::LaySpawn,
        Activity::LongJump,
        Activity::Swim,
        Activity::Idle,
    ]);
}
