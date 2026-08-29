//! Vanilla `WardenEntitySensor`.

use foton_utils::Downcast as _;

use super::{NearestLivingEntitySensor, Sensor};
use crate::entity::SharedEntity;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::entities::WardenEntity;

/// Picks the one nearby entity a warden would go and look at.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.WardenEntitySensor`, which
/// extends the nearest-living sensor and adds one memory. Players are preferred over
/// everything else regardless of distance, which is why a warden mid-sniff walks past a
/// closer mob to reach you.
pub struct WardenEntitySensor;

impl Sensor for WardenEntitySensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        let mut memories = NearestLivingEntitySensor.required_memories();
        memories.push(memory_module_types::NEAREST_ATTACKABLE.id());
        memories
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        NearestLivingEntitySensor.do_tick(ctx);

        let Some(warden) = ctx.mob().downcast_ref::<WardenEntity>() else {
            return;
        };
        let can_target = |entity: &SharedEntity| warden.can_target_entity(Some(entity.as_ref()));
        let nearby = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_LIVING_ENTITIES)
            .unwrap_or_default();
        let closest = |want_player: bool| {
            nearby
                .iter()
                .filter_map(EntityMemory::get)
                .find(|entity| can_target(entity) && entity.as_player().is_some() == want_player)
        };

        match closest(true).or_else(|| closest(false)) {
            Some(entity) => ctx.brain().set_memory(
                memory_module_types::NEAREST_ATTACKABLE,
                EntityMemory::new(&entity),
            ),
            None => ctx
                .brain()
                .erase_memory(memory_module_types::NEAREST_ATTACKABLE.id()),
        }
    }
}
