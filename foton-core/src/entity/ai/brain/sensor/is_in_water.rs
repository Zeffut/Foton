//! Vanilla `IsInWaterSensor`.

use super::Sensor;

use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{MemoryModuleId, Unit, memory_module_types};

/// Remembers whether the body is in water.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.IsInWaterSensor`.
pub struct IsInWaterSensor;

impl Sensor for IsInWaterSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::IS_IN_WATER.id()]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        if ctx.mob().is_in_water() {
            ctx.brain()
                .set_memory(memory_module_types::IS_IN_WATER, Unit);
        } else {
            ctx.brain()
                .erase_memory(memory_module_types::IS_IN_WATER.id());
        }
    }
}
