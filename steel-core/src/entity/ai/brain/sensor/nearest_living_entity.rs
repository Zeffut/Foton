//! Vanilla `NearestLivingEntitySensor`.

use rustc_hash::FxHashSet;

use super::{Sensor, follow_range, is_entity_targetable};
use crate::entity::LivingEntity;
use crate::entity::SharedEntity;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleId, NearestVisibleLivingEntities, memory_module_types,
};

/// Remembers every living entity within follow range, nearest first.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.NearestLivingEntitySensor`.
pub struct NearestLivingEntitySensor;

impl Sensor for NearestLivingEntitySensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_LIVING_ENTITIES.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let range = follow_range(body);
        let search_area = body.bounding_box().inflate_xyz(range, range, range);
        let body_id = body.id();

        let mut nearby: Vec<SharedEntity> =
            ctx.world()
                .get_entities_in_aabb_matching(&search_area, |entity| {
                    entity.id() != body_id
                        && entity
                            .as_living_entity()
                            .is_some_and(LivingEntity::is_alive)
                });
        let body_position = body.position();
        nearby.sort_by(|left, right| {
            let left = left.position().distance_squared(body_position);
            let right = right.position().distance_squared(body_position);
            left.total_cmp(&right)
        });

        let mut visible = FxHashSet::default();
        for entity in &nearby {
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            if is_entity_targetable(ctx.world(), body, living) {
                visible.insert(entity.id());
            }
        }

        let remembered: Vec<EntityMemory> = nearby.iter().map(EntityMemory::new).collect();
        ctx.brain().set_memory(
            memory_module_types::NEAREST_LIVING_ENTITIES,
            remembered.clone(),
        );
        ctx.brain().set_memory(
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES,
            NearestVisibleLivingEntities::new(remembered, visible),
        );
    }
}
