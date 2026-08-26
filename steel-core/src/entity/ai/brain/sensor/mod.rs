//! Sensors: what writes the world into a brain's memories.

mod adult;
mod axolotl_attackables;
mod breeze_attack_entity;
mod frog_attackables;
mod hurt_by;
mod is_in_water;
mod mob_sensor;
mod nearest_item;
mod nearest_living_entity;
mod piglin_specific;
mod player;
mod tempting;
mod villager_hostiles;
mod warden_specific;

pub use adult::{AdultSensor, AdultSensorAnyType};
pub use axolotl_attackables::AxolotlAttackablesSensor;
pub use breeze_attack_entity::BreezeAttackEntitySensor;
pub use frog_attackables::FrogAttackablesSensor;
pub use hurt_by::HurtBySensor;
pub use is_in_water::IsInWaterSensor;
pub use mob_sensor::MobSensor;
pub use nearest_item::NearestItemSensor;
pub use nearest_living_entity::NearestLivingEntitySensor;
pub use piglin_specific::{
    HoglinSpecificSensor, PiglinBruteSpecificSensor, PiglinSpecificSensor, is_zombified,
};
pub use player::PlayerSensor;
pub use tempting::TemptingSensor;
pub use villager_hostiles::VillagerHostilesSensor;
pub use warden_specific::WardenEntitySensor;

use steel_registry::vanilla_attributes;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _};

use super::context::BrainContext;
use super::memory::{MemoryModuleId, memory_module_types};
use crate::entity::LivingEntity;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::entities::ArmadilloEntity;
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

/// Vanilla parity: the `5` scan rate of `SensorType.ARMADILLO_SCARE_DETECTED`,
/// four times a second rather than once.
const ARMADILLO_SCARE_SCAN_RATE: i32 = 5;
/// Vanilla parity: the `80` tick life of the danger memory it sets.
const ARMADILLO_DANGER_MEMORY_TICKS: i64 = 80;

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
    /// Vanilla `SensorType.NAUTILUS_TEMPTATIONS`.
    NautilusTemptations,
    /// Vanilla `SensorType.FROG_ATTACKABLES`.
    FrogAttackables,
    /// Vanilla `SensorType.AXOLOTL_ATTACKABLES`.
    AxolotlAttackables,
    /// Vanilla `SensorType.ARMADILLO_SCARE_DETECTED`.
    ArmadilloScareDetected,
    /// Vanilla `SensorType.NEAREST_ADULT`.
    NearestAdult,
    /// Vanilla `SensorType.NEAREST_ADULT_ANY_TYPE`.
    NearestAdultAnyType,
    /// Vanilla `SensorType.PIGLIN_SPECIFIC_SENSOR`.
    PiglinSpecific,
    /// Vanilla `SensorType.PIGLIN_BRUTE_SPECIFIC_SENSOR`.
    PiglinBruteSpecific,
    /// Vanilla `SensorType.HOGLIN_SPECIFIC_SENSOR`.
    HoglinSpecific,
    /// Vanilla `SensorType.BREEZE_ATTACK_ENTITY_SENSOR`.
    BreezeAttackEntity,
    /// Vanilla `SensorType.WARDEN_ENTITY_SENSOR`.
    WardenEntity,
    /// Vanilla `SensorType.VILLAGER_HOSTILES`.
    VillagerHostiles,
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
            // Vanilla parity: `SensorType.NAUTILUS_TEMPTATIONS`, built from
            // `NautilusAi.getTemptations()` -- the `#minecraft:nautilus_food`
            // tag, not the mob's own `isFood`, so an untamed nautilus that only
            // eats its taming items still follows a player holding food.
            Self::NautilusTemptations => Box::new(TemptingSensor::new(|_, item_stack| {
                REGISTRY
                    .items
                    .is_in_tag(item_stack.item(), &ItemTag::NAUTILUS_FOOD)
            })),
            Self::FrogAttackables => Box::new(FrogAttackablesSensor),
            Self::AxolotlAttackables => Box::new(AxolotlAttackablesSensor),
            // Vanilla parity: `SensorType.ARMADILLO_SCARE_DETECTED`, whose four
            // arguments are the whole sensor -- the armadillo supplies both
            // predicates itself.
            Self::ArmadilloScareDetected => Box::new(MobSensor::new(
                ARMADILLO_SCARE_SCAN_RATE,
                |body, other| {
                    use steel_utils::Downcast as _;

                    body.downcast_ref::<ArmadilloEntity>()
                        .is_some_and(|armadillo| armadillo.is_scared_by(other))
                },
                |body| {
                    use steel_utils::Downcast as _;

                    body.downcast_ref::<ArmadilloEntity>()
                        .is_some_and(ArmadilloEntity::can_stay_rolled_up)
                },
                memory_module_types::DANGER_DETECTED_RECENTLY,
                ARMADILLO_DANGER_MEMORY_TICKS,
            )),
            Self::NearestAdult => Box::new(AdultSensor),
            Self::NearestAdultAnyType => Box::new(AdultSensorAnyType),
            Self::PiglinSpecific => Box::new(PiglinSpecificSensor),
            Self::PiglinBruteSpecific => Box::new(PiglinBruteSpecificSensor),
            Self::HoglinSpecific => Box::new(HoglinSpecificSensor),
            Self::BreezeAttackEntity => Box::new(BreezeAttackEntitySensor::new()),
            Self::WardenEntity => Box::new(WardenEntitySensor),
            Self::VillagerHostiles => Box::new(VillagerHostilesSensor),
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

/// Returns whether `body` could attack `target` even without seeing it.
///
/// Vanilla parity: `Sensor.isEntityAttackableIgnoringLineOfSight`, which is how
/// a nautilus stays angry at something that swam behind a rock.
#[must_use]
pub(crate) fn is_entity_attackable_ignoring_line_of_sight(
    world: &World,
    body: &dyn LivingEntity,
    target: &dyn LivingEntity,
) -> bool {
    let conditions = TargetingConditions::for_combat()
        .range(follow_range(body))
        .ignore_line_of_sight();
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
