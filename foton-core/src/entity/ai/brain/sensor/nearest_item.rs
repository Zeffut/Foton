//! Vanilla `NearestItemSensor`.

use foton_utils::Downcast as _;

use super::Sensor;
use crate::entity::SharedEntity;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::entities::ItemEntity;

/// How far out this sensor looks, horizontally.
///
/// Vanilla parity: `NearestItemSensor.XZ_RANGE`.
const XZ_RANGE: f64 = 32.0;
/// Vanilla parity: `NearestItemSensor.Y_RANGE`.
const Y_RANGE: f64 = 16.0;
/// Vanilla parity: `NearestItemSensor.MAX_DISTANCE_TO_WANTED_ITEM`.
const MAX_DISTANCE_TO_WANTED_ITEM: f64 = 32.0;

/// Remembers the nearest dropped item the body would pick up.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.NearestItemSensor`.
pub struct NearestItemSensor;

impl Sensor for NearestItemSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::NEAREST_VISIBLE_WANTED_ITEM.id()]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let search_area = body.bounding_box().inflate_xyz(XZ_RANGE, Y_RANGE, XZ_RANGE);
        let mut items: Vec<SharedEntity> = ctx
            .world()
            .get_entities_in_aabb_matching(&search_area, |entity| {
                entity.downcast_ref::<ItemEntity>().is_some()
            });

        let body_position = body.position();
        items.sort_by(|left, right| {
            let left = left.position().distance_squared(body_position);
            let right = right.position().distance_squared(body_position);
            left.total_cmp(&right)
        });

        let wanted = items.into_iter().find(|entity| {
            let Some(item_entity) = entity.downcast_ref::<ItemEntity>() else {
                return false;
            };
            body.wants_to_pick_up(ctx.world(), &item_entity.get_item())
                && entity.position().distance_squared(body_position)
                    <= MAX_DISTANCE_TO_WANTED_ITEM * MAX_DISTANCE_TO_WANTED_ITEM
                && body.has_line_of_sight_cached(entity.as_ref())
        });

        ctx.brain().set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_WANTED_ITEM,
            wanted.as_ref().map(EntityMemory::new),
        );
    }
}
