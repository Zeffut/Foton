//! Vanilla `AxolotlAttackablesSensor`.

use foton_registry::vanilla_entity_type_tags::EntityTypeTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _};

use crate::entity::SharedEntity;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};

use super::{Sensor, follow_range, is_entity_attackable};

/// Vanilla parity: `AxolotlAttackablesSensor.TARGET_DETECTION_DISTANCE`, which
/// the sensor squares before comparing.
const TARGET_DETECTION_DISTANCE_SQR: f64 = 64.0;

/// Remembers the nearest thing an axolotl would pick a fight with.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.AxolotlAttackablesSensor`.
/// Two lists feed it: the drowned and the guardians it always hates, and the
/// fish it hunts -- but only while it has no hunting cooldown, which is what
/// stops one axolotl clearing a whole reef.
pub struct AxolotlAttackablesSensor;

impl Sensor for AxolotlAttackablesSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_ATTACKABLE.id(),
            memory_module_types::HAS_HUNTING_COOLDOWN.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let range = follow_range(body);
        let search_area = body.bounding_box().inflate_xyz(range, range, range);
        let body_id = body.id();
        let body_position = body.position();
        // Vanilla reads the cooldown once per candidate; the answer cannot
        // change inside one scan, so it is read once here instead.
        let hunting = !ctx
            .brain()
            .has_memory_value(memory_module_types::HAS_HUNTING_COOLDOWN.id());

        let mut candidates: Vec<SharedEntity> =
            ctx.world()
                .get_entities_in_aabb_matching(&search_area, |entity| {
                    if entity.id() == body_id {
                        return false;
                    }
                    let Some(living) = entity.as_living_entity() else {
                        return false;
                    };
                    if entity.position().distance_squared(body_position)
                        > TARGET_DETECTION_DISTANCE_SQR
                    {
                        return false;
                    }
                    if !living.is_in_water() {
                        return false;
                    }
                    let always_hostile = REGISTRY.entity_types.is_in_tag(
                        entity.entity_type(),
                        &EntityTypeTag::AXOLOTL_ALWAYS_HOSTILES,
                    );
                    let hunt_target = hunting
                        && REGISTRY
                            .entity_types
                            .is_in_tag(entity.entity_type(), &EntityTypeTag::AXOLOTL_HUNT_TARGETS);
                    if !always_hostile && !hunt_target {
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
