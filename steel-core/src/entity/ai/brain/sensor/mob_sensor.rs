//! Vanilla `MobSensor`.

use crate::entity::LivingEntity;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryModuleType, memory_module_types};

use super::Sensor;

/// Whether one particular nearby mob counts.
type MobTest = Box<dyn Fn(&dyn LivingEntity, &dyn LivingEntity) -> bool + Send>;
/// Whether the body is in any state to care.
type ReadyTest = Box<dyn Fn(&dyn LivingEntity) -> bool + Send>;

/// Sets a boolean memory while some nearby mob matches.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.MobSensor`. It reads
/// `NEAREST_LIVING_ENTITIES` rather than scanning itself, so it costs nothing
/// beyond the scan the nearest-living sensor already did -- which is why it can
/// run four times a second.
pub struct MobSensor {
    scan_rate: i32,
    mob_test: MobTest,
    ready_test: ReadyTest,
    to_set: MemoryModuleType<bool>,
    memory_time_to_live: i64,
}

impl MobSensor {
    /// Vanilla parity: the `MobSensor(int, BiPredicate, Predicate, MemoryModuleType, int)`
    /// constructor.
    #[must_use]
    pub fn new(
        scan_rate: i32,
        mob_test: impl Fn(&dyn LivingEntity, &dyn LivingEntity) -> bool + Send + 'static,
        ready_test: impl Fn(&dyn LivingEntity) -> bool + Send + 'static,
        to_set: MemoryModuleType<bool>,
        memory_time_to_live: i64,
    ) -> Self {
        Self {
            scan_rate,
            mob_test: Box::new(mob_test),
            ready_test: Box::new(ready_test),
            to_set,
            memory_time_to_live,
        }
    }
}

impl Sensor for MobSensor {
    fn scan_rate(&self) -> i32 {
        self.scan_rate
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_LIVING_ENTITIES.id(),
            self.to_set.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        if !(self.ready_test)(body) {
            ctx.brain().erase_memory(self.to_set.id());
            return;
        }

        let Some(nearby) = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_LIVING_ENTITIES)
        else {
            return;
        };
        let present = nearby.iter().any(|remembered| {
            remembered.get().is_some_and(|entity| {
                entity
                    .as_living_entity()
                    .is_some_and(|other| (self.mob_test)(body, other))
            })
        });
        if present {
            ctx.brain()
                .set_memory_with_expiry(self.to_set, true, self.memory_time_to_live);
        }
    }
}
