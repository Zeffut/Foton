//! Sensors: what writes the world into a brain's memories.

mod adult;
mod axolotl_attackables;
mod frog_attackables;
mod hurt_by;
mod is_in_water;
mod nearest_item;
mod nearest_living_entity;
mod piglin_specific;
mod player;
mod tempting;

pub use adult::AdultSensor;
pub use axolotl_attackables::AxolotlAttackablesSensor;
pub use frog_attackables::FrogAttackablesSensor;
pub use hurt_by::HurtBySensor;
pub use is_in_water::IsInWaterSensor;
pub use nearest_item::NearestItemSensor;
pub use nearest_living_entity::NearestLivingEntitySensor;
pub use piglin_specific::{
    HoglinSpecificSensor, PiglinBruteSpecificSensor, PiglinSpecificSensor, is_zombified,
};
pub use player::PlayerSensor;
pub use tempting::TemptingSensor;

use steel_registry::vanilla_attributes;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _};

use super::context::BrainContext;
use super::memory::{MemoryModuleId, memory_module_types};
use crate::entity::LivingEntity;
use crate::entity::ai::targeting::TargetingConditions;
use crate::world::World;

/// How often a sensor rescans when it does not say otherwise.
///
/// Vanilla parity: `Sensor.DEFAULT_SCAN_RATE`.
pub const DEFAULT_SCAN_RATE: i32 = 20;

/// The range every shared `TargetingConditions` in vanilla's `Sensor` starts at.
///
/// Vanilla parity: `Sensor.DEFAULT_TARGETING_RANGE`. Vanilla mutates six shared
/// static `TargetingConditions` to the body's follow range on every tick, which
/// is a data race waiting to happen; Steel builds the conditions per call
/// instead, so the constant is only the fallback when a body has no follow
/// range attribute.
const DEFAULT_TARGETING_RANGE: f64 = 16.0;

/// Something that periodically writes what it observes into a brain.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.Sensor`. Vanilla's
/// `tick` is `final` and owns the scan-rate countdown; here the countdown lives
/// in the brain so the trait stays a single overridable surface.
pub trait Sensor: Send {
    /// Ticks between rescans.
    ///
    /// Vanilla parity: the `scanRate` constructor argument.
    fn scan_rate(&self) -> i32 {
        DEFAULT_SCAN_RATE
    }

    /// The memories this sensor writes, so the brain can register them.
    ///
    /// Vanilla parity: `Sensor.requires`.
    fn required_memories(&self) -> Vec<MemoryModuleId>;

    /// Rescans the world.
    ///
    /// Vanilla parity: `Sensor.doTick`.
    fn do_tick(&mut self, ctx: &BrainContext<'_>);
}

/// Which sensor a brain asks for.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.SensorType`. Like
/// `Activity` and `MemoryModuleType`, vanilla's registry is a hardcoded Java
/// list that never reaches a packet or a save file, and `SteelExtractor` emits no
/// `sensor_type` asset, so the constants are mirrored as an enum. Only the
/// sensors a Steel mob drives are here; the rest (`WARDEN_ENTITY_SENSOR`,
/// `GOLEM_DETECTED`, ...) arrive with their mobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorType {
    /// Vanilla `SensorType.NEAREST_LIVING_ENTITIES`.
    NearestLivingEntities,
    /// Vanilla `SensorType.NEAREST_PLAYERS`.
    NearestPlayers,
    /// Vanilla `SensorType.NEAREST_ITEMS`.
    NearestItems,
    /// Vanilla `SensorType.HURT_BY`.
    HurtBy,
    /// Vanilla `SensorType.IS_IN_WATER`.
    IsInWater,
    /// Vanilla `SensorType.FOOD_TEMPTATIONS`.
    FoodTemptations,
    /// Vanilla `SensorType.FROG_TEMPTATIONS`.
    FrogTemptations,
    /// Vanilla `SensorType.FROG_ATTACKABLES`.
    FrogAttackables,
    /// Vanilla `SensorType.AXOLOTL_ATTACKABLES`.
    AxolotlAttackables,
    /// Vanilla `SensorType.NEAREST_ADULT`.
    NearestAdult,
    /// Vanilla `SensorType.PIGLIN_SPECIFIC_SENSOR`.
    PiglinSpecific,
    /// Vanilla `SensorType.PIGLIN_BRUTE_SPECIFIC_SENSOR`.
    PiglinBruteSpecific,
    /// Vanilla `SensorType.HOGLIN_SPECIFIC_SENSOR`.
    HoglinSpecific,
}

impl SensorType {
    /// Vanilla parity: `SensorType.create`.
    #[must_use]
    pub fn create(self) -> Box<dyn Sensor> {
        match self {
            Self::NearestLivingEntities => Box::new(NearestLivingEntitySensor),
            Self::NearestPlayers => Box::new(PlayerSensor),
            Self::NearestItems => Box::new(NearestItemSensor),
            Self::HurtBy => Box::new(HurtBySensor),
            Self::IsInWater => Box::new(IsInWaterSensor),
            Self::FoodTemptations => Box::new(TemptingSensor::for_animal()),
            // Vanilla parity: `SensorType.FROG_TEMPTATIONS`, which tempts on the
            // item tag rather than on the mob's own `isFood` -- that is what
            // lets a tadpole, which is a fish and has no `isFood`, be led along
            // by the same slime ball a frog follows.
            Self::FrogTemptations => Box::new(TemptingSensor::new(|_, item_stack| {
                REGISTRY
                    .items
                    .is_in_tag(item_stack.item(), &ItemTag::FROG_FOOD)
            })),
            Self::FrogAttackables => Box::new(FrogAttackablesSensor),
            Self::AxolotlAttackables => Box::new(AxolotlAttackablesSensor),
            Self::NearestAdult => Box::new(AdultSensor),
            Self::PiglinSpecific => Box::new(PiglinSpecificSensor),
            Self::PiglinBruteSpecific => Box::new(PiglinBruteSpecificSensor),
            Self::HoglinSpecific => Box::new(HoglinSpecificSensor),
        }
    }
}

/// The distance a body notices things from.
///
/// Vanilla parity: `body.getAttributeValue(Attributes.FOLLOW_RANGE)`.
#[must_use]
pub(crate) fn follow_range(body: &dyn LivingEntity) -> f64 {
    body.attributes()
        .lock()
        .get_value(vanilla_attributes::FOLLOW_RANGE)
        .unwrap_or(DEFAULT_TARGETING_RANGE)
}

/// Returns whether `body` currently notices `target`.
///
/// Vanilla parity: `Sensor.isEntityTargetable`. The invisibility test is
/// skipped for whatever the body is already attacking, so a mob does not lose
/// track of a target that drinks invisibility mid-fight.
#[must_use]
pub(crate) fn is_entity_targetable(
    world: &World,
    body: &dyn LivingEntity,
    target: &dyn LivingEntity,
) -> bool {
    let conditions = TargetingConditions::for_non_combat().range(follow_range(body));
    let conditions = if is_current_attack_target(body, target) {
        conditions.ignore_invisibility_testing()
    } else {
        conditions
    };
    conditions.test(world, Some(body), target)
}

/// Returns whether `body` could attack `target` if it wanted to.
///
/// Vanilla parity: `Sensor.isEntityAttackable`.
#[must_use]
pub(crate) fn is_entity_attackable(
    world: &World,
    body: &dyn LivingEntity,
    target: &dyn LivingEntity,
) -> bool {
    let conditions = TargetingConditions::for_combat().range(follow_range(body));
    let conditions = if is_current_attack_target(body, target) {
        conditions.ignore_invisibility_testing()
    } else {
        conditions
    };
    conditions.test(world, Some(body), target)
}

fn is_current_attack_target(body: &dyn LivingEntity, target: &dyn LivingEntity) -> bool {
    let Some(mob) = body.as_mob() else {
        return false;
    };
    let Some(brain) = mob.brain() else {
        return false;
    };
    brain
        .get_memory(memory_module_types::ATTACK_TARGET)
        .is_some_and(|attack_target| attack_target.id() == target.id())
}
