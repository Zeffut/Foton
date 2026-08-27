//! Vanilla `MoveToSkySeeingSpot`.

use glam::DVec3;
use steel_utils::BlockPos;

use crate::chunk::heightmap::HeightmapType;
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::behavior::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::world::World;

/// How many positions are tried before the villager gives up on the sky.
///
/// Vanilla parity: the `for (int i = 0; i < 10; i++)` of `getOutdoorPosition`.
const OUTDOOR_ATTEMPTS: i32 = 10;

/// The horizontal spread of one attempt.
///
/// Vanilla parity: the `random.nextInt(20) - 10` of `getOutdoorPosition`.
const OUTDOOR_HORIZONTAL_SPREAD: i32 = 20;

/// The vertical spread of one attempt.
///
/// Vanilla parity: the `random.nextInt(6) - 3` of `getOutdoorPosition`.
const OUTDOOR_VERTICAL_SPREAD: i32 = 6;

/// Whether `target` is open to the sky and not above the body.
///
/// Vanilla parity: `MoveToSkySeeingSpot.hasNoBlocksAbove`. The height
/// comparison is what stops a villager celebrating on a roof it cannot reach:
/// a column is only a candidate if its motion-blocking top is at or below where
/// the villager already stands.
#[must_use]
pub fn has_no_blocks_above(world: &World, body: &dyn PathfinderMob, target: BlockPos) -> bool {
    world.can_see_sky(target)
        && f64::from(
            world
                .heightmap_pos(HeightmapType::MotionBlocking, target)
                .y(),
        ) <= body.position().y
}

/// Vanilla parity: the private `MoveToSkySeeingSpot.getOutdoorPosition`.
fn outdoor_position(world: &World, body: &dyn PathfinderMob) -> Option<DVec3> {
    let pos = body.block_position();
    for _ in 0..OUTDOOR_ATTEMPTS {
        let candidate = pos.offset(
            rand::random_range(0..OUTDOOR_HORIZONTAL_SPREAD) - OUTDOOR_HORIZONTAL_SPREAD / 2,
            rand::random_range(0..OUTDOOR_VERTICAL_SPREAD) - OUTDOOR_VERTICAL_SPREAD / 2,
            rand::random_range(0..OUTDOOR_HORIZONTAL_SPREAD) - OUTDOOR_HORIZONTAL_SPREAD / 2,
        );
        if has_no_blocks_above(world, body, candidate) {
            // Vanilla parity: `Vec3.atBottomCenterOf`, which centers the column
            // but keeps the block's own floor.
            let (x, _, z) = candidate.get_center();
            return Some(DVec3::new(x, f64::from(candidate.y()), z));
        }
    }
    None
}

/// Walks a villager out from under a roof.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.MoveToSkySeeingSpot`.
/// It is the first thing the RAID package offers once the village has won,
/// because the fireworks
/// [`CelebrateVillagersSurvivedRaid`](crate::entity::entities::mobs::npc::villager_ai::CelebrateVillagersSurvivedRaid)
/// throws are only thrown from a block that can see the sky.
pub struct MoveToSkySeeingSpot {
    speed_modifier: f64,
}

impl MoveToSkySeeingSpot {
    /// Vanilla parity: `MoveToSkySeeingSpot.create`.
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier }
    }
}

impl Trigger for MoveToSkySeeingSpot {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::WALK_TARGET.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }

        let world = ctx.world();
        let mob = ctx.mob();
        // Vanilla returns `false` from under open sky, so a villager already
        // outdoors leaves the roll to whatever else the gate is offering.
        if world.can_see_sky(mob.block_position()) {
            return false;
        }

        if let Some(target) = outdoor_position(world, mob) {
            brain.set_memory(
                memory_module_types::WALK_TARGET,
                WalkTarget::of_position(target, self.speed_modifier, 0),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "MoveToSkySeeingSpot"
    }
}
