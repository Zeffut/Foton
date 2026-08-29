//! Vanilla `VillagerBabiesSensor`.

use foton_registry::vanilla_entities;

use super::Sensor;

use crate::entity::ai::brain::behavior::utils;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};

/// Remembers the village's other children.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.VillagerBabiesSensor`.
/// It scans nothing of its own: it filters the visible-mobs memory
/// `NearestLivingEntitySensor` already fills, so its range is whatever that one
/// last wrote and its cost is a walk over a short list.
pub struct VillagerBabiesSensor;

impl Sensor for VillagerBabiesSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::VISIBLE_VILLAGER_BABIES.id()]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let babies = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
            .map(|visible| {
                visible.find_all(|candidate| {
                    utils::is_of_type(candidate, &vanilla_entities::VILLAGER) && candidate.is_baby()
                })
            })
            .unwrap_or_default();
        // Vanilla writes the list whether or not it is empty; the memory map
        // erases an empty collection either way, which is what leaves
        // `VISIBLE_VILLAGER_BABIES` absent for the play package to gate on.
        ctx.brain().set_memory(
            memory_module_types::VISIBLE_VILLAGER_BABIES,
            babies.iter().map(EntityMemory::new).collect::<Vec<_>>(),
        );
    }
}
