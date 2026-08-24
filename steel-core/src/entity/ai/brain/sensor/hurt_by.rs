//! Vanilla `HurtBySensor`.

use std::sync::Arc;

use super::Sensor;
use crate::entity::LivingEntity;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};

/// Remembers what last hurt the body, and who did it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.HurtBySensor`.
pub struct HurtBySensor;

impl Sensor for HurtBySensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::HURT_BY.id(),
            memory_module_types::HURT_BY_ENTITY.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let brain = ctx.brain();

        match body.last_damage_source() {
            Some(source) => {
                let attacker = source
                    .causing_entity_id
                    .and_then(|id| ctx.world().get_entity_by_id(id))
                    .filter(|entity| entity.is_living_entity());
                brain.set_memory(memory_module_types::HURT_BY, source);
                if let Some(attacker) = attacker {
                    brain.set_memory(
                        memory_module_types::HURT_BY_ENTITY,
                        EntityMemory::new(&attacker),
                    );
                }
            }
            None => brain.erase_memory(memory_module_types::HURT_BY.id()),
        }

        // Vanilla parity: the trailing `ifPresent` that drops a remembered
        // attacker once it dies or leaves this level.
        let attacker_is_stale = brain
            .get_memory(memory_module_types::HURT_BY_ENTITY)
            .is_some_and(|attacker| {
                attacker.get().is_none_or(|entity| {
                    entity
                        .as_living_entity()
                        .is_none_or(|living| !LivingEntity::is_alive(living))
                        || entity
                            .level()
                            .is_none_or(|level| !Arc::ptr_eq(&level, ctx.world()))
                })
            });
        if attacker_is_stale {
            brain.erase_memory(memory_module_types::HURT_BY_ENTITY.id());
        }
    }
}
