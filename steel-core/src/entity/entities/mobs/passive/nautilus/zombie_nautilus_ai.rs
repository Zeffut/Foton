//! The zombie nautilus's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.nautilus.ZombieNautilusAi`.
//! The same three activities as the living nautilus, minus the panic and minus
//! the courting -- a zombie nautilus never breeds -- and a slower charge.

use steel_registry::sound_events;

use crate::entity::ai::brain::behavior::{
    Behavior, ChargeAttack, CountDownCooldownTicks, LookAtTargetSink, MoveToTargetSink,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};

use super::nautilus_ai::{
    ATTACK_KNOCKBACK_FORCE, MAX_CHARGE_DISTANCE, MAX_TARGET_DETECTION_DISTANCE,
    TIME_BETWEEN_ATTACKS, attack_target_conditions, idle_swim_gate, start_attacking_what_it_fights,
    tempted_at_speed,
};

/// Vanilla parity: `ZombieNautilusAi.SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f64 = 0.9;
/// Vanilla parity: `ZombieNautilusAi.SPEED_WHEN_ATTACKING`.
const SPEED_WHEN_ATTACKING: f32 = 0.5;

/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;

/// Vanilla parity: the sensor list of `ZombieNautilus.BRAIN_PROVIDER`.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestAdult,
    SensorType::NearestPlayers,
    SensorType::HurtBy,
    SensorType::NautilusTemptations,
];

/// Vanilla parity: `ZombieNautilus.BRAIN_PROVIDER` plus `ZombieNautilusAi.getActivities`.
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

/// Vanilla parity: `ZombieNautilusAi.initCoreActivity`, which is the living
/// nautilus's core without the panic.
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

/// Vanilla parity: `ZombieNautilusAi.initIdleActivity`.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                1,
                Behavior::boxed(tempted_at_speed(SPEED_MULTIPLIER_WHEN_TEMPTED)),
            ),
            (2, start_attacking_what_it_fights()),
            (3, idle_swim_gate()),
        ],
    )
}

/// Vanilla parity: `ZombieNautilusAi.initFightActivity`.
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
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_DASH,
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

/// Vanilla parity: `ZombieNautilusAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
}
