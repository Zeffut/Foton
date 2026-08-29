//! Vanilla `ReactToBell` and `RingBell`.

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_blocks;

use crate::behavior::blocks::BellBlock;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::behavior::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};

/// The chance a villager standing at the bell leaves it alone this tick.
///
/// Vanilla parity: `RingBell.BELL_RING_CHANCE`, whose `nextFloat() <= 0.95F`
/// means one tick in twenty actually rings.
const BELL_RING_CHANCE: f32 = 0.95;

/// How close a villager has to be to reach the rope.
///
/// Vanilla parity: `RingBell.RING_BELL_FROM_DISTANCE`.
const RING_BELL_FROM_DISTANCE: f64 = 3.0;

/// Sends a villager that heard a bell into hiding.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.ReactToBell`.
///
/// The `HEARD_BELL_TIME` this reads is written by [`BellBlockEntity`] on
/// everything within thirty-two blocks of a rung bell. The raid check is what
/// keeps the two halves of the village apart: a bell rung during a raid is the
/// alarm the `PRE_RAID` and RAID packages are already answering, and this would
/// only pull the villager out of them, so it stands down and lets
/// [`SetRaidStatus`] keep the villager where it is.
///
/// [`BellBlockEntity`]: crate::block_entity::entities::BellBlockEntity
/// [`SetRaidStatus`]: super::SetRaidStatus
pub struct ReactToBell;

impl Trigger for ReactToBell {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::HEARD_BELL_TIME.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if !ctx
            .brain()
            .has_memory_value(memory_module_types::HEARD_BELL_TIME.id())
        {
            return false;
        }
        if ctx
            .world()
            .get_raid_at(ctx.mob().block_position())
            .is_none()
        {
            ctx.brain().set_active_activity_if_possible(Activity::Hide);
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "ReactToBell"
    }
}

/// Rings the village bell a villager is standing at.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.RingBell`. It runs
/// only in the `PRE_RAID` package, which is also what walks the villager to the
/// meeting point in the first place -- so the raid alarm is a villager reaching
/// the bell and pulling it, rather than anything the raid does itself.
pub struct RingBell;

impl Trigger for RingBell {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::MEETING_POINT.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(meeting_point) = ctx.brain().get_memory(memory_module_types::MEETING_POINT) else {
            return false;
        };
        if rand::random::<f32>() <= BELL_RING_CHANCE {
            return false;
        }

        // Vanilla returns `true` once the roll passes, whether or not the bell
        // was in reach or still standing.
        let mob = ctx.mob();
        let pos = meeting_point.pos;
        if !utils::block_closer_than(pos, mob.block_position(), RING_BELL_FROM_DISTANCE) {
            return true;
        }
        let world = ctx.world();
        if world.get_block_state(pos).get_block() == &vanilla_blocks::BELL {
            BellBlock::attempt_to_ring(world, pos, None, Some(mob));
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "RingBell"
    }
}
