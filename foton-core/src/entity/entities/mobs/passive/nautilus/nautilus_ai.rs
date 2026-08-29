//! The nautilus's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.nautilus.NautilusAi`.
//! Three activities: a core that panics, swims and counts three cooldowns down,
//! an idle set that courts, follows food and picks a fight, and a fight set
//! that is one behavior -- the charge.

use foton_registry::vanilla_entities;
use foton_registry::vanilla_entity_type_tags::EntityTypeTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _, sound_events};

use crate::entity::ai::brain::behavior::utils::living_entity_from_uuid_memory;
use crate::entity::ai::brain::behavior::{
    AnimalMakeLove, AnimalPanic, Behavior, BehaviorControl, ChargeAttack, CountDownCooldownTicks,
    FollowTemptation, GateBehavior, LookAtTargetSink, MoveToTargetSink, OneShot, OrderPolicy,
    RandomStroll, RunningPolicy, SetWalkTargetFromLookTarget, StartAttacking,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::{SensorType, is_entity_attackable_ignoring_line_of_sight};
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::nautilus::sample_time_between_non_player_attacks;
use crate::entity::{AgeableMob, LivingEntity, SharedEntity, is_tamed};

/// Vanilla parity: `NautilusAi.SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER`.
const SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER: f64 = 1.0;
/// Vanilla parity: `NautilusAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 1.3;
/// Vanilla parity: `NautilusAi.SPEED_MULTIPLIER_WHEN_MAKING_LOVE`.
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f64 = 0.4;
/// Vanilla parity: `NautilusAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 1.6;
/// Vanilla parity: `NautilusAi.SPEED_WHEN_ATTACKING`.
const SPEED_WHEN_ATTACKING: f32 = 0.6;
/// Vanilla parity: `NautilusAi.ATTACK_KNOCKBACK_FORCE`.
pub(super) const ATTACK_KNOCKBACK_FORCE: f32 = 2.0;
/// Vanilla parity: `NautilusAi.TIME_BETWEEN_ATTACKS`.
pub(super) const TIME_BETWEEN_ATTACKS: i32 = 80;
/// Vanilla parity: `NautilusAi.MAX_CHARGE_DISTANCE`.
pub(super) const MAX_CHARGE_DISTANCE: f64 = 12.0;
/// Vanilla parity: `NautilusAi.MAX_TARGET_DETECTION_DISTANCE`.
pub(super) const MAX_TARGET_DETECTION_DISTANCE: f64 = 11.0;

/// Vanilla parity: the `AnimalMakeLove(NAUTILUS, 0.4F, 2)` of the idle activity.
const MAKE_LOVE_CLOSE_ENOUGH: i32 = 2;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `SetWalkTargetFromLookTarget.create(1.0F, 3)` of the
/// idle gate.
const WALK_TO_LOOK_TARGET_CLOSE_ENOUGH: i32 = 3;
/// How close a tempted baby follows the food, and how close an adult does.
///
/// Vanilla parity: the `mob -> mob.isBaby() ? 2.5 : 3.5` of the idle activity.
const TEMPTED_CLOSE_ENOUGH_BABY: f64 = 2.5;
const TEMPTED_CLOSE_ENOUGH_ADULT: f64 = 3.5;

/// The coin flip a nautilus makes before it goes looking for a fight.
///
/// Vanilla parity: the `random.nextFloat() < 0.5F` of
/// `NautilusAi.findNearestValidAttackTarget`.
const UNPROVOKED_ATTACK_CHANCE: f32 = 0.5;

/// Vanilla parity: the sensor list of `Nautilus.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestAdult,
    SensorType::NearestPlayers,
    SensorType::HurtBy,
    SensorType::NautilusTemptations,
];

/// Vanilla parity: `Nautilus.BRAIN_PROVIDER` plus `NautilusAi.getActivities`.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new_with_memories(
        SENSORS,
        &[
            memory_module_types::ANGRY_AT.id(),
            memory_module_types::ATTACK_TARGET_COOLDOWN.id(),
        ],
        vec![core_activity(), idle_activity(), fight_activity()],
    )
}

/// Vanilla parity: `NautilusAi.initCoreActivity`.
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
                memory_module_types::CHARGE_COOLDOWN_TICKS,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::ATTACK_TARGET_COOLDOWN,
            )),
        ],
    )
}

/// Vanilla parity: `NautilusAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                1,
                Behavior::boxed(AnimalMakeLove::new(
                    &vanilla_entities::NAUTILUS,
                    SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                    MAKE_LOVE_CLOSE_ENOUGH,
                )),
            ),
            (2, Behavior::boxed(tempted_by_food())),
            (3, start_attacking_what_it_fights()),
            (4, idle_swim_gate()),
        ],
    )
}

/// Vanilla parity: `NautilusAi.initFightActivity`.
///
/// The fight ends the moment a rider tempts it, a mate turns up, or the charge
/// cooldown lands, which is why the conditions are spelled out rather than
/// gated on the attack target alone.
fn fight_activity() -> ActivityData {
    ActivityData::create(
        Activity::Fight,
        0,
        vec![Behavior::boxed(ChargeAttack::new(
            TIME_BETWEEN_ATTACKS,
            attack_target_conditions(),
            SPEED_WHEN_ATTACKING,
            ATTACK_KNOCKBACK_FORCE,
            MAX_CHARGE_DISTANCE,
            MAX_TARGET_DETECTION_DISTANCE,
            &sound_events::ENTITY_NAUTILUS_DASH,
        ))],
    )
    .with_conditions(vec![
        (
            memory_module_types::ATTACK_TARGET.id(),
            MemoryStatus::ValuePresent,
        ),
        (
            memory_module_types::TEMPTING_PLAYER.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::BREED_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ),
        (
            memory_module_types::CHARGE_COOLDOWN_TICKS.id(),
            MemoryStatus::ValueAbsent,
        ),
    ])
}

/// Vanilla parity: the `FollowTemptation` both nautilus brains build, differing
/// only in how fast they follow.
pub(super) fn tempted_at_speed(speed_multiplier: f64) -> FollowTemptation {
    FollowTemptation::new(move |_| speed_multiplier).with_close_enough_distance(|body| {
        if body.as_ageable_mob().is_some_and(AgeableMob::is_baby) {
            TEMPTED_CLOSE_ENOUGH_BABY
        } else {
            TEMPTED_CLOSE_ENOUGH_ADULT
        }
    })
}

fn tempted_by_food() -> FollowTemptation {
    tempted_at_speed(SPEED_MULTIPLIER_WHEN_TEMPTED)
}

/// Vanilla parity: the shared idle `GateBehavior`, which swims somewhere at
/// random or towards whatever the nautilus is already looking at.
pub(super) fn idle_swim_gate() -> Box<dyn BehaviorControl> {
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
                OneShot::boxed(RandomStroll::swim(SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER)),
                2,
            ),
            (
                OneShot::boxed(SetWalkTargetFromLookTarget::new(
                    SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER,
                    WALK_TO_LOOK_TARGET_CLOSE_ENOUGH,
                )),
                3,
            ),
        ],
    ))
}

/// Vanilla parity: the `StartAttacking.create(NautilusAi::findNearestValidAttackTarget)`
/// both nautilus brains put in their idle activity.
pub(super) fn start_attacking_what_it_fights() -> Box<dyn BehaviorControl> {
    OneShot::boxed(StartAttacking::new(find_nearest_valid_attack_target))
}

/// Vanilla parity: `NautilusAi.ATTACK_TARGET_CONDITIONS`.
///
/// An armor stand is only a valid target where mob griefing is on, and nothing
/// outside the world border is one at all.
pub(super) fn attack_target_conditions() -> TargetingConditions {
    use foton_registry::vanilla_game_rules::MOB_GRIEFING;

    TargetingConditions::for_combat().selector(|_, target, world| {
        let is_armor_stand = target.entity_type() == &vanilla_entities::ARMOR_STAND;
        (world.get_game_rule(&MOB_GRIEFING) || !is_armor_stand)
            && world
                .world_border_snapshot()
                .is_within_bounds(target.bounding_box())
    })
}

/// Vanilla parity: `NautilusAi.findNearestValidAttackTarget`.
///
/// A nautilus that is courting, out of the water, a baby or tame picks no
/// fights. One that is none of those goes for whoever angered it first, and
/// only then -- once its long cooldown has run out, and only on a coin flip --
/// for the nearest thing in the hostiles tag.
pub fn find_nearest_valid_attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    let body = ctx.mob();
    let brain = ctx.brain();
    if brain.has_memory_value(memory_module_types::BREED_TARGET.id())
        || !body.is_in_water()
        || body.as_ageable_mob().is_some_and(AgeableMob::is_baby)
        || is_tamed(body.as_entity_event_source())
    {
        return None;
    }

    let body_living = body.as_entity_event_source().as_living_entity()?;
    let angry_at = living_entity_from_uuid_memory(
        ctx.world(),
        brain,
        memory_module_types::ANGRY_AT,
    )
    .filter(|entity| {
        entity.is_in_water()
            && entity.as_living_entity().is_some_and(|living| {
                is_entity_attackable_ignoring_line_of_sight(ctx.world(), body_living, living)
            })
    });
    if angry_at.is_some() {
        return angry_at;
    }

    if brain.has_memory_value(memory_module_types::ATTACK_TARGET_COOLDOWN.id()) {
        return None;
    }

    brain.set_memory(
        memory_module_types::ATTACK_TARGET_COOLDOWN,
        sample_time_between_non_player_attacks(),
    );
    if rand::random::<f32>() < UNPROVOKED_ATTACK_CHANCE {
        return None;
    }

    brain
        .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        .and_then(|visible| visible.find_closest(is_hostile_target))
}

/// Vanilla parity: `NautilusAi.isHostileTarget`.
fn is_hostile_target(mob: &dyn LivingEntity) -> bool {
    mob.is_in_water()
        && REGISTRY
            .entity_types
            .is_in_tag(mob.entity_type(), &EntityTypeTag::NAUTILUS_HOSTILES)
}

/// Vanilla parity: `NautilusAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
}
