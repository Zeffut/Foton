//! Vanilla `GolemSensor`.

use foton_registry::vanilla_entities;

use super::Sensor;

use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::ai::brain::{Brain, behavior::utils};

/// How often a villager looks around for a golem.
///
/// Vanilla parity: `GolemSensor.GOLEM_SCAN_RATE`, ten seconds -- far slower
/// than most sensors, because the memory it writes lasts thirty.
const GOLEM_SCAN_RATE: i32 = 200;

/// How long a villager remembers having seen a golem.
///
/// Vanilla parity: `GolemSensor.MEMORY_TIME_TO_LIVE`. One tick short of six
/// hundred, so a village that already has a golem spends thirty seconds not
/// wanting another.
const MEMORY_TIME_TO_LIVE: i64 = 599;

/// Notices an iron golem standing in the village.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.GolemSensor`. It is
/// the whole of what stops a village raising a second golem: nothing counts
/// golems, and no golem is ever destroyed -- a villager simply refuses to want
/// one for thirty seconds after it last saw one.
pub struct GolemSensor;

impl GolemSensor {
    /// Vanilla parity: `GolemSensor.golemDetected`, the static every villager
    /// that agreed to a golem is passed once one has actually been raised.
    pub fn golem_detected(brain: &Brain) {
        brain.set_memory_with_expiry(
            memory_module_types::GOLEM_DETECTED_RECENTLY,
            true,
            MEMORY_TIME_TO_LIVE,
        );
    }
}

impl Sensor for GolemSensor {
    fn scan_rate(&self) -> i32 {
        GOLEM_SCAN_RATE
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_LIVING_ENTITIES.id(),
            memory_module_types::GOLEM_DETECTED_RECENTLY.id(),
        ]
    }

    /// Vanilla parity: `GolemSensor.checkForNearbyGolem`, which reads the
    /// nearby-mobs memory rather than scanning the world itself -- so the range
    /// is whatever `NearestLivingEntitySensor` last wrote.
    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(nearby) = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_LIVING_ENTITIES)
        else {
            return;
        };
        let golem_present = nearby.iter().any(|memory| {
            memory.get().is_some_and(|entity| {
                utils::is_of_type(entity.as_ref(), &vanilla_entities::IRON_GOLEM)
            })
        });
        if golem_present {
            Self::golem_detected(ctx.brain());
        }
    }
}
