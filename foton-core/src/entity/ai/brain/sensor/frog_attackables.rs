//! Vanilla `FrogAttackablesSensor`.

use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::entities::FrogEntity;
use crate::entity::{LivingEntity, SharedEntity};

use super::{Sensor, follow_range, is_entity_attackable};

/// Vanilla parity: `FrogAttackablesSensor.TARGET_DETECTION_DISTANCE`.
const TARGET_DETECTION_DISTANCE: f64 = 10.0;

/// Remembers the nearest thing a frog would swallow.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.FrogAttackablesSensor`,
/// a `NearestVisibleLivingEntitySensor` whose match is "attackable, edible, and
/// not one of the five targets the tongue has already failed to reach".
pub struct FrogAttackablesSensor;

impl Sensor for FrogAttackablesSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_ATTACKABLE.id(),
            memory_module_types::UNREACHABLE_TONGUE_TARGETS.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let range = follow_range(body);
        let search_area = body.bounding_box().inflate_xyz(range, range, range);
        let body_id = body.id();
        let body_position = body.position();
        let unreachable = ctx
            .brain()
            .get_memory(memory_module_types::UNREACHABLE_TONGUE_TARGETS)
            .unwrap_or_default();

        let mut candidates: Vec<SharedEntity> =
            ctx.world()
                .get_entities_in_aabb_matching(&search_area, |entity| {
                    if entity.id() == body_id {
                        return false;
                    }
                    let Some(living) = entity.as_living_entity() else {
                        return false;
                    };
                    if !LivingEntity::is_alive(living) || !FrogEntity::can_eat(living) {
                        return false;
                    }
                    if unreachable.contains(&entity.uuid()) {
                        return false;
                    }
                    if entity.position().distance(body_position) >= TARGET_DETECTION_DISTANCE {
                        return false;
                    }
                    is_entity_attackable(ctx.world(), body, living)
                });

        candidates.sort_by(|left, right| {
            let left = left.position().distance_squared(body_position);
            let right = right.position().distance_squared(body_position);
            left.total_cmp(&right)
        });

        ctx.brain().set_memory_or_erase(
            memory_module_types::NEAREST_ATTACKABLE,
            candidates.first().map(EntityMemory::new),
        );
    }
}
