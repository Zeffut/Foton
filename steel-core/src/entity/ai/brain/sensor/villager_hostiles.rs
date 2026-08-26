//! Vanilla `VillagerHostilesSensor`.

use std::ptr;

use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entities;

use super::Sensor;

use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};

/// How close each kind of monster has to be before a villager panics.
///
/// Vanilla parity: `VillagerHostilesSensor.ACCEPTABLE_DISTANCE_FROM_HOSTILES`.
/// A villager is braver about a zombie than about a pillager, and anything not
/// on this list does not frighten it at all.
const ACCEPTABLE_DISTANCE_FROM_HOSTILES: &[(EntityTypeRef, f64)] = &[
    (&vanilla_entities::DROWNED, 8.0),
    (&vanilla_entities::EVOKER, 12.0),
    (&vanilla_entities::HUSK, 8.0),
    (&vanilla_entities::ILLUSIONER, 12.0),
    (&vanilla_entities::PILLAGER, 15.0),
    (&vanilla_entities::RAVAGER, 12.0),
    (&vanilla_entities::VEX, 8.0),
    (&vanilla_entities::VINDICATOR, 10.0),
    (&vanilla_entities::ZOGLIN, 10.0),
    (&vanilla_entities::ZOMBIE, 8.0),
    (&vanilla_entities::ZOMBIE_VILLAGER, 8.0),
];

/// Remembers the nearest monster a villager is frightened of.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.VillagerHostilesSensor`,
/// a `NearestVisibleLivingEntitySensor` that reads the visible-mob memory rather
/// than scanning the world itself.
pub struct VillagerHostilesSensor;

impl Sensor for VillagerHostilesSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            memory_module_types::NEAREST_HOSTILE.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body_position = ctx.mob().position();
        let nearest = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
            .and_then(|visible| {
                visible.find_closest(|candidate| {
                    acceptable_distance(candidate.entity_type()).is_some_and(|distance| {
                        candidate.position().distance_squared(body_position) <= distance * distance
                    })
                })
            });
        ctx.brain().set_memory_or_erase(
            memory_module_types::NEAREST_HOSTILE,
            nearest.map(|entity| EntityMemory::new(&entity)),
        );
    }
}

/// Vanilla parity: the `ACCEPTABLE_DISTANCE_FROM_HOSTILES.get(mob.getType())`
/// of `isClose`, which is also the `containsKey` of `isHostile`.
fn acceptable_distance(entity_type: EntityTypeRef) -> Option<f64> {
    ACCEPTABLE_DISTANCE_FROM_HOSTILES
        .iter()
        .find(|(candidate, _)| ptr::eq(*candidate, entity_type))
        .map(|&(_, distance)| distance)
}
