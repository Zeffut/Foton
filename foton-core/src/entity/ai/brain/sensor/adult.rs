//! Vanilla `AdultSensor` and the any-type variant under it.

use std::ptr;

use foton_registry::vanilla_entity_type_tags::EntityTypeTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _};

use super::Sensor;
use crate::entity::LivingEntity;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};

/// The memories both adult sensors write and read.
///
/// Vanilla parity: `AdultSensor.requires`.
fn adult_sensor_memories() -> Vec<MemoryModuleId> {
    vec![
        memory_module_types::NEAREST_VISIBLE_ADULT.id(),
        memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
    ]
}

/// Writes the closest visible entity `is_adult` accepts into the adult memory.
///
/// Vanilla parity: `AdultSensor.setNearestVisibleAdult`, the one method
/// `AdultSensorAnyType` overrides.
fn remember_nearest_visible_adult(
    ctx: &BrainContext<'_>,
    is_adult: impl FnMut(&dyn LivingEntity) -> bool,
) {
    let Some(visible) = ctx
        .brain()
        .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
    else {
        // Vanilla's `ifPresent` leaves the old memory alone when the
        // visible-entity memory has not been filled in yet.
        return;
    };

    let adult = visible.find_closest(is_adult);
    ctx.brain().set_memory_or_erase(
        memory_module_types::NEAREST_VISIBLE_ADULT,
        adult.as_ref().map(EntityMemory::new),
    );
}

/// Remembers the nearest grown-up of the body's own kind.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.AdultSensor`. It is
/// what a baby follows.
pub struct AdultSensor;

impl Sensor for AdultSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        adult_sensor_memories()
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body_type = ctx.mob().entity_type();
        remember_nearest_visible_adult(ctx, |candidate| {
            ptr::eq(candidate.entity_type(), body_type) && !candidate.is_baby()
        });
    }
}

/// Remembers the nearest grown-up of any kind a baby will trail after.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.AdultSensorAnyType`,
/// which swaps the same-kind test for the `followable_friendly_mobs` tag. A
/// ghastling follows any adult on that list, not only another happy ghast.
pub struct AdultSensorAnyType;

impl Sensor for AdultSensorAnyType {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        adult_sensor_memories()
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        remember_nearest_visible_adult(ctx, |candidate| {
            REGISTRY.entity_types.is_in_tag(
                candidate.entity_type(),
                &EntityTypeTag::FOLLOWABLE_FRIENDLY_MOBS,
            ) && !candidate.is_baby()
        });
    }
}
