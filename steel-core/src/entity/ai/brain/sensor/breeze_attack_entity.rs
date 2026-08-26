//! Vanilla `BreezeAttackEntitySensor`.

use super::{Sensor, is_entity_attackable};
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::ai::brain::sensor::NearestLivingEntitySensor;

/// Picks the nearest thing a breeze would shoot at.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.BreezeAttackEntitySensor`,
/// a `NearestLivingEntitySensor` that also writes `NEAREST_ATTACKABLE` from the
/// first attackable entry of the list its base just filled. Order matters: the
/// base sorts by distance, so "first" is "nearest".
///
/// Vanilla also filters with `EntitySelector.NO_CREATIVE_OR_SPECTATOR` before
/// `Sensor.isEntityAttackable`. That filter is subsumed: a spectator fails
/// `canBeSeenByAnyone` and a creative player is invulnerable and so fails
/// `canBeSeenAsEnemy`, both inside the same `isEntityAttackable`.
pub struct BreezeAttackEntitySensor {
    nearest_living_entities: NearestLivingEntitySensor,
}

impl BreezeAttackEntitySensor {
    /// Creates the sensor.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nearest_living_entities: NearestLivingEntitySensor,
        }
    }
}

impl Default for BreezeAttackEntitySensor {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for BreezeAttackEntitySensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        let mut required = self.nearest_living_entities.required_memories();
        required.push(memory_module_types::NEAREST_ATTACKABLE.id());
        required
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        self.nearest_living_entities.do_tick(ctx);

        let body = ctx.mob();
        let nearest = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_LIVING_ENTITIES)
            .unwrap_or_default()
            .into_iter()
            .find(|remembered| {
                remembered.get().is_some_and(|entity| {
                    entity
                        .as_living_entity()
                        .is_some_and(|living| is_entity_attackable(ctx.world(), body, living))
                })
            });

        ctx.brain()
            .set_memory_or_erase(memory_module_types::NEAREST_ATTACKABLE, nearest);
    }
}
