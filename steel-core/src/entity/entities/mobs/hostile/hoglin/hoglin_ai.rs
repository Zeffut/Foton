//! The hoglin's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.hoglin.HoglinAi`.

use steel_registry::vanilla_entities;
use steel_utils::value_providers::UniformIntProvider;

use crate::entity::SharedEntity;
use crate::entity::ai::brain::behavior::BehaviorControl;
use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, BabyFollowAdult, BecomePassiveIfMemoryPresent, Behavior, DoNothing,
    EraseMemoryIf, LookAtTargetSink, MeleeAttack, MoveToTargetSink, OneShot, RandomStroll, RunOne,
    SetEntityLookTargetSometimes, SetWalkTargetAwayFrom,
    SetWalkTargetFromAttackTargetIfTargetOutOfReach, SetWalkTargetFromLookTarget, StartAttacking,
    StopAttackingIfTargetInvalid, utils,
};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::{SensorType, is_entity_attackable};
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::{LivingEntity, Mob};
use crate::world::World;

/// Vanilla parity: `HoglinAi.RETREAT_DURATION`, five to twenty seconds.
const RETREAT_DURATION: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5 * 20,
    max_inclusive: 20 * 20,
};
/// Vanilla parity: `HoglinAi.ATTACK_DURATION`.
const ATTACK_DURATION: i64 = 200;
/// Vanilla parity: `HoglinAi.DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING`.
const DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING: i32 = 8;
/// Vanilla parity: `HoglinAi.DESIRED_DISTANCE_FROM_PIGLIN_WHEN_RETREATING`.
const DESIRED_DISTANCE_FROM_PIGLIN_WHEN_RETREATING: i32 = 15;
/// Vanilla parity: `HoglinAi.ATTACK_INTERVAL`.
const ATTACK_INTERVAL: i64 = 40;
/// Vanilla parity: `HoglinAi.BABY_ATTACK_INTERVAL`.
const BABY_ATTACK_INTERVAL: i64 = 15;
/// Vanilla parity: `HoglinAi.REPELLENT_PACIFY_TIME`.
const REPELLENT_PACIFY_TIME: i64 = 200;
/// Vanilla parity: `HoglinAi.ADULT_FOLLOW_RANGE`.
const ADULT_FOLLOW_RANGE: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5,
    max_inclusive: 16,
};
/// Vanilla parity: `HoglinAi.SPEED_MULTIPLIER_WHEN_AVOIDING_REPELLENT`.
const SPEED_MULTIPLIER_WHEN_AVOIDING_REPELLENT: f64 = 1.0;
/// Vanilla parity: `HoglinAi.SPEED_MULTIPLIER_WHEN_RETREATING`.
const SPEED_MULTIPLIER_WHEN_RETREATING: f64 = 1.3;
/// Vanilla parity: `HoglinAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 0.4;
/// Vanilla parity: `HoglinAi.SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f64 = 0.6;
/// Vanilla parity: the `1.0F` of the fight activity's walk behavior.
const SPEED_MULTIPLIER_WHEN_CHASING: f64 = 1.0;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `SetEntityLookTargetSometimes.create(8.0F, UniformInt.of(30, 60))`
/// shared by the idle and retreat activities.
const GAZE_RANGE: f64 = 8.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30,
    max_inclusive: 60,
};
/// Vanilla parity: the `DoNothing(30, 60)` of the idle movement gate.
const IDLE_DO_NOTHING_MIN: i32 = 30;
const IDLE_DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(0.4F, 3)` of the same gate.
const IDLE_LOOK_WALK_CLOSE_ENOUGH: i32 = 3;
/// Vanilla parity: `HoglinAi.SPEED_MULTIPLIER_WHEN_MAKING_LOVE`.
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f64 = 0.6;
/// Vanilla parity: the `2` close-enough distance of the same `AnimalMakeLove`.
const MAKE_LOVE_CLOSE_ENOUGH: i32 = 2;
/// How near a repellent has to be for a hoglin to refuse to walk there.
///
/// Vanilla parity: the `8.0` of `HoglinAi.isPosNearNearestRepellent`, which is
/// also `REPELLENT_DETECTION_RANGE_HORIZONTAL`.
pub const REPELLENT_AVOID_RANGE: f64 = 8.0;

/// The sensors a hoglin runs.
///
/// Vanilla parity: the sensor list of `Hoglin.BRAIN_PROVIDER`.
pub const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::NearestAdult,
    SensorType::HoglinSpecific,
];

/// Builds a hoglin's brain.
///
/// Vanilla parity: `Hoglin.BRAIN_PROVIDER` feeding `HoglinAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![
            core_activity(),
            idle_activity(),
            fight_activity(),
            retreat_activity(),
        ],
    )
}

/// Vanilla parity: `HoglinAi.initCoreActivity`.
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

/// Vanilla parity: `HoglinAi.initIdleActivity`.
///
/// Every behavior of vanilla's list is here.
fn idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            OneShot::boxed(BecomePassiveIfMemoryPresent::new(
                memory_module_types::NEAREST_REPELLENT.id(),
                REPELLENT_PACIFY_TIME,
            )),
            Behavior::boxed(AnimalMakeLove::new(
                &vanilla_entities::HOGLIN,
                SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                MAKE_LOVE_CLOSE_ENOUGH,
            )),
            OneShot::boxed(SetWalkTargetAwayFrom::pos(
                memory_module_types::NEAREST_REPELLENT,
                SPEED_MULTIPLIER_WHEN_AVOIDING_REPELLENT,
                DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING,
                true,
            )),
            OneShot::boxed(StartAttacking::new(find_nearest_valid_attack_target)),
            OneShot::boxed(SetWalkTargetAwayFrom::entity(
                memory_module_types::NEAREST_VISIBLE_ADULT_PIGLIN,
                SPEED_MULTIPLIER_WHEN_IDLING,
                DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING,
                false,
            )),
            OneShot::boxed(SetEntityLookTargetSometimes::any_within(
                GAZE_RANGE,
                GAZE_INTERVAL,
            )),
            OneShot::boxed(BabyFollowAdult::new(
                ADULT_FOLLOW_RANGE,
                SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
            )),
            idle_movement_behaviors(),
        ],
    )
}

/// Vanilla parity: `HoglinAi.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        10,
        vec![
            OneShot::boxed(BecomePassiveIfMemoryPresent::new(
                memory_module_types::NEAREST_REPELLENT.id(),
                REPELLENT_PACIFY_TIME,
            )),
            Behavior::boxed(AnimalMakeLove::new(
                &vanilla_entities::HOGLIN,
                SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                MAKE_LOVE_CLOSE_ENOUGH,
            )),
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
            OneShot::boxed(EraseMemoryIf::new(
                is_breeding,
                memory_module_types::ATTACK_TARGET.id(),
            )),
        ],
    )
    .gated_by(memory_module_types::ATTACK_TARGET.id())
}

/// Vanilla parity: `HoglinAi.initRetreatActivity`.
fn retreat_activity() -> ActivityData {
    ActivityData::create(
        Activity::Avoid,
        10,
        vec![
            OneShot::boxed(SetWalkTargetAwayFrom::entity(
                memory_module_types::AVOID_TARGET,
                SPEED_MULTIPLIER_WHEN_RETREATING,
                DESIRED_DISTANCE_FROM_PIGLIN_WHEN_RETREATING,
                false,
            )),
            idle_movement_behaviors(),
            OneShot::boxed(SetEntityLookTargetSometimes::any_within(
                GAZE_RANGE,
                GAZE_INTERVAL,
            )),
            OneShot::boxed(EraseMemoryIf::new(
                wants_to_stop_fleeing,
                memory_module_types::AVOID_TARGET.id(),
            )),
        ],
    )
    .gated_by(memory_module_types::AVOID_TARGET.id())
}

/// Vanilla parity: `HoglinAi.createIdleMovementBehaviors`.
fn idle_movement_behaviors() -> Box<dyn BehaviorControl> {
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
    ]))
}

/// Picks the activity a hoglin should be in.
///
/// Vanilla parity: `HoglinAi.updateActivity`, minus the activity-change sound,
/// which the mob plays because only it can `makeSound`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Avoid, Activity::Idle]);
}

/// Vanilla parity: `HoglinAi.findNearestValidAttackTarget`. A hoglin pacified
/// by warped fungus, or busy breeding, picks no fight at all.
fn find_nearest_valid_attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    if is_pacified(ctx) || is_breeding(ctx) {
        return None;
    }
    ctx.brain()
        .get_memory(memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
        .and_then(|memory| memory.get())
}

/// Vanilla parity: `HoglinAi.wantsToStopFleeing`.
fn wants_to_stop_fleeing(ctx: &BrainContext<'_>) -> bool {
    !ctx.mob().is_baby() && !piglins_outnumber_hoglins(ctx)
}

/// Vanilla parity: `HoglinAi.piglinsOutnumberHoglins`. The hoglin counts itself.
fn piglins_outnumber_hoglins(ctx: &BrainContext<'_>) -> bool {
    if ctx.mob().is_baby() {
        return false;
    }
    let brain = ctx.brain();
    let piglin_count = brain
        .get_memory(memory_module_types::VISIBLE_ADULT_PIGLIN_COUNT)
        .unwrap_or(0);
    let hoglin_count = brain
        .get_memory(memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT)
        .unwrap_or(0)
        + 1;
    piglin_count > hoglin_count
}

/// Vanilla parity: `HoglinAi.isBreeding`.
fn is_breeding(ctx: &BrainContext<'_>) -> bool {
    ctx.brain()
        .has_memory_value(memory_module_types::BREED_TARGET.id())
}

/// Vanilla parity: `HoglinAi.isPacified`.
pub fn is_pacified(ctx: &BrainContext<'_>) -> bool {
    ctx.brain()
        .has_memory_value(memory_module_types::PACIFIED.id())
}

/// Vanilla parity: `HoglinAi.isPacified`, read straight off a brain.
#[must_use]
pub fn brain_is_pacified(brain: &Brain) -> bool {
    brain.has_memory_value(memory_module_types::PACIFIED.id())
}

/// Vanilla parity: `HoglinAi.onHitTarget`. A grown hoglin that lands a hit on a
/// piglin while the piglins have it outnumbered turns and runs instead of
/// pressing the attack, and takes its neighbors with it.
pub fn on_hit_target(brain: &Brain, body: &dyn LivingEntity, target: &SharedEntity) {
    if body.is_baby() {
        return;
    }
    let target_is_piglin = utils::is_of_type(target.as_ref(), &vanilla_entities::PIGLIN);
    if target_is_piglin && piglins_outnumber_hoglins_on(brain, body) {
        set_avoid_target(brain, target);
        broadcast_retreat(brain, body, target);
    } else {
        broadcast_attack_target(brain, body, target);
    }
}

/// The brain-only half of [`piglins_outnumber_hoglins`], for the mob-side calls.
fn piglins_outnumber_hoglins_on(brain: &Brain, body: &dyn LivingEntity) -> bool {
    if body.is_baby() {
        return false;
    }
    let piglin_count = brain
        .get_memory(memory_module_types::VISIBLE_ADULT_PIGLIN_COUNT)
        .unwrap_or(0);
    let hoglin_count = brain
        .get_memory(memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT)
        .unwrap_or(0)
        + 1;
    piglin_count > hoglin_count
}

/// Vanilla parity: `HoglinAi.setAvoidTarget`.
fn set_avoid_target(brain: &Brain, avoid_target: &SharedEntity) {
    brain.erase_memory(memory_module_types::ATTACK_TARGET.id());
    brain.erase_memory(memory_module_types::WALK_TARGET.id());
    brain.set_memory_with_expiry(
        memory_module_types::AVOID_TARGET,
        utils::remember(avoid_target),
        i64::from(rand::random_range(
            RETREAT_DURATION.min_inclusive..=RETREAT_DURATION.max_inclusive,
        )),
    );
}

/// Vanilla parity: `HoglinAi.setAttackTarget`.
fn set_attack_target(brain: &Brain, target: &SharedEntity) {
    brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
    brain.erase_memory(memory_module_types::BREED_TARGET.id());
    brain.set_memory_with_expiry(
        memory_module_types::ATTACK_TARGET,
        utils::remember(target),
        ATTACK_DURATION,
    );
}

/// Vanilla parity: `HoglinAi.broadcastRetreat`.
fn broadcast_retreat(brain: &Brain, body: &dyn LivingEntity, target: &SharedEntity) {
    for_each_visible_adult_hoglin(brain, |neighbor_brain, _| {
        retreat_from_nearest_target(neighbor_brain, body, target);
    });
}

/// Vanilla parity: `HoglinAi.retreatFromNearestTarget`.
fn retreat_from_nearest_target(
    brain: &Brain,
    body: &dyn LivingEntity,
    new_avoid_target: &SharedEntity,
) {
    let mut nearest = new_avoid_target.clone();
    nearest = utils::nearest_target(
        body.as_entity_event_source(),
        brain
            .get_memory(memory_module_types::AVOID_TARGET)
            .and_then(|memory| memory.get()),
        nearest,
    );
    nearest = utils::nearest_target(
        body.as_entity_event_source(),
        brain
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get()),
        nearest,
    );
    set_avoid_target(brain, &nearest);
}

/// Vanilla parity: `HoglinAi.broadcastAttackTarget`.
fn broadcast_attack_target(brain: &Brain, body: &dyn LivingEntity, target: &SharedEntity) {
    for_each_visible_adult_hoglin(brain, |neighbor_brain, _| {
        if brain_is_pacified(neighbor_brain) {
            return;
        }
        let nearest = utils::nearest_target(
            body.as_entity_event_source(),
            neighbor_brain
                .get_memory(memory_module_types::ATTACK_TARGET)
                .and_then(|memory| memory.get()),
            target.clone(),
        );
        set_attack_target(neighbor_brain, &nearest);
    });
}

/// Runs `action` against every visible adult hoglin's own brain.
///
/// Vanilla parity: the `getVisibleAdultHoglins(body).forEach(...)` the two
/// broadcasts share. Vanilla's list is typed `List<Hoglin>`, so it can call
/// straight through; Steel remembers entities untyped and reaches the brain
/// through [`Mob::brain`].
fn for_each_visible_adult_hoglin(brain: &Brain, mut action: impl FnMut(&Brain, &SharedEntity)) {
    let Some(neighbors) = brain.get_memory(memory_module_types::NEAREST_VISIBLE_ADULT_HOGLINS)
    else {
        return;
    };
    for remembered in neighbors {
        let Some(entity) = remembered.get() else {
            continue;
        };
        let Some(neighbor_brain) = entity.as_mob().and_then(Mob::brain) else {
            continue;
        };
        action(neighbor_brain, &entity);
    }
}

/// Vanilla parity: `HoglinAi.wasHurtBy`.
pub fn was_hurt_by(world: &World, brain: &Brain, body: &dyn LivingEntity, attacker: &SharedEntity) {
    brain.erase_memory(memory_module_types::PACIFIED.id());
    brain.erase_memory(memory_module_types::BREED_TARGET.id());
    if body.is_baby() {
        retreat_from_nearest_target(brain, body, attacker);
        return;
    }
    maybe_retaliate(world, brain, body, attacker);
}

/// Vanilla parity: the private `HoglinAi.maybeRetaliate`.
fn maybe_retaliate(world: &World, brain: &Brain, body: &dyn LivingEntity, attacker: &SharedEntity) {
    let attacker_is_piglin = utils::is_of_type(attacker.as_ref(), &vanilla_entities::PIGLIN);
    if brain.is_active(Activity::Avoid) && attacker_is_piglin {
        return;
    }
    if utils::is_of_type(attacker.as_ref(), &vanilla_entities::HOGLIN) {
        return;
    }
    if utils::is_other_target_much_further_away_than_current_attack_target(
        brain,
        body.as_entity_event_source(),
        attacker.as_ref(),
        4.0,
    ) {
        return;
    }
    let Some(living_attacker) = attacker.as_living_entity() else {
        return;
    };
    if !is_entity_attackable(world, body, living_attacker) {
        return;
    }
    set_attack_target(brain, attacker);
    broadcast_attack_target(brain, body, attacker);
}
