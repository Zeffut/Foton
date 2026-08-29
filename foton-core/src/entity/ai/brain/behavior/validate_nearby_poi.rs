//! Vanilla `ValidateNearbyPoi`.

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::BlockStateProperties;
use foton_registry::poi::PoiTypeRef;
use foton_registry::{REGISTRY, RegistryExt as _};
use foton_utils::{BlockPos, Downcast as _, GlobalPos, WorldAabb};

use super::{BrainContext, Trigger, utils};
use crate::behavior::BlockStateBehaviorExt as _;
use crate::entity::LivingEntity;
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryModuleType};
use crate::entity::entities::mobs::npc::VillagerEntity;
use crate::world::World;

/// Vanilla parity: `ValidateNearbyPoi.MAX_DISTANCE`.
const MAX_DISTANCE: f64 = 16.0;

/// Which POI types the remembered position is still allowed to be.
///
/// Context-aware for the same reason [`AcquirePoi`]'s is: vanilla re-binds the
/// predicate by rebuilding the brain when a villager changes profession.
///
/// [`AcquirePoi`]: super::AcquirePoi
type PoiTypeFilter = Box<dyn Fn(&BrainContext<'_>, PoiTypeRef) -> bool + Send>;

/// Forgets a remembered point of interest once it stops being one.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.ValidateNearbyPoi`.
/// A villager only re-checks what it can see -- within sixteen blocks -- so
/// mining somebody's bed out while they are away does not un-claim it until
/// they come home.
pub struct ValidateNearbyPoi {
    poi_type: PoiTypeFilter,
    memory: MemoryModuleType<GlobalPos>,
}

impl ValidateNearbyPoi {
    /// Vanilla parity: `ValidateNearbyPoi.create`.
    #[must_use]
    pub fn new(
        poi_type: impl Fn(&BrainContext<'_>, PoiTypeRef) -> bool + Send + 'static,
        memory: MemoryModuleType<GlobalPos>,
    ) -> Self {
        Self {
            poi_type: Box::new(poi_type),
            memory,
        }
    }
}

impl Trigger for ValidateNearbyPoi {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![self.memory.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(remembered) = brain.get_memory(self.memory) else {
            return false;
        };
        let world = ctx.world();
        let pos = remembered.pos;
        if remembered.dimension != world.key
            || !utils::block_closer_to_center_than(pos, ctx.mob().position(), MAX_DISTANCE)
        {
            return false;
        }

        let still_a_poi = {
            let storage = world.poi_storage.lock();
            storage.exists(pos, &|poi_type_id| {
                REGISTRY
                    .poi_types
                    .by_id(poi_type_id)
                    .is_some_and(|poi_type| (self.poi_type)(ctx, poi_type))
            })
        };
        if !still_a_poi {
            brain.erase_memory(self.memory.id());
            return true;
        }

        if bed_is_occupied(world, pos, ctx.mob().is_sleeping()) {
            brain.erase_memory(self.memory.id());
            // Somebody else is in the bed but is not a villager -- a player, or
            // a villager that has since been removed -- so the ticket that is
            // still booked against it belongs to nobody. Give it back.
            if !bed_is_occupied_by_villager(world, pos) {
                let mut storage = world.poi_storage.lock();
                let _released = storage.release_ticket(pos);
            }
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "ValidateNearbyPoi"
    }
}

/// Vanilla parity: the private `ValidateNearbyPoi.bedIsOccupied`.
fn bed_is_occupied(world: &World, pos: BlockPos, body_is_sleeping: bool) -> bool {
    let state = world.get_block_state(pos);
    state.is_bed() && state.get_value(&BlockStateProperties::OCCUPIED) && !body_is_sleeping
}

/// Vanilla parity: the private `ValidateNearbyPoi.bedIsOccupiedByVillager`,
/// whose `new AABB(BlockPos)` is that block's own cube.
fn bed_is_occupied_by_villager(world: &World, pos: BlockPos) -> bool {
    let aabb = WorldAabb::new(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
        f64::from(pos.x() + 1),
        f64::from(pos.y() + 1),
        f64::from(pos.z() + 1),
    );
    !world
        .get_entities_in_aabb_matching(&aabb, |entity| {
            entity
                .downcast_ref::<VillagerEntity>()
                .is_some_and(LivingEntity::is_sleeping)
        })
        .is_empty()
}
