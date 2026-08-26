//! Vanilla `AdultSensor`.

use std::ptr;

use super::Sensor;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};

/// Remembers the nearest grown-up of the body's own kind.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.AdultSensor`. It is
/// what a baby follows.
pub struct AdultSensor;

impl Sensor for AdultSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_VISIBLE_ADULT.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(visible) = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        else {
            // Vanilla's `ifPresent` leaves the old memory alone when the
            // visible-entity memory has not been filled in yet.
            return;
        };

        let body_type = ctx.mob().entity_type();
        let adult = visible.find_closest(|candidate| {
            ptr::eq(candidate.entity_type(), body_type) && !candidate.is_baby()
        });
        ctx.brain().set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_ADULT,
            adult.as_ref().map(EntityMemory::new),
        );
    }
}
