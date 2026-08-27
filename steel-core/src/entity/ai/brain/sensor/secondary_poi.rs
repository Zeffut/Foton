//! Vanilla `SecondaryPoiSensor`.

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_utils::{Downcast as _, GlobalPos};

use super::Sensor;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::entities::VillagerEntity;

/// Vanilla parity: the `40` a `SecondaryPoiSensor` is constructed with.
const SCAN_RATE: i32 = 40;
/// Vanilla parity: the `horizontalSearch` of `SecondaryPoiSensor.doTick`.
const HORIZONTAL_SEARCH: i32 = 4;
/// Vanilla parity: the `-2..=2` the same loop walks vertically.
const VERTICAL_SEARCH: i32 = 2;

/// Remembers the blocks a villager's profession works on top of its workstation.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.SecondaryPoiSensor`.
/// Only the farmer has a secondary POI at all, and the farmland this writes is
/// what `HarvestFarmland` and `StrollToPoiList` are gated on.
pub struct SecondaryPoiSensor;

impl Sensor for SecondaryPoiSensor {
    fn scan_rate(&self) -> i32 {
        SCAN_RATE
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::SECONDARY_JOB_SITE.id()]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(villager) = ctx.mob().downcast_ref::<VillagerEntity>() else {
            return;
        };
        let profession = villager.profession();
        let world = ctx.world();
        let center = ctx.mob().block_position();

        let mut job_sites = Vec::new();
        for x in -HORIZONTAL_SEARCH..=HORIZONTAL_SEARCH {
            for y in -VERTICAL_SEARCH..=VERTICAL_SEARCH {
                for z in -HORIZONTAL_SEARCH..=HORIZONTAL_SEARCH {
                    let pos = center.offset(x, y, z);
                    if profession.is_secondary_poi(world.get_block_state(pos).get_block()) {
                        job_sites.push(GlobalPos::new(world.key.clone(), pos));
                    }
                }
            }
        }

        if job_sites.is_empty() {
            ctx.brain()
                .erase_memory(memory_module_types::SECONDARY_JOB_SITE.id());
        } else {
            ctx.brain()
                .set_memory(memory_module_types::SECONDARY_JOB_SITE, job_sites);
        }
    }
}
