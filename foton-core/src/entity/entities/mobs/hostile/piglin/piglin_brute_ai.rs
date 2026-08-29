//! The piglin brute's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.piglin.PiglinBruteAi`.

use std::sync::Arc;

use foton_registry::vanilla_entities;
use foton_utils::GlobalPos;

use crate::entity::LivingEntity;
use crate::entity::ai::brain::behavior::BehaviorControl;
use crate::entity::ai::brain::behavior::{
    Behavior, DoNothing, InteractWith, LookAtTargetSink, MeleeAttack, MoveToTargetSink, OneShot,
    RandomStroll, RunOne, SetEntityLookTarget, SetLookAndInteract,
    SetWalkTargetFromAttackTargetIfTargetOutOfReach, StartAttacking, StopAttackingIfTargetInvalid,
    StopBeingAngryIfTargetDead, StrollAroundPoi, StrollToPoi, utils,
};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::sensor::follow_range;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::{PathfinderMob, SharedEntity};
use crate::world::World;

/// Vanilla parity: `PiglinBruteAi.MELEE_ATTACK_COOLDOWN`.
const MELEE_ATTACK_COOLDOWN: i64 = 20;
/// Vanilla parity: `PiglinBruteAi.ACTIVITY_SOUND_LIKELIHOOD_PER_TICK`.
const ACTIVITY_SOUND_LIKELIHOOD_PER_TICK: f32 = 0.0125;
/// Vanilla parity: `PiglinBruteAi.MAX_LOOK_DIST`.
const MAX_LOOK_DIST: f64 = 8.0;
/// Vanilla parity: `PiglinBruteAi.INTERACTION_RANGE`.
const INTERACTION_RANGE: i32 = 8;
/// Vanilla parity: `PiglinBruteAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 0.6;
/// Vanilla parity: `PiglinBruteAi.HOME_CLOSE_ENOUGH_DISTANCE`.
const HOME_CLOSE_ENOUGH_DISTANCE: i32 = 2;
/// Vanilla parity: `PiglinBruteAi.HOME_TOO_FAR_DISTANCE`.
const HOME_TOO_FAR_DISTANCE: i32 = 100;
/// Vanilla parity: `PiglinBruteAi.HOME_STROLL_AROUND_DISTANCE`.
const HOME_STROLL_AROUND_DISTANCE: i32 = 5;
/// Vanilla parity: the `1.0F` chase speed of the fight activity.
const SPEED_MULTIPLIER_WHEN_CHASING: f64 = 1.0;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `DoNothing(30, 60)` of both idle gates.
const IDLE_DO_NOTHING_MIN: i32 = 30;
const IDLE_DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `2` stop distance of the idle `InteractWith` calls.
const INTERACT_STOP_DISTANCE: i32 = 2;
/// Vanilla parity: the `4` range of `SetLookAndInteract.create(PLAYER, 4)`.
const LOOK_AND_INTERACT_RANGE: i32 = 4;

/// The sensors a piglin brute runs.
///
/// Vanilla parity: the sensor list of `PiglinBrute.BRAIN_PROVIDER`.
pub const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::NearestItems,
    SensorType::HurtBy,
    SensorType::PiglinBruteSpecific,
];

/// Builds a piglin brute's brain.
///
/// Vanilla parity: `PiglinBrute.BRAIN_PROVIDER` feeding
/// `PiglinBruteAi.getActivities`.
///
/// **Missing behavior**: vanilla's core activity also holds `InteractWithDoor`,
/// which Foton has no port of yet; the navigation flag that lets the brute walk
/// through an open door is set either way.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![core_activity(), idle_activity(), fight_activity()],
    )
}

/// Pins the brute to where it spawned.
///
/// Vanilla parity: `PiglinBruteAi.initMemories`, which is what keeps a bastion's
/// brutes inside the bastion.
pub fn init_memories(brain: &Brain, home: GlobalPos) {
    brain.set_memory(memory_module_types::HOME, home);
}

/// Vanilla parity: `PiglinBruteAi.initCoreActivity`.
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
            OneShot::boxed(StopBeingAngryIfTargetDead),
        ],
    )
}

/// Vanilla parity: `PiglinBruteAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            OneShot::boxed(StartAttacking::new(find_nearest_valid_attack_target)),
            idle_look_behaviors(),
            idle_movement_behaviors(),
            OneShot::boxed(SetLookAndInteract::new(
                &vanilla_entities::PLAYER,
                LOOK_AND_INTERACT_RANGE,
            )),
        ],
    )
}

/// Vanilla parity: `PiglinBruteAi.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        10,
        vec![
            OneShot::boxed(
                StopAttackingIfTargetInvalid::new()
                    .when(|ctx, target| !is_nearest_valid_attack_target(ctx, target)),
            ),
            OneShot::boxed(SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(
                SPEED_MULTIPLIER_WHEN_CHASING,
            )),
            OneShot::boxed(MeleeAttack::new(MELEE_ATTACK_COOLDOWN)),
        ],
    )
    .gated_by(memory_module_types::ATTACK_TARGET.id())
}

/// Vanilla parity: `PiglinBruteAi.createIdleLookBehaviors`.
fn idle_look_behaviors() -> Box<dyn BehaviorControl> {
    Box::new(RunOne::unconditional(vec![
        (
            OneShot::boxed(SetEntityLookTarget::of_type(
                &vanilla_entities::PLAYER,
                MAX_LOOK_DIST,
            )),
            1,
        ),
        (
            OneShot::boxed(SetEntityLookTarget::of_type(
                &vanilla_entities::PIGLIN,
                MAX_LOOK_DIST,
            )),
            1,
        ),
        (
            OneShot::boxed(SetEntityLookTarget::of_type(
                &vanilla_entities::PIGLIN_BRUTE,
                MAX_LOOK_DIST,
            )),
            1,
        ),
        (
            OneShot::boxed(SetEntityLookTarget::any_within(MAX_LOOK_DIST)),
            1,
        ),
        (
            Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
            1,
        ),
    ]))
}

/// Vanilla parity: `PiglinBruteAi.createIdleMovementBehaviors`.
fn idle_movement_behaviors() -> Box<dyn BehaviorControl> {
    Box::new(RunOne::unconditional(vec![
        (
            OneShot::boxed(RandomStroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
            2,
        ),
        (
            OneShot::boxed(InteractWith::of(
                &vanilla_entities::PIGLIN,
                INTERACTION_RANGE,
                memory_module_types::INTERACTION_TARGET,
                SPEED_MULTIPLIER_WHEN_IDLING,
                INTERACT_STOP_DISTANCE,
            )),
            2,
        ),
        (
            OneShot::boxed(InteractWith::of(
                &vanilla_entities::PIGLIN_BRUTE,
                INTERACTION_RANGE,
                memory_module_types::INTERACTION_TARGET,
                SPEED_MULTIPLIER_WHEN_IDLING,
                INTERACT_STOP_DISTANCE,
            )),
            2,
        ),
        (
            OneShot::boxed(StrollToPoi::new(
                memory_module_types::HOME,
                SPEED_MULTIPLIER_WHEN_IDLING,
                HOME_CLOSE_ENOUGH_DISTANCE,
                HOME_TOO_FAR_DISTANCE,
            )),
            2,
        ),
        (
            OneShot::boxed(StrollAroundPoi::new(
                memory_module_types::HOME,
                SPEED_MULTIPLIER_WHEN_IDLING,
                HOME_STROLL_AROUND_DISTANCE,
            )),
            2,
        ),
        (
            Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
            1,
        ),
    ]))
}

/// Picks the activity a brute should be in.
///
/// Vanilla parity: `PiglinBruteAi.updateActivity`. It returns whether the
/// activity changed, because the angry sound is the mob's to play.
pub fn update_activity(brain: &Brain) -> bool {
    let old_activity = brain.active_non_core_activity();
    brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
    old_activity != brain.active_non_core_activity()
}

/// Whether the brute should grunt this tick.
///
/// Vanilla parity: `PiglinBruteAi.maybePlayActivitySound`, a one-in-eighty roll
/// which -- with `playActivitySound` only speaking while fighting -- is what
/// makes a bastion's brutes growl irregularly during a fight and never
/// otherwise.
#[must_use]
pub fn should_play_activity_sound(brain: &Brain) -> bool {
    rand::random::<f32>() < ACTIVITY_SOUND_LIKELIHOOD_PER_TICK
        && brain.active_non_core_activity() == Some(Activity::Fight)
}

/// Vanilla parity: the private `PiglinBruteAi.findNearestValidAttackTarget`.
fn find_nearest_valid_attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    let brain = ctx.brain();
    if let Some(angry_at) =
        utils::living_entity_from_uuid_memory(ctx.world(), brain, memory_module_types::ANGRY_AT)
        && angry_at.as_living_entity().is_some_and(|living| {
            TargetingConditions::for_combat()
                .range(follow_range(ctx.mob()))
                .ignore_line_of_sight()
                .test(ctx.world(), Some(ctx.mob()), living)
        })
    {
        return Some(angry_at);
    }

    if let Some(player) = brain
        .get_memory(memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
        .and_then(|memory| memory.get())
    {
        return Some(player);
    }
    brain
        .get_memory(memory_module_types::NEAREST_VISIBLE_NEMESIS)
        .and_then(|memory| memory.get())
}

/// Vanilla parity: the private `PiglinBruteAi.isNearestValidAttackTarget`.
fn is_nearest_valid_attack_target(ctx: &BrainContext<'_>, target: &SharedEntity) -> bool {
    find_nearest_valid_attack_target(ctx).is_some_and(|nearest| nearest.id() == target.id())
}

/// Reacts to being hit.
///
/// Vanilla parity: `PiglinBruteAi.wasHurtBy`, which ignores any piglin -- a
/// brute never turns on its own kind.
pub fn was_hurt_by(
    world: &Arc<World>,
    brain: &Brain,
    body: &dyn PathfinderMob,
    attacker: &SharedEntity,
    attacker_living: &dyn LivingEntity,
) {
    if utils::is_of_type(attacker.as_ref(), &vanilla_entities::PIGLIN)
        || utils::is_of_type(attacker.as_ref(), &vanilla_entities::PIGLIN_BRUTE)
    {
        return;
    }
    super::piglin_ai::maybe_retaliate(world, brain, body, attacker, attacker_living);
}

// Vanilla's `PiglinBruteAi.setAngerTarget` is `protected` and called from
// nowhere upstream; a brute's anger is set through `PiglinAi.setAngerTarget`
// by way of `maybeRetaliate`. It is left out rather than ported as dead code.
