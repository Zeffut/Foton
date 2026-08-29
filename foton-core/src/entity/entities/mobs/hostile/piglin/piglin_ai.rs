//! The piglin's brain and the free functions around it.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.piglin.PiglinAi`.

use std::cell::Cell;
use std::ptr;
use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_game_rules::UNIVERSAL_ANGER;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_entities, vanilla_items,
    vanilla_loot_tables,
};
use foton_utils::Downcast as _;
use foton_utils::types::InteractionHand;
use foton_utils::value_providers::UniformIntProvider;
use glam::DVec3;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::behavior::{
    BackUpIfTooClose, Behavior, CopyMemoryWithExpiry, CrossbowAttack, DismountOrSkipMounting,
    DoNothing, EraseMemoryIf, GoToTargetLocation, GoToWantedItem, InteractWith, LookAtTargetSink,
    MeleeAttack, Mount, MoveToTargetSink, OneShot, RandomStroll, RunOne, SetEntityLookTarget,
    SetLookAndInteract, SetWalkTargetAwayFrom, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, StartCelebratingIfTargetDead,
    StopAttackingIfTargetInvalid, StopBeingAngryIfTargetDead, Trigger, TriggerGate, utils,
};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::{SensorType, is_entity_attackable, is_zombified};

use super::super::piglin_predicates::is_player_holding_loved_item;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::{Entity as _, LivingEntity, Mob, PathfinderMob, SharedEntity};
use crate::player::Player;
use crate::world::World;

use super::behaviors::{
    RememberIfHoglinWasKilled, StartAdmiringItemIfSeen, StartHuntingHoglin,
    StopAdmiringIfItemTooFarAway, StopAdmiringIfTiredOfTryingToReachItem,
    StopHoldingItemIfNoLongerAdmiring,
};
use super::entity::PiglinEntity;
use crate::entity::ai::brain::behavior::{BehaviorControl, BehaviorStatus};
use crate::entity::ai::brain::memory::MemoryModuleId;
use crate::entity::ai::brain::sensor::follow_range;
use crate::entity::ai::goal::land_random_pos;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::entities::{HoglinEntity, ItemEntity};
use crate::entity::{RemovalReason, barter_loot_items};

/// Returns whether `item_stack` is what a piglin will barter for.
///
/// Vanilla parity: `PiglinAi.BARTERING_ITEM` plus `isBarterCurrency` -- a gold
/// ingot and nothing else.
#[must_use]
pub fn is_barter_currency(item_stack: &ItemStack) -> bool {
    item_stack.is(&vanilla_items::GOLD_INGOT)
}

/// Vanilla parity: `PiglinAi.PLAYER_ANGER_RANGE`.
const PLAYER_ANGER_RANGE: f64 = 16.0;
/// Vanilla parity: `PiglinAi.ANGER_DURATION`.
const ANGER_DURATION: i64 = 600;
/// Vanilla parity: `PiglinAi.ADMIRE_DURATION`.
const ADMIRE_DURATION: i64 = 119;
/// Vanilla parity: `PiglinAi.MAX_DISTANCE_TO_WALK_TO_ITEM`.
const MAX_DISTANCE_TO_WALK_TO_ITEM: i32 = 9;
/// Vanilla parity: `PiglinAi.MAX_TIME_TO_WALK_TO_ITEM`.
const MAX_TIME_TO_WALK_TO_ITEM: i32 = 200;
/// Vanilla parity: `PiglinAi.HOW_LONG_TIME_TO_DISABLE_ADMIRE_WALKING_IF_CANT_REACH_ITEM`.
const DISABLE_ADMIRE_WALKING_TIME: i64 = 200;
/// Vanilla parity: `PiglinAi.CELEBRATION_TIME`.
const CELEBRATION_TIME: i64 = 300;
/// Vanilla parity: `PiglinAi.TIME_BETWEEN_HUNTS`, thirty to a hundred and twenty seconds.
const TIME_BETWEEN_HUNTS: UniformIntProvider = UniformIntProvider {
    min_inclusive: 30 * 20,
    max_inclusive: 120 * 20,
};
/// Vanilla parity: `PiglinAi.BABY_FLEE_DURATION_AFTER_GETTING_HIT`.
const BABY_FLEE_DURATION_AFTER_GETTING_HIT: i64 = 100;
/// Vanilla parity: `PiglinAi.HIT_BY_PLAYER_MEMORY_TIMEOUT`.
const HIT_BY_PLAYER_MEMORY_TIMEOUT: i64 = 400;
/// Vanilla parity: `PiglinAi.MAX_WALK_DISTANCE_TO_START_RIDING`.
const MAX_WALK_DISTANCE_TO_START_RIDING: i32 = 8;
/// Vanilla parity: `PiglinAi.RIDE_START_INTERVAL`, ten to forty seconds.
const RIDE_START_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 10 * 20,
    max_inclusive: 40 * 20,
};
/// Vanilla parity: `PiglinAi.RIDE_DURATION`, ten to thirty seconds.
const RIDE_DURATION: UniformIntProvider = UniformIntProvider {
    min_inclusive: 10 * 20,
    max_inclusive: 30 * 20,
};
/// Vanilla parity: `PiglinAi.RETREAT_DURATION`, five to twenty seconds.
const RETREAT_DURATION: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5 * 20,
    max_inclusive: 20 * 20,
};
/// Vanilla parity: `PiglinAi.MELEE_ATTACK_COOLDOWN`.
const MELEE_ATTACK_COOLDOWN: i64 = 20;
/// Vanilla parity: `PiglinAi.EAT_COOLDOWN`.
const EAT_COOLDOWN: i64 = 200;
/// Vanilla parity: `PiglinAi.DESIRED_DISTANCE_FROM_ENTITY_WHEN_AVOIDING`.
const DESIRED_DISTANCE_FROM_ENTITY_WHEN_AVOIDING: i32 = 12;
/// Vanilla parity: `PiglinAi.MAX_LOOK_DIST`.
const MAX_LOOK_DIST: f64 = 8.0;
/// Vanilla parity: `PiglinAi.MAX_LOOK_DIST_FOR_PLAYER_HOLDING_LOVED_ITEM`.
const MAX_LOOK_DIST_FOR_PLAYER_HOLDING_LOVED_ITEM: f64 = 14.0;
/// Vanilla parity: `PiglinAi.INTERACTION_RANGE`.
const INTERACTION_RANGE: i32 = 8;
/// Vanilla parity: `PiglinAi.MIN_DESIRED_DIST_FROM_TARGET_WHEN_HOLDING_CROSSBOW`.
const MIN_DESIRED_DIST_FROM_TARGET_WHEN_HOLDING_CROSSBOW: f64 = 5.0;
/// Vanilla parity: `PiglinAi.SPEED_WHEN_STRAFING_BACK_FROM_TARGET`.
const SPEED_WHEN_STRAFING_BACK_FROM_TARGET: f32 = 0.75;
/// Vanilla parity: `PiglinAi.DESIRED_DISTANCE_FROM_ZOMBIFIED`.
const DESIRED_DISTANCE_FROM_ZOMBIFIED: f64 = 6.0;
/// Vanilla parity: `PiglinAi.AVOID_ZOMBIFIED_DURATION`, five to seven seconds.
const AVOID_ZOMBIFIED_DURATION: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5 * 20,
    max_inclusive: 7 * 20,
};
/// Vanilla parity: `PiglinAi.BABY_AVOID_NEMESIS_DURATION`, five to seven seconds.
const BABY_AVOID_NEMESIS_DURATION: UniformIntProvider = UniformIntProvider {
    min_inclusive: 5 * 20,
    max_inclusive: 7 * 20,
};
/// Vanilla parity: `PiglinAi.PROBABILITY_OF_CELEBRATION_DANCE`.
const PROBABILITY_OF_CELEBRATION_DANCE: f32 = 0.1;
/// Vanilla parity: `PiglinAi.SPEED_MULTIPLIER_WHEN_AVOIDING`.
const SPEED_MULTIPLIER_WHEN_AVOIDING: f64 = 1.0;
/// Vanilla parity: `PiglinAi.SPEED_MULTIPLIER_WHEN_RETREATING`.
const SPEED_MULTIPLIER_WHEN_RETREATING: f64 = 1.0;
/// Vanilla parity: `PiglinAi.SPEED_MULTIPLIER_WHEN_MOUNTING`.
const SPEED_MULTIPLIER_WHEN_MOUNTING: f64 = 0.8;
/// Vanilla parity: `PiglinAi.SPEED_MULTIPLIER_WHEN_GOING_TO_WANTED_ITEM`.
const SPEED_MULTIPLIER_WHEN_GOING_TO_WANTED_ITEM: f64 = 1.0;
/// Vanilla parity: `PiglinAi.SPEED_MULTIPLIER_WHEN_GOING_TO_CELEBRATE_LOCATION`.
const SPEED_MULTIPLIER_WHEN_GOING_TO_CELEBRATE_LOCATION: f64 = 1.0;
/// Vanilla parity: `PiglinAi.SPEED_MULTIPLIER_WHEN_DANCING`.
const SPEED_MULTIPLIER_WHEN_DANCING: f64 = 0.6;
/// Vanilla parity: `PiglinAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 0.6;
/// Vanilla parity: the `1.0F` chase speed of the fight activity.
const SPEED_MULTIPLIER_WHEN_CHASING: f64 = 1.0;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `DoNothing(30, 60)` of the idle look and movement gates.
const IDLE_DO_NOTHING_MIN: i32 = 30;
const IDLE_DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `DoNothing(10, 20)` of the celebrate gate.
const CELEBRATE_DO_NOTHING_MIN: i32 = 10;
const CELEBRATE_DO_NOTHING_MAX: i32 = 20;
/// Vanilla parity: the `RandomStroll.stroll(0.6F, 2, 1)` of the celebrate gate.
const CELEBRATE_STROLL_HORIZONTAL: i32 = 2;
const CELEBRATE_STROLL_VERTICAL: i32 = 1;
/// Vanilla parity: the `3` close-enough distance of the idle look-walk.
const IDLE_LOOK_WALK_CLOSE_ENOUGH: i32 = 3;
/// Vanilla parity: the `2` and `4` close-enough distances of the two
/// `GoToTargetLocation` calls in the celebrate activity.
const CELEBRATE_CLOSE_ENOUGH: i32 = 2;
const DANCE_CLOSE_ENOUGH: i32 = 4;
/// Vanilla parity: the `2` stop distance of the idle `InteractWith`.
const INTERACT_STOP_DISTANCE: i32 = 2;
/// Vanilla parity: the `4` range of `SetLookAndInteract.create(PLAYER, 4)`.
const LOOK_AND_INTERACT_RANGE: i32 = 4;

/// Runs an inner behavior only while a condition holds.
///
/// Vanilla parity: `BehaviorBuilder.triggerIf(Predicate, BehaviorControl)`,
/// which the piglin uses four times. Foton has no declarative builder -- see
/// the module note on [`crate::entity::ai::brain::behavior`] -- so the guard is
/// a wrapper rather than a combinator.
struct ConditionalTrigger {
    condition: fn(&BrainContext<'_>) -> bool,
    inner: Box<dyn BehaviorControl>,
}

impl ConditionalTrigger {
    fn boxed(
        condition: fn(&BrainContext<'_>) -> bool,
        inner: Box<dyn BehaviorControl>,
    ) -> Box<dyn BehaviorControl> {
        Box::new(Self { condition, inner })
    }
}

impl BehaviorControl for ConditionalTrigger {
    fn status(&self) -> BehaviorStatus {
        self.inner.status()
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        self.inner.required_memories()
    }

    fn try_start(&mut self, ctx: &BrainContext<'_>) -> bool {
        (self.condition)(ctx) && self.inner.try_start(ctx)
    }

    fn tick_or_stop(&mut self, ctx: &BrainContext<'_>) {
        self.inner.tick_or_stop(ctx);
    }

    fn do_stop(&mut self, ctx: &BrainContext<'_>) {
        self.inner.do_stop(ctx);
    }

    fn debug_name(&self) -> &'static str {
        self.inner.debug_name()
    }
}

/// The sensors a piglin runs.
///
/// Vanilla parity: the sensor list of `Piglin.BRAIN_PROVIDER`.
pub const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::NearestItems,
    SensorType::HurtBy,
    SensorType::PiglinSpecific,
];

/// Builds a piglin's brain.
///
/// Vanilla parity: `Piglin.BRAIN_PROVIDER` feeding `PiglinAi.getActivities`.
///
/// **Missing behavior**: vanilla's core activity also holds `InteractWithDoor`,
/// and its fight activity the three `Spear*` behaviors. Foton has no
/// door-opening behavior yet -- the navigation flag `AbstractPiglin`'s
/// constructor sets is ported, so a piglin still walks through an open door --
/// and no `KINETIC_WEAPON` mob path for the golden spear.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new(
        SENSORS,
        vec![
            core_activity(),
            idle_activity(),
            admire_item_activity(),
            fight_activity(),
            celebrate_activity(),
            retreat_activity(),
            ride_hoglin_activity(),
        ],
    )
}

/// Seeds the memories a freshly spawned piglin starts with.
///
/// Vanilla parity: `PiglinAi.initMemories`, which staggers the first hunt so a
/// newly generated bastion does not empty its stable at once.
pub fn init_memories(brain: &Brain) {
    brain.set_memory_with_expiry(
        memory_module_types::HUNTED_RECENTLY,
        true,
        sample_time_between_hunts(),
    );
}

/// Vanilla parity: `PiglinAi.TIME_BETWEEN_HUNTS.sample(random)`.
#[must_use]
pub fn sample_time_between_hunts() -> i64 {
    i64::from(rand::random_range(
        TIME_BETWEEN_HUNTS.min_inclusive..=TIME_BETWEEN_HUNTS.max_inclusive,
    ))
}

/// Vanilla parity: `PiglinAi.initCoreActivity`.
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
            baby_avoid_nemesis(),
            avoid_zombified(),
            OneShot::boxed(StopHoldingItemIfNoLongerAdmiring),
            OneShot::boxed(StartAdmiringItemIfSeen::new(ADMIRE_DURATION)),
            OneShot::boxed(StartCelebratingIfTargetDead::new(
                CELEBRATION_TIME,
                wants_to_dance,
            )),
            OneShot::boxed(StopBeingAngryIfTargetDead),
        ],
    )
}

/// Vanilla parity: `PiglinAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            OneShot::boxed(SetEntityLookTarget::matching(
                is_player_holding_loved_item,
                MAX_LOOK_DIST_FOR_PLAYER_HOLDING_LOVED_ITEM,
            )),
            OneShot::boxed(StartAttacking::conditional(
                |ctx| !ctx.mob().is_baby(),
                find_nearest_valid_attack_target,
            )),
            OneShot::boxed(StartHuntingHoglin),
            avoid_repellent(),
            baby_sometimes_ride_baby_hoglin(),
            idle_look_behaviors(),
            idle_movement_behaviors(),
            OneShot::boxed(SetLookAndInteract::new(
                &vanilla_entities::PLAYER,
                LOOK_AND_INTERACT_RANGE,
            )),
        ],
    )
}

/// Vanilla parity: `PiglinAi.initFightActivity`.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        10,
        vec![
            OneShot::boxed(
                StopAttackingIfTargetInvalid::new()
                    .when(|ctx, target| !is_nearest_valid_attack_target(ctx, target)),
            ),
            ConditionalTrigger::boxed(
                has_crossbow,
                OneShot::boxed(BackUpIfTooClose::new(
                    MIN_DESIRED_DIST_FROM_TARGET_WHEN_HOLDING_CROSSBOW,
                    SPEED_WHEN_STRAFING_BACK_FROM_TARGET,
                )),
            ),
            OneShot::boxed(SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(
                SPEED_MULTIPLIER_WHEN_CHASING,
            )),
            OneShot::boxed(MeleeAttack::new(MELEE_ATTACK_COOLDOWN)),
            CrossbowAttack::boxed(super::entity::crossbow_attack_hooks()),
            OneShot::boxed(RememberIfHoglinWasKilled),
            OneShot::boxed(EraseMemoryIf::new(
                is_near_zombified,
                memory_module_types::ATTACK_TARGET.id(),
            )),
        ],
    )
    .gated_by(memory_module_types::ATTACK_TARGET.id())
}

/// Vanilla parity: `PiglinAi.initCelebrateActivity`.
fn celebrate_activity() -> ActivityData {
    ActivityData::create(
        Activity::Celebrate,
        10,
        vec![
            avoid_repellent(),
            OneShot::boxed(SetEntityLookTarget::matching(
                is_player_holding_loved_item,
                MAX_LOOK_DIST_FOR_PLAYER_HOLDING_LOVED_ITEM,
            )),
            OneShot::boxed(StartAttacking::conditional(
                |ctx| !ctx.mob().is_baby(),
                find_nearest_valid_attack_target,
            )),
            ConditionalTrigger::boxed(
                |ctx| !is_dancing(ctx),
                OneShot::boxed(GoToTargetLocation::new(
                    memory_module_types::CELEBRATE_LOCATION,
                    CELEBRATE_CLOSE_ENOUGH,
                    SPEED_MULTIPLIER_WHEN_GOING_TO_CELEBRATE_LOCATION,
                )),
            ),
            ConditionalTrigger::boxed(
                is_dancing,
                OneShot::boxed(GoToTargetLocation::new(
                    memory_module_types::CELEBRATE_LOCATION,
                    DANCE_CLOSE_ENOUGH,
                    SPEED_MULTIPLIER_WHEN_DANCING,
                )),
            ),
            Box::new(RunOne::unconditional(vec![
                (
                    OneShot::boxed(SetEntityLookTarget::of_type(
                        &vanilla_entities::PIGLIN,
                        MAX_LOOK_DIST,
                    )),
                    1,
                ),
                (
                    OneShot::boxed(RandomStroll::stroll_within(
                        SPEED_MULTIPLIER_WHEN_IDLING,
                        CELEBRATE_STROLL_HORIZONTAL,
                        CELEBRATE_STROLL_VERTICAL,
                    )),
                    1,
                ),
                (
                    Box::new(DoNothing::new(
                        CELEBRATE_DO_NOTHING_MIN,
                        CELEBRATE_DO_NOTHING_MAX,
                    )),
                    1,
                ),
            ])),
        ],
    )
    .gated_by(memory_module_types::CELEBRATE_LOCATION.id())
}

/// Vanilla parity: `PiglinAi.initAdmireItemActivity`.
fn admire_item_activity() -> ActivityData {
    ActivityData::create(
        Activity::AdmireItem,
        10,
        vec![
            OneShot::boxed(GoToWantedItem::conditional(
                is_not_holding_loved_item_in_off_hand,
                SPEED_MULTIPLIER_WHEN_GOING_TO_WANTED_ITEM,
                true,
                MAX_DISTANCE_TO_WALK_TO_ITEM,
            )),
            OneShot::boxed(StopAdmiringIfItemTooFarAway::new(
                MAX_DISTANCE_TO_WALK_TO_ITEM,
            )),
            OneShot::boxed(StopAdmiringIfTiredOfTryingToReachItem::new(
                MAX_TIME_TO_WALK_TO_ITEM,
                DISABLE_ADMIRE_WALKING_TIME,
            )),
        ],
    )
    .gated_by(memory_module_types::ADMIRING_ITEM.id())
}

/// Vanilla parity: `PiglinAi.initRetreatActivity`.
fn retreat_activity() -> ActivityData {
    ActivityData::create(
        Activity::Avoid,
        10,
        vec![
            OneShot::boxed(SetWalkTargetAwayFrom::entity(
                memory_module_types::AVOID_TARGET,
                SPEED_MULTIPLIER_WHEN_RETREATING,
                DESIRED_DISTANCE_FROM_ENTITY_WHEN_AVOIDING,
                true,
            )),
            idle_look_behaviors(),
            idle_movement_behaviors(),
            OneShot::boxed(EraseMemoryIf::new(
                wants_to_stop_fleeing,
                memory_module_types::AVOID_TARGET.id(),
            )),
        ],
    )
    .gated_by(memory_module_types::AVOID_TARGET.id())
}

/// Vanilla parity: `PiglinAi.initRideHoglinActivity`.
fn ride_hoglin_activity() -> ActivityData {
    ActivityData::create(
        Activity::Ride,
        10,
        vec![
            OneShot::boxed(Mount::new(SPEED_MULTIPLIER_WHEN_MOUNTING)),
            OneShot::boxed(SetEntityLookTarget::matching(
                is_player_holding_loved_item,
                MAX_LOOK_DIST,
            )),
            // Vanilla wraps the look gate in `BehaviorBuilder.sequence(triggerIf(
            // Entity::isPassenger), ...)`, and adds an always-true entry so a
            // riding piglin sometimes looks at nothing in particular.
            ConditionalTrigger::boxed(
                |ctx| ctx.mob().is_passenger(),
                OneShot::boxed(TriggerGate::trigger_one_shuffled(ride_look_triggers())),
            ),
            OneShot::boxed(DismountOrSkipMounting::new(
                MAX_WALK_DISTANCE_TO_START_RIDING,
                wants_to_stop_riding,
            )),
        ],
    )
    .gated_by(memory_module_types::RIDE_TARGET.id())
}

/// Vanilla parity: the `createLookBehaviors()` list plus the always-true entry
/// of `initRideHoglinActivity`.
fn ride_look_triggers() -> Vec<(Box<dyn Trigger>, i32)> {
    vec![
        (
            Box::new(SetEntityLookTarget::of_type(
                &vanilla_entities::PLAYER,
                MAX_LOOK_DIST,
            )),
            1,
        ),
        (
            Box::new(SetEntityLookTarget::of_type(
                &vanilla_entities::PIGLIN,
                MAX_LOOK_DIST,
            )),
            1,
        ),
        (Box::new(SetEntityLookTarget::any_within(MAX_LOOK_DIST)), 1),
        (Box::new(LookAtNothing), 1),
    ]
}

/// The do-nothing arm of the ride look gate.
///
/// Vanilla parity: the `BehaviorBuilder.triggerIf(e -> true)` entry, which is
/// there so a riding piglin does not stare at something every single tick.
struct LookAtNothing;

impl Trigger for LookAtNothing {
    fn trigger(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn debug_name(&self) -> &'static str {
        "LookAtNothing"
    }
}

/// Vanilla parity: the private `PiglinAi.wantsToStopRiding`. A baby piglin gets
/// off when its mount grows up, dies, is hurt, hurts it, or is itself a piglin
/// that has been thrown off something.
fn wants_to_stop_riding(ctx: &BrainContext<'_>, vehicle: &SharedEntity) -> bool {
    let Some(vehicle_mob) = vehicle.as_mob() else {
        return false;
    };
    let hurt_recently = |brain: Option<&Brain>| {
        brain.is_some_and(|brain| brain.has_memory_value(memory_module_types::HURT_BY.id()))
    };

    !LivingEntity::is_baby(vehicle_mob)
        || !vehicle.is_alive()
        || hurt_recently(Some(ctx.brain()))
        || hurt_recently(vehicle_mob.brain())
        || (utils::is_of_type(vehicle.as_ref(), &vanilla_entities::PIGLIN)
            && vehicle.vehicle().is_none())
}

/// Vanilla parity: the private `PiglinAi.isBabyRidingBaby`.
fn is_baby_riding_baby(piglin: &PiglinEntity) -> bool {
    if !LivingEntity::is_baby(piglin) {
        return false;
    }
    let Some(vehicle) = piglin.vehicle() else {
        return false;
    };
    let is_piglin_or_hoglin = utils::is_of_type(vehicle.as_ref(), &vanilla_entities::PIGLIN)
        || utils::is_of_type(vehicle.as_ref(), &vanilla_entities::HOGLIN);
    is_piglin_or_hoglin
        && vehicle
            .as_living_entity()
            .is_some_and(LivingEntity::is_baby)
}

/// Vanilla parity: `PiglinAi.createLookBehaviors`.
fn look_behaviors() -> Vec<(Box<dyn BehaviorControl>, i32)> {
    vec![
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
            OneShot::boxed(SetEntityLookTarget::any_within(MAX_LOOK_DIST)),
            1,
        ),
    ]
}

/// Vanilla parity: `PiglinAi.createIdleLookBehaviors`.
fn idle_look_behaviors() -> Box<dyn BehaviorControl> {
    let mut behaviors = look_behaviors();
    behaviors.push((
        Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
        1,
    ));
    Box::new(RunOne::unconditional(behaviors))
}

/// Vanilla parity: `PiglinAi.createIdleMovementBehaviors`.
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
            ConditionalTrigger::boxed(
                |ctx| !sees_player_holding_loved_item(ctx),
                OneShot::boxed(SetWalkTargetFromLookTarget::new(
                    SPEED_MULTIPLIER_WHEN_IDLING,
                    IDLE_LOOK_WALK_CLOSE_ENOUGH,
                )),
            ),
            2,
        ),
        (
            Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
            1,
        ),
    ]))
}

/// Vanilla parity: `PiglinAi.avoidRepellent`.
fn avoid_repellent() -> Box<dyn BehaviorControl> {
    OneShot::boxed(SetWalkTargetAwayFrom::pos(
        memory_module_types::NEAREST_REPELLENT,
        SPEED_MULTIPLIER_WHEN_AVOIDING,
        REPELLENT_DETECTION_RANGE_HORIZONTAL,
        false,
    ))
}

/// Vanilla parity: `PiglinAi.REPELLENT_DETECTION_RANGE_HORIZONTAL`.
pub const REPELLENT_DETECTION_RANGE_HORIZONTAL: i32 = 8;

/// Vanilla parity: `PiglinAi.babyAvoidNemesis`.
fn baby_avoid_nemesis() -> Box<dyn BehaviorControl> {
    OneShot::boxed(CopyMemoryWithExpiry::new(
        |ctx| ctx.mob().is_baby(),
        memory_module_types::NEAREST_VISIBLE_NEMESIS,
        memory_module_types::AVOID_TARGET,
        BABY_AVOID_NEMESIS_DURATION,
    ))
}

/// Vanilla parity: `PiglinAi.avoidZombified`.
fn avoid_zombified() -> Box<dyn BehaviorControl> {
    OneShot::boxed(CopyMemoryWithExpiry::new(
        is_near_zombified,
        memory_module_types::NEAREST_VISIBLE_ZOMBIFIED,
        memory_module_types::AVOID_TARGET,
        AVOID_ZOMBIFIED_DURATION,
    ))
}

/// Vanilla parity: `PiglinAi.babySometimesRideBabyHoglin`.
///
/// **Missing behavior**: the `Mount` and `DismountOrSkipMounting` pair of
/// vanilla's `Activity.RIDE` is ported, but the activity itself is not
/// registered, because a baby piglin riding a baby hoglin needs the passenger
/// stack a hoglin does not yet expose. Copying the memory is harmless -- with
/// no RIDE activity nothing reads it -- and is kept so the behavior lands
/// whole when the ride does.
fn baby_sometimes_ride_baby_hoglin() -> Box<dyn BehaviorControl> {
    let ticks_until_next_start = Cell::new(0_i32);
    OneShot::boxed(CopyMemoryWithExpiry::new(
        move |ctx| {
            if !ctx.mob().is_baby() {
                return false;
            }
            // Vanilla parity: `SetEntityLookTargetSometimes.Ticker.tickDownAndCheck`.
            if ticks_until_next_start.get() == 0 {
                ticks_until_next_start.set(
                    rand::random_range(
                        RIDE_START_INTERVAL.min_inclusive..=RIDE_START_INTERVAL.max_inclusive,
                    ) - 1,
                );
                return false;
            }
            ticks_until_next_start.set(ticks_until_next_start.get() - 1);
            ticks_until_next_start.get() == 0
        },
        memory_module_types::NEAREST_VISIBLE_BABY_HOGLIN,
        memory_module_types::RIDE_TARGET,
        RIDE_DURATION,
    ))
}

/// Picks the activity a piglin should be in, and keeps its flags in step.
///
/// Vanilla parity: `PiglinAi.updateActivity`. The sound on an activity change
/// is the mob's, because only it can `makeSound`; [`sound_for_current_activity`]
/// is what it asks.
pub fn update_activity(piglin: &PiglinEntity) -> Option<SoundEventRef> {
    let brain = piglin.brain_ref();
    let old_activity = brain.active_non_core_activity();
    brain.set_active_activity_to_first_valid(&[
        Activity::AdmireItem,
        Activity::Fight,
        Activity::Avoid,
        Activity::Celebrate,
        Activity::Ride,
        Activity::Idle,
    ]);
    let new_activity = brain.active_non_core_activity();

    Mob::set_aggressive(
        piglin,
        brain.has_memory_value(memory_module_types::ATTACK_TARGET.id()),
    );
    if !brain.has_memory_value(memory_module_types::RIDE_TARGET.id()) && is_baby_riding_baby(piglin)
    {
        piglin.stop_riding();
    }
    if !brain.has_memory_value(memory_module_types::CELEBRATE_LOCATION.id()) {
        brain.erase_memory(memory_module_types::DANCING.id());
    }
    piglin.set_dancing(brain.has_memory_value(memory_module_types::DANCING.id()));

    (old_activity != new_activity)
        .then(|| sound_for_current_activity(piglin))
        .flatten()
}

/// The sound a piglin makes for whatever it is doing.
///
/// Vanilla parity: `PiglinAi.getSoundForCurrentActivity` and the
/// `getSoundForActivity` it delegates to.
#[must_use]
pub fn sound_for_current_activity(piglin: &PiglinEntity) -> Option<SoundEventRef> {
    let brain = piglin.brain_ref();
    let activity = brain.active_non_core_activity()?;

    if activity == Activity::Fight {
        return Some(&sound_events::ENTITY_PIGLIN_ANGRY);
    }
    if piglin.piglin_is_converting() {
        return Some(&sound_events::ENTITY_PIGLIN_RETREAT);
    }
    if activity == Activity::Avoid && is_near_avoid_target(piglin) {
        return Some(&sound_events::ENTITY_PIGLIN_RETREAT);
    }
    if activity == Activity::AdmireItem {
        return Some(&sound_events::ENTITY_PIGLIN_ADMIRING_ITEM);
    }
    if activity == Activity::Celebrate {
        return Some(&sound_events::ENTITY_PIGLIN_CELEBRATE);
    }
    if brain.has_memory_value(memory_module_types::NEAREST_PLAYER_HOLDING_WANTED_ITEM.id()) {
        return Some(&sound_events::ENTITY_PIGLIN_JEALOUS);
    }
    if brain.has_memory_value(memory_module_types::NEAREST_REPELLENT.id()) {
        return Some(&sound_events::ENTITY_PIGLIN_RETREAT);
    }
    Some(&sound_events::ENTITY_PIGLIN_AMBIENT)
}

/// Vanilla parity: the private `PiglinAi.isNearAvoidTarget`.
fn is_near_avoid_target(piglin: &PiglinEntity) -> bool {
    piglin
        .brain_ref()
        .get_memory(memory_module_types::AVOID_TARGET)
        .and_then(|memory| memory.get())
        .is_some_and(|target| {
            let range = f64::from(DESIRED_DISTANCE_FROM_ENTITY_WHEN_AVOIDING);
            target.position().distance_squared(piglin.position()) < range * range
        })
}

/// Vanilla parity: `PiglinAi.isIdle`.
#[must_use]
pub fn is_idle(brain: &Brain) -> bool {
    brain.is_active(Activity::Idle)
}

/// Vanilla parity: the private `PiglinAi.hasCrossbow`.
fn has_crossbow(ctx: &BrainContext<'_>) -> bool {
    ctx.mob()
        .is_holding(&mut |item| item.is(&vanilla_items::CROSSBOW))
}

/// Vanilla parity: the private `PiglinAi.isDancing`, read off the brain.
fn is_dancing(ctx: &BrainContext<'_>) -> bool {
    ctx.brain()
        .has_memory_value(memory_module_types::DANCING.id())
}

/// Vanilla parity: the private `PiglinAi.seesPlayerHoldingLovedItem`.
fn sees_player_holding_loved_item(ctx: &BrainContext<'_>) -> bool {
    ctx.brain()
        .has_memory_value(memory_module_types::NEAREST_PLAYER_HOLDING_WANTED_ITEM.id())
}

/// Vanilla parity: `PiglinAi.isLovedItem`, the `piglin_loved` tag.
#[must_use]
pub fn is_loved_item(item_stack: &ItemStack) -> bool {
    REGISTRY
        .items
        .is_in_tag(item_stack.item(), &ItemTag::PIGLIN_LOVED)
}

/// Vanilla parity: the private `PiglinAi.isFood`, the `piglin_food` tag.
#[must_use]
pub fn is_food(item_stack: &ItemStack) -> bool {
    REGISTRY
        .items
        .is_in_tag(item_stack.item(), &ItemTag::PIGLIN_FOOD)
}

/// Vanilla parity: the private `PiglinAi.isNotHoldingLovedItemInOffHand`.
fn is_not_holding_loved_item_in_off_hand(ctx: &BrainContext<'_>) -> bool {
    let offhand = ctx.mob().get_item_in_hand(InteractionHand::OffHand);
    offhand.is_empty() || !is_loved_item(&offhand)
}

/// Vanilla parity: the private `PiglinAi.isNearZombified`.
fn is_near_zombified(ctx: &BrainContext<'_>) -> bool {
    ctx.brain()
        .get_memory(memory_module_types::NEAREST_VISIBLE_ZOMBIFIED)
        .and_then(|memory| memory.get())
        .is_some_and(|zombified| {
            zombified.position().distance_squared(ctx.mob().position())
                < DESIRED_DISTANCE_FROM_ZOMBIFIED * DESIRED_DISTANCE_FROM_ZOMBIFIED
        })
}

/// Vanilla parity: the private `PiglinAi.wantsToDance`, a one-in-ten roll on a
/// killed hoglin.
///
/// The roll is seeded on the game time, exactly as vanilla's
/// `RandomSource.createThreadLocalInstance(level.getGameTime())` is, so every
/// piglin celebrating the same kill on the same tick makes the same decision --
/// a pack dances in unison or not at all, which is the observable behavior.
fn wants_to_dance(ctx: &BrainContext<'_>, killed_target: &SharedEntity) -> bool {
    use foton_utils::random::{Random as _, legacy_random::LegacyRandom};

    if !utils::is_of_type(killed_target.as_ref(), &vanilla_entities::HOGLIN) {
        return false;
    }
    #[expect(
        clippy::cast_sign_loss,
        reason = "vanilla seeds the roll with the raw game time, negative or not"
    )]
    let mut random = LegacyRandom::from_seed(ctx.game_time() as u64);
    random.next_f32() < PROBABILITY_OF_CELEBRATION_DANCE
}

/// Vanilla parity: the private `PiglinAi.findNearestValidAttackTarget`.
fn find_nearest_valid_attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    if is_near_zombified(ctx) {
        return None;
    }
    let brain = ctx.brain();

    if let Some(angry_at) =
        utils::living_entity_from_uuid_memory(ctx.world(), brain, memory_module_types::ANGRY_AT)
        && angry_at
            .as_living_entity()
            .is_some_and(|living| is_entity_attackable_ignoring_line_of_sight(ctx, living))
    {
        return Some(angry_at);
    }

    if brain.has_memory_value(memory_module_types::UNIVERSAL_ANGER.id())
        && let Some(player) = brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
            .and_then(|memory| memory.get())
    {
        return Some(player);
    }

    if let Some(nemesis) = brain
        .get_memory(memory_module_types::NEAREST_VISIBLE_NEMESIS)
        .and_then(|memory| memory.get())
    {
        return Some(nemesis);
    }

    let player_not_wearing_gold = brain
        .get_memory(memory_module_types::NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD)
        .and_then(|memory| memory.get())?;
    let attackable = player_not_wearing_gold
        .as_living_entity()
        .is_some_and(|living| is_entity_attackable(ctx.world(), ctx.mob(), living));
    attackable.then_some(player_not_wearing_gold)
}

/// Vanilla parity: the private `PiglinAi.isNearestValidAttackTarget`.
fn is_nearest_valid_attack_target(ctx: &BrainContext<'_>, target: &SharedEntity) -> bool {
    find_nearest_valid_attack_target(ctx).is_some_and(|nearest| nearest.id() == target.id())
}

/// Vanilla parity: `Sensor.isEntityAttackableIgnoringLineOfSight`.
///
/// Foton's targeting conditions carry the line-of-sight test, so the ignoring
/// form is the combat conditions with sight testing turned off.
fn is_entity_attackable_ignoring_line_of_sight(
    ctx: &BrainContext<'_>,
    target: &dyn LivingEntity,
) -> bool {
    TargetingConditions::for_combat()
        .range(follow_range(ctx.mob()))
        .ignore_line_of_sight()
        .test(ctx.world(), Some(ctx.mob()), target)
}

/// Vanilla parity: the private `PiglinAi.wantsToStopFleeing`.
fn wants_to_stop_fleeing(ctx: &BrainContext<'_>) -> bool {
    let brain = ctx.brain();
    let Some(avoided) = brain
        .get_memory(memory_module_types::AVOID_TARGET)
        .and_then(|memory| memory.get())
    else {
        return true;
    };
    if utils::is_of_type(avoided.as_ref(), &vanilla_entities::HOGLIN) {
        return !hoglins_outnumber_piglins(brain);
    }
    let Some(living) = avoided.as_living_entity() else {
        return false;
    };
    if !is_zombified(living) {
        return false;
    }
    brain
        .get_memory(memory_module_types::NEAREST_VISIBLE_ZOMBIFIED)
        .is_none_or(|nearest| nearest.id() != avoided.id())
}

/// Vanilla parity: the private `PiglinAi.hoglinsOutnumberPiglins`. The piglin
/// counts itself.
fn hoglins_outnumber_piglins(brain: &Brain) -> bool {
    let piglin_count = brain
        .get_memory(memory_module_types::VISIBLE_ADULT_PIGLIN_COUNT)
        .unwrap_or(0)
        + 1;
    let hoglin_count = brain
        .get_memory(memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT)
        .unwrap_or(0);
    hoglin_count > piglin_count
}

/// Vanilla parity: `PiglinAi.wantsToPickup`.
#[must_use]
pub fn wants_to_pickup(piglin: &PiglinEntity, item_stack: &ItemStack) -> bool {
    let brain = piglin.brain_ref();
    if LivingEntity::is_baby(piglin)
        && REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::IGNORED_BY_PIGLIN_BABIES)
    {
        return false;
    }
    if REGISTRY
        .items
        .is_in_tag(item_stack.item(), &ItemTag::PIGLIN_REPELLENTS)
    {
        return false;
    }
    if brain.has_memory_value(memory_module_types::ADMIRING_DISABLED.id())
        && brain.has_memory_value(memory_module_types::ATTACK_TARGET.id())
    {
        return false;
    }

    let offhand = piglin.get_item_in_hand(InteractionHand::OffHand);
    let not_holding_loved = offhand.is_empty() || !is_loved_item(&offhand);
    if is_barter_currency(item_stack) {
        return not_holding_loved;
    }

    let has_space = piglin.can_add_to_inventory(item_stack);
    if item_stack.is(&vanilla_items::GOLD_NUGGET) {
        return has_space;
    }
    if is_food(item_stack) {
        return !brain.has_memory_value(memory_module_types::ATE_RECENTLY.id()) && has_space;
    }
    if is_loved_item(item_stack) {
        return not_holding_loved && has_space;
    }
    let slot = piglin.equipment_slot_for_item(item_stack);
    let current = piglin.get_item_by_slot(slot);
    Mob::can_replace_current_item(piglin, item_stack, &current, slot)
}

/// Takes one dropped item off the ground.
///
/// Vanilla parity: `PiglinAi.pickUpItem`. A gold nugget is taken whole; every
/// other stack gives up exactly one item, which is why a piglin never empties a
/// pile of ingots at once.
pub fn pick_up_item(piglin: &PiglinEntity, item_entity: &SharedEntity) {
    stop_walking(piglin);

    let Some(item_entity_ref) = item_entity.downcast_ref::<ItemEntity>() else {
        return;
    };
    let stack = item_entity_ref.get_item();
    let taken = if stack.is(&vanilla_items::GOLD_NUGGET) {
        item_entity_ref.set_item(ItemStack::empty());
        item_entity.set_removed(RemovalReason::Discarded);
        stack
    } else {
        let mut remaining = stack;
        let taken = remaining.split(1);
        if remaining.is_empty() {
            item_entity.set_removed(RemovalReason::Discarded);
        } else {
            item_entity_ref.set_item(remaining);
        }
        taken
    };

    let brain = piglin.brain_ref();
    if is_loved_item(&taken) {
        brain.erase_memory(memory_module_types::TIME_TRYING_TO_REACH_ADMIRE_ITEM.id());
        hold_in_offhand(piglin, taken);
        admire_gold_item(brain);
        return;
    }
    if is_food(&taken) && !brain.has_memory_value(memory_module_types::ATE_RECENTLY.id()) {
        brain.set_memory_with_expiry(memory_module_types::ATE_RECENTLY, true, EAT_COOLDOWN);
        return;
    }
    if piglin.equip_item_if_possible(&taken).is_empty() {
        put_in_inventory(piglin, taken);
    }
}

/// Vanilla parity: the private `PiglinAi.holdInOffhand`, which spills whatever
/// the piglin was already holding rather than stacking two prizes.
fn hold_in_offhand(piglin: &PiglinEntity, item_stack: ItemStack) {
    let existing = piglin.get_item_in_hand(InteractionHand::OffHand);
    if !existing.is_empty() {
        let _ = piglin.spawn_at_location(existing, 0.0);
    }
    piglin.hold_in_off_hand(item_stack);
}

/// Vanilla parity: the private `PiglinAi.admireGoldItem`.
fn admire_gold_item(brain: &Brain) {
    brain.set_memory_with_expiry(memory_module_types::ADMIRING_ITEM, true, ADMIRE_DURATION);
}

/// Vanilla parity: the private `PiglinAi.putInInventory`, which throws away
/// whatever would not fit.
fn put_in_inventory(piglin: &PiglinEntity, item_stack: ItemStack) {
    let leftover = piglin.add_to_inventory(item_stack);
    throw_items_toward_random_pos(piglin, vec![leftover]);
}

/// Vanilla parity: the private `PiglinAi.stopWalking`.
fn stop_walking(piglin: &PiglinEntity) {
    piglin
        .brain_ref()
        .erase_memory(memory_module_types::WALK_TARGET.id());
    piglin.mob_base().navigation().lock().stop();
}

/// Empties the off hand, bartering the gold rather than pocketing it.
///
/// Vanilla parity: `PiglinAi.stopHoldingOffHandItem`. An adult with the barter
/// currency rolls `gameplay/piglin_bartering` and throws the result; a baby
/// simply keeps what it found, which is why a baby never trades.
pub fn stop_holding_off_hand_item(piglin: &PiglinEntity, bartering_enabled: bool) {
    let item_stack = piglin.get_item_in_hand(InteractionHand::OffHand);
    piglin.set_item_in_hand(InteractionHand::OffHand, ItemStack::empty());

    if !piglin.is_adult() {
        if !piglin.equip_item_if_possible(&item_stack).is_empty() {
            return;
        }
        let main_hand = piglin.get_item_in_hand(InteractionHand::MainHand);
        if is_loved_item(&main_hand) {
            put_in_inventory(piglin, main_hand);
        } else {
            throw_items(piglin, vec![main_hand]);
        }
        piglin.hold_in_main_hand(item_stack);
        return;
    }

    let barter_currency = is_barter_currency(&item_stack);
    if bartering_enabled && barter_currency {
        throw_items(piglin, barter_response_items(piglin));
        return;
    }
    if barter_currency {
        return;
    }
    if piglin.equip_item_if_possible(&item_stack).is_empty() {
        put_in_inventory(piglin, item_stack);
    }
}

/// Rolls the barter table.
///
/// Vanilla parity: the private `PiglinAi.getBarterResponseItems`, which asks
/// `BuiltInLootTables.PIGLIN_BARTERING` with the `PIGLIN_BARTER` parameter set
/// -- `THIS_ENTITY` and nothing else.
#[must_use]
pub fn barter_response_items(piglin: &PiglinEntity) -> Vec<ItemStack> {
    barter_loot_items(piglin, &vanilla_loot_tables::GAMEPLAY_PIGLIN_BARTERING)
}

/// Vanilla parity: the private `PiglinAi.throwItems`, which aims at the nearest
/// visible player when there is one.
fn throw_items(piglin: &PiglinEntity, item_stacks: Vec<ItemStack>) {
    let toward = piglin
        .brain_ref()
        .get_memory(memory_module_types::NEAREST_VISIBLE_PLAYER)
        .and_then(|memory| memory.get())
        .map(|player| player.position());
    match toward {
        Some(position) => throw_items_toward_pos(piglin, item_stacks, position),
        None => throw_items_toward_random_pos(piglin, item_stacks),
    }
}

/// Vanilla parity: the private `PiglinAi.throwItemsTowardRandomPos`.
fn throw_items_toward_random_pos(piglin: &PiglinEntity, item_stacks: Vec<ItemStack>) {
    let target = land_random_pos(piglin, 4, 2).unwrap_or(piglin.position());
    throw_items_toward_pos(piglin, item_stacks, target);
}

/// Vanilla parity: the private `PiglinAi.throwItemsTowardPos`.
fn throw_items_toward_pos(piglin: &PiglinEntity, item_stacks: Vec<ItemStack>, target: DVec3) {
    if item_stacks.iter().all(ItemStack::is_empty) {
        return;
    }
    piglin.swing(InteractionHand::OffHand, true);
    for item_stack in item_stacks {
        utils::throw_item(piglin, item_stack, target + DVec3::new(0.0, 1.0, 0.0));
    }
}

/// Hands a piglin a gold ingot from a player's hand.
///
/// Vanilla parity: `PiglinAi.mobInteract`.
pub fn mob_interact(
    piglin: &PiglinEntity,
    player: &Player,
    hand: InteractionHand,
) -> InteractionResult {
    let held = {
        let inventory = player.inventory.lock();
        let stack = inventory.get_item_in_hand(hand);
        stack.copy_with_count(stack.count())
    };
    if !can_admire(piglin, &held) {
        return InteractionResult::Pass;
    }

    let taken = {
        let mut inventory = player.inventory.lock();
        let mut stack = inventory.get_item_in_hand(hand).clone();
        let taken = stack.split(1);
        if !player.has_infinite_materials() {
            inventory.set_item_in_hand(hand, stack);
        }
        taken
    };
    hold_in_offhand(piglin, taken);
    admire_gold_item(piglin.brain_ref());
    stop_walking(piglin);
    InteractionResult::Success
}

/// Vanilla parity: `PiglinAi.canAdmire`.
#[must_use]
pub fn can_admire(piglin: &PiglinEntity, player_held: &ItemStack) -> bool {
    let brain = piglin.brain_ref();
    !brain.has_memory_value(memory_module_types::ADMIRING_DISABLED.id())
        && !brain.has_memory_value(memory_module_types::ADMIRING_ITEM.id())
        && piglin.is_adult()
        && is_barter_currency(player_held)
}

/// Reacts to being hit.
///
/// Vanilla parity: `PiglinAi.wasHurtBy`. A piglin hit by another piglin ignores
/// it entirely; everything else drops what it was admiring, and a baby, or an
/// adult a hoglin pack has outnumbered, runs rather than fights.
pub fn was_hurt_by(
    world: &Arc<World>,
    piglin: &PiglinEntity,
    attacker: &SharedEntity,
    attacker_living: &dyn LivingEntity,
) {
    if utils::is_of_type(attacker.as_ref(), &vanilla_entities::PIGLIN) {
        return;
    }
    if !piglin.get_item_in_hand(InteractionHand::OffHand).is_empty() {
        stop_holding_off_hand_item(piglin, false);
    }

    let brain = piglin.brain_ref();
    brain.erase_memory(memory_module_types::CELEBRATE_LOCATION.id());
    brain.erase_memory(memory_module_types::DANCING.id());
    brain.erase_memory(memory_module_types::ADMIRING_ITEM.id());
    if utils::is_of_type(attacker.as_ref(), &vanilla_entities::PLAYER) {
        brain.set_memory_with_expiry(
            memory_module_types::ADMIRING_DISABLED,
            true,
            HIT_BY_PLAYER_MEMORY_TIMEOUT,
        );
    }

    // Vanilla parity: an avoid target of a different kind is dropped, so a
    // piglin fleeing hoglins turns on a player who shoots it.
    if let Some(avoid_target) = brain
        .get_memory(memory_module_types::AVOID_TARGET)
        .and_then(|memory| memory.get())
        && !ptr::eq(avoid_target.entity_type(), attacker.entity_type())
    {
        brain.erase_memory(memory_module_types::AVOID_TARGET.id());
    }

    if LivingEntity::is_baby(piglin) {
        brain.set_memory_with_expiry(
            memory_module_types::AVOID_TARGET,
            utils::remember(attacker),
            BABY_FLEE_DURATION_AFTER_GETTING_HIT,
        );
        if is_attackable_ignoring_line_of_sight(world, piglin, attacker_living) {
            broadcast_anger_target(world, brain, piglin, attacker);
        }
        return;
    }

    if utils::is_of_type(attacker.as_ref(), &vanilla_entities::HOGLIN)
        && hoglins_outnumber_piglins(brain)
    {
        set_avoid_target_and_dont_hunt_for_a_while(brain, attacker);
        broadcast_retreat(brain, piglin, attacker);
        return;
    }

    maybe_retaliate(world, brain, piglin, attacker, attacker_living);
}

/// Vanilla parity: `PiglinAi.maybeRetaliate`, shared with the piglin brute.
pub fn maybe_retaliate(
    world: &Arc<World>,
    brain: &Brain,
    body: &dyn PathfinderMob,
    attacker: &SharedEntity,
    attacker_living: &dyn LivingEntity,
) {
    if brain.is_active(Activity::Avoid) {
        return;
    }
    if !is_attackable_ignoring_line_of_sight(world, body, attacker_living) {
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

    if utils::is_of_type(attacker.as_ref(), &vanilla_entities::PLAYER)
        && world.get_game_rule(&UNIVERSAL_ANGER)
    {
        set_anger_target_to_nearest_targetable_player_if_found(world, brain, body, attacker);
        broadcast_universal_anger(world, brain);
        return;
    }
    set_anger_target(world, brain, body, attacker);
    broadcast_anger_target(world, brain, body, attacker);
}

/// Vanilla parity: `Sensor.isEntityAttackableIgnoringLineOfSight`, off a mob
/// rather than a brain context.
fn is_attackable_ignoring_line_of_sight(
    world: &World,
    body: &dyn PathfinderMob,
    target: &dyn LivingEntity,
) -> bool {
    TargetingConditions::for_combat()
        .range(follow_range(body))
        .ignore_line_of_sight()
        .test(world, Some(body), target)
}

/// Vanilla parity: `PiglinAi.setAngerTarget`.
pub fn set_anger_target(
    world: &World,
    brain: &Brain,
    body: &dyn PathfinderMob,
    target: &SharedEntity,
) {
    let Some(living) = target.as_living_entity() else {
        return;
    };
    if !is_attackable_ignoring_line_of_sight(world, body, living) {
        return;
    }

    brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
    brain.set_memory_with_expiry(memory_module_types::ANGRY_AT, target.uuid(), ANGER_DURATION);
    if utils::is_of_type(target.as_ref(), &vanilla_entities::HOGLIN) && body.can_hunt() {
        dont_kill_any_more_hoglins_for_a_while(brain);
    }
    if utils::is_of_type(target.as_ref(), &vanilla_entities::PLAYER)
        && world.get_game_rule(&UNIVERSAL_ANGER)
    {
        brain.set_memory_with_expiry(memory_module_types::UNIVERSAL_ANGER, true, ANGER_DURATION);
    }
}

/// Vanilla parity: the private `PiglinAi.setAngerTargetToNearestTargetablePlayerIfFound`.
fn set_anger_target_to_nearest_targetable_player_if_found(
    world: &World,
    brain: &Brain,
    body: &dyn PathfinderMob,
    fallback: &SharedEntity,
) {
    let nearest_player = brain
        .get_memory(memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
        .and_then(|memory| memory.get());
    match nearest_player {
        Some(player) => set_anger_target(world, brain, body, &player),
        None => set_anger_target(world, brain, body, fallback),
    }
}

/// Vanilla parity: `PiglinAi.broadcastUniversalAnger`.
fn broadcast_universal_anger(world: &World, brain: &Brain) {
    for_each_nearby_adult_piglin(brain, |neighbor_brain, neighbor| {
        let Some(pathfinder) = neighbor.as_pathfinder_mob() else {
            return;
        };
        let Some(player) = neighbor_brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
            .and_then(|memory| memory.get())
        else {
            return;
        };
        set_anger_target(world, neighbor_brain, pathfinder, &player);
    });
}

/// Vanilla parity: `PiglinAi.broadcastAngerTarget`, which is how one shout
/// turns a whole bastion. A neighbor that cannot hunt is skipped for a hoglin
/// target, so the stable is not attacked by proxy.
pub fn broadcast_anger_target(
    world: &World,
    brain: &Brain,
    _body: &dyn PathfinderMob,
    target: &SharedEntity,
) {
    let target_is_hoglin = utils::is_of_type(target.as_ref(), &vanilla_entities::HOGLIN);
    let target_huntable = target
        .downcast_ref::<HoglinEntity>()
        .is_some_and(HoglinEntity::can_be_hunted);

    for_each_nearby_adult_piglin(brain, |neighbor_brain, neighbor| {
        let Some(pathfinder) = neighbor.as_pathfinder_mob() else {
            return;
        };
        if target_is_hoglin && (!pathfinder.can_hunt() || !target_huntable) {
            return;
        }
        set_anger_target_if_closer_than_current(world, neighbor_brain, pathfinder, target);
    });
}

/// Vanilla parity: the private `PiglinAi.setAngerTargetIfCloserThanCurrent`.
fn set_anger_target_if_closer_than_current(
    world: &World,
    brain: &Brain,
    body: &dyn PathfinderMob,
    new_target: &SharedEntity,
) {
    let current =
        utils::living_entity_from_uuid_memory(world, brain, memory_module_types::ANGRY_AT);
    let nearest = utils::nearest_target(
        body.as_entity_event_source(),
        current.clone(),
        new_target.clone(),
    );
    if current.is_none_or(|current| current.id() != nearest.id()) {
        set_anger_target(world, brain, body, &nearest);
    }
}

/// Vanilla parity: the private `PiglinAi.broadcastRetreat`.
fn broadcast_retreat(brain: &Brain, body: &dyn PathfinderMob, target: &SharedEntity) {
    let Some(neighbors) = brain.get_memory(memory_module_types::NEAREST_VISIBLE_ADULT_PIGLINS)
    else {
        return;
    };
    for remembered in neighbors {
        let Some(neighbor) = remembered.get() else {
            continue;
        };
        // Vanilla parity: only a `Piglin` retreats; a brute stands its ground.
        if !utils::is_of_type(neighbor.as_ref(), &vanilla_entities::PIGLIN) {
            continue;
        }
        let Some(neighbor_brain) = neighbor.as_mob().and_then(Mob::brain) else {
            continue;
        };
        retreat_from_nearest_target(neighbor_brain, body, target);
    }
}

/// Vanilla parity: the private `PiglinAi.retreatFromNearestTarget`.
fn retreat_from_nearest_target(
    brain: &Brain,
    body: &dyn PathfinderMob,
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
    set_avoid_target_and_dont_hunt_for_a_while(brain, &nearest);
}

/// Vanilla parity: the private `PiglinAi.setAvoidTargetAndDontHuntForAWhile`.
fn set_avoid_target_and_dont_hunt_for_a_while(brain: &Brain, target: &SharedEntity) {
    brain.erase_memory(memory_module_types::ANGRY_AT.id());
    brain.erase_memory(memory_module_types::ATTACK_TARGET.id());
    brain.erase_memory(memory_module_types::WALK_TARGET.id());
    brain.set_memory_with_expiry(
        memory_module_types::AVOID_TARGET,
        utils::remember(target),
        i64::from(rand::random_range(
            RETREAT_DURATION.min_inclusive..=RETREAT_DURATION.max_inclusive,
        )),
    );
    dont_kill_any_more_hoglins_for_a_while(brain);
}

/// Vanilla parity: `PiglinAi.dontKillAnyMoreHoglinsForAWhile`.
pub fn dont_kill_any_more_hoglins_for_a_while(brain: &Brain) {
    brain.set_memory_with_expiry(
        memory_module_types::HUNTED_RECENTLY,
        true,
        sample_time_between_hunts(),
    );
}

/// Runs `action` against every nearby adult piglin's own brain.
///
/// Vanilla parity: the `getAdultPiglins(body).forEach(...)` shared by the two
/// anger broadcasts, which reads `NEARBY_ADULT_PIGLINS` -- the unfiltered list,
/// so a shout carries through a wall.
fn for_each_nearby_adult_piglin(brain: &Brain, mut action: impl FnMut(&Brain, &SharedEntity)) {
    let Some(neighbors) = brain.get_memory(memory_module_types::NEARBY_ADULT_PIGLINS) else {
        return;
    };
    for remembered in neighbors {
        let Some(neighbor) = remembered.get() else {
            continue;
        };
        let Some(neighbor_brain) = neighbor.as_mob().and_then(Mob::brain) else {
            continue;
        };
        action(neighbor_brain, &neighbor);
    }
}

/// Angers every idle piglin that can see `player`.
///
/// Vanilla parity: `PiglinAi.angerNearbyPiglins`, called when a player opens a
/// container piglins guard or breaks a block in the `guarded_by_piglins` tag.
pub fn anger_nearby_piglins(world: &Arc<World>, player: &SharedEntity, only_if_they_see: bool) {
    let search = player.bounding_box().inflate_xyz(
        PLAYER_ANGER_RANGE,
        PLAYER_ANGER_RANGE,
        PLAYER_ANGER_RANGE,
    );
    let nearby = world.get_entities_in_aabb_matching(&search, |entity| {
        utils::is_of_type(entity, &vanilla_entities::PIGLIN)
    });

    let universal_anger = world.get_game_rule(&UNIVERSAL_ANGER);
    for piglin_entity in nearby {
        let Some(piglin) = piglin_entity.downcast_ref::<PiglinEntity>() else {
            continue;
        };
        let brain = piglin.brain_ref();
        if !is_idle(brain) {
            continue;
        }
        if only_if_they_see && !utils::can_see(brain, player.as_ref()) {
            continue;
        }
        if universal_anger {
            set_anger_target_to_nearest_targetable_player_if_found(world, brain, piglin, player);
        } else {
            set_anger_target(world, brain, piglin, player);
        }
    }
}

/// Drops whatever the piglin was admiring when it zombifies.
///
/// Vanilla parity: `PiglinAi.cancelAdmiring`.
pub fn cancel_admiring(piglin: &PiglinEntity) {
    let brain = piglin.brain_ref();
    if !brain.has_memory_value(memory_module_types::ADMIRING_ITEM.id()) {
        return;
    }
    let offhand = piglin.get_item_in_hand(InteractionHand::OffHand);
    if offhand.is_empty() {
        return;
    }
    let _ = piglin.spawn_at_location(offhand, 0.0);
    piglin.set_item_in_hand(InteractionHand::OffHand, ItemStack::empty());
}
