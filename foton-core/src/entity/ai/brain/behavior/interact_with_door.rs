//! Vanilla `InteractWithDoor`.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::BlockStateProperties;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _};
use foton_utils::{BlockPos, BlockStateId, GlobalPos};
use rustc_hash::FxHashSet;

use super::{BrainContext, Trigger, utils};
use crate::behavior::BLOCK_BEHAVIORS;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::ai::node::Node;
use crate::entity::{Mob, PathfinderMob};
use crate::world::World;

/// How long the same path node is left alone after one look at it.
///
/// Vanilla parity: `InteractWithDoor.COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE`.
const COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE: i32 = 20;

/// How far a mob may walk from a door it opened before it stops being its
/// business.
///
/// Vanilla parity: `InteractWithDoor.SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN`.
const SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN: f64 = 3.0;

/// How close another mob has to be to the door to be worth holding it for.
///
/// Vanilla parity: `InteractWithDoor.MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS`.
const MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS: f64 = 2.0;

/// Opens the doors a mob is about to walk through, and closes them behind it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.InteractWithDoor`.
///
/// It reads the `PATH` memory rather than the navigation, which is what makes
/// it a brain behavior at all: the two nodes it looks at are the one the mob is
/// leaving and the one it is walking into, and a door standing in either is
/// opened before the mob reaches it. Every door it opens -- and every door it
/// walked out of, opened or not -- goes into `DOORS_TO_CLOSE`, and it is that
/// set the mob works back through as it walks away.
///
/// The node cooldown is what keeps this cheap: a mob that has not reached its
/// next node yet is left alone for a second rather than re-tested every tick.
pub struct InteractWithDoor {
    /// Vanilla parity: the `MutableObject<Node> lastCheckedNode` the builder
    /// closes over -- compared by position, which is the only part of a node
    /// that says which block it is.
    last_checked_node: Option<BlockPos>,
    /// Vanilla parity: the `MutableInt remainingCooldown`.
    remaining_cooldown: i32,
}

impl InteractWithDoor {
    /// Vanilla parity: `InteractWithDoor.create`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_checked_node: None,
            remaining_cooldown: 0,
        }
    }
}

impl Default for InteractWithDoor {
    fn default() -> Self {
        Self::new()
    }
}

impl Trigger for InteractWithDoor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::PATH.id(),
            memory_module_types::DOORS_TO_CLOSE.id(),
            memory_module_types::NEAREST_LIVING_ENTITIES.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        // Vanilla parity: the `i.present(PATH)` of the group.
        let Some(path) = brain.get_memory(memory_module_types::PATH) else {
            return false;
        };
        if path.not_started() || path.is_done() {
            return false;
        }
        let (Some(from), Some(to)) = (
            path.previous_node().map(Node::as_block_pos),
            path.next_node().map(Node::as_block_pos),
        ) else {
            return false;
        };

        if self.last_checked_node == Some(to) {
            self.remaining_cooldown = COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE;
        } else {
            self.remaining_cooldown -= 1;
            if self.remaining_cooldown > 0 {
                return false;
            }
        }
        self.last_checked_node = Some(to);

        let world = ctx.world();
        let mob = ctx.mob();
        // The node behind is remembered whether or not it had to be opened:
        // a mob that walked out through a door somebody else left open still
        // shuts it.
        if let Some(state) = interactable_door(world, from) {
            open_door(world, mob, state, from, true);
            remember_door(brain, world, from);
        }
        // The node ahead is only remembered when this mob is the one that
        // opened it, so a door already standing open is left as it was found.
        if let Some(state) = interactable_door(world, to)
            && !is_open(state)
        {
            open_door(world, mob, state, to, true);
            remember_door(brain, world, to);
        }

        close_doors_behind(world, mob, brain, Some(from), Some(to));
        true
    }

    fn debug_name(&self) -> &'static str {
        "InteractWithDoor"
    }
}

/// Shuts every door in `DOORS_TO_CLOSE` this mob has finished with.
///
/// Vanilla parity: `InteractWithDoor.closeDoorsThatIHaveOpenedOrPassedThrough`,
/// which is public upstream because [`SleepInBed`] calls it too -- a villager
/// getting into bed shuts what it left open on the way. Vanilla is handed the
/// set and mutates it in place; Foton reads and writes the memory here, so both
/// callers pass the brain rather than unpacking it themselves.
///
/// A door is dropped from the set on every path but one: the mob is still
/// standing in it. Everything else -- out of range, no longer a door, already
/// shut, or another of its kind about to come through -- is a reason to stop
/// tracking it, and vanilla drops it in each case whether or not it shut
/// anything.
///
/// [`SleepInBed`]: super::SleepInBed
pub fn close_doors_behind(
    world: &Arc<World>,
    mob: &dyn PathfinderMob,
    brain: &Brain,
    moving_from: Option<BlockPos>,
    moving_to: Option<BlockPos>,
) {
    let Some(doors) = brain.get_memory(memory_module_types::DOORS_TO_CLOSE) else {
        return;
    };
    let nearest = brain.get_memory(memory_module_types::NEAREST_LIVING_ENTITIES);

    let mut still_tracked = FxHashSet::default();
    for door in doors {
        if moving_from == Some(door.pos) || moving_to == Some(door.pos) {
            still_tracked.insert(door);
            continue;
        }
        if is_door_too_far_away(world, mob, &door) {
            continue;
        }
        let Some(state) = interactable_door(world, door.pos) else {
            continue;
        };
        if !is_open(state) {
            continue;
        }
        if other_mobs_coming_through_door(mob, door.pos, nearest.as_deref()) {
            continue;
        }
        open_door(world, mob, state, door.pos, false);
    }
    brain.set_memory(memory_module_types::DOORS_TO_CLOSE, still_tracked);
}

/// Returns the state at `pos` when it is a door a mob is allowed to work.
///
/// Vanilla parity: the `state.is(BlockTags.MOB_INTERACTABLE_DOORS, s ->
/// s.getBlock() instanceof DoorBlock)` both halves of this behavior test --
/// the tag *and* the class. The tag is what actually decides anything: every
/// block vanilla puts in it is a door, so the class half only ever matters to a
/// datapack that puts something else there, and nothing in Foton can observe
/// the difference today. It is carried anyway because it costs one call and it
/// is what stops a mistaken tag turning a chest into something a village
/// swings open.
fn interactable_door(world: &World, pos: BlockPos) -> Option<BlockStateId> {
    let state = world.get_block_state(pos);
    let block = state.get_block();
    let interactable = REGISTRY
        .blocks
        .is_in_tag(block, &BlockTag::MOB_INTERACTABLE_DOORS)
        && BLOCK_BEHAVIORS.get_behavior(block).is_wooden_door(state);
    interactable.then_some(state)
}

/// Vanilla parity: `DoorBlock.isOpen`.
fn is_open(state: BlockStateId) -> bool {
    state.get_value(&BlockStateProperties::OPEN)
}

/// Vanilla parity: `DoorBlock.setOpen`, which no-ops when the door is already
/// in the state asked for.
fn open_door(
    world: &Arc<World>,
    mob: &dyn PathfinderMob,
    state: BlockStateId,
    pos: BlockPos,
    open: bool,
) {
    BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .set_door_open(state, world, pos, Some(mob.as_entity_event_source()), open);
}

/// Vanilla parity: the private `InteractWithDoor.rememberDoorToClose`.
fn remember_door(brain: &Brain, world: &World, pos: BlockPos) {
    let mut doors = brain
        .get_memory(memory_module_types::DOORS_TO_CLOSE)
        .unwrap_or_default();
    doors.insert(GlobalPos::new(world.key.clone(), pos));
    brain.set_memory(memory_module_types::DOORS_TO_CLOSE, doors);
}

/// Vanilla parity: the private `InteractWithDoor.isDoorTooFarAway`.
fn is_door_too_far_away(world: &World, mob: &dyn PathfinderMob, door: &GlobalPos) -> bool {
    door.dimension != world.key
        || !utils::block_closer_to_center_than(
            door.pos,
            mob.position(),
            SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN,
        )
}

/// Vanilla parity: the private `InteractWithDoor.areOtherMobsComingThroughDoor`.
///
/// Only mobs of the same kind hold a door for each other, which is what stops a
/// village holding its doors open for the zombies chasing it.
fn other_mobs_coming_through_door(
    mob: &dyn PathfinderMob,
    door_pos: BlockPos,
    nearest: Option<&[EntityMemory]>,
) -> bool {
    let Some(nearest) = nearest else {
        return false;
    };
    nearest.iter().any(|memory| {
        let Some(other) = memory.get() else {
            return false;
        };
        if !utils::is_of_type(other.as_ref(), mob.entity_type())
            || !utils::block_closer_to_center_than(
                door_pos,
                other.position(),
                MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS,
            )
        {
            return false;
        }
        other
            .as_mob()
            .and_then(Mob::brain)
            .is_some_and(|brain| is_mob_coming_through_door(brain, door_pos))
    })
}

/// Vanilla parity: the private `InteractWithDoor.isMobComingThroughDoor`.
fn is_mob_coming_through_door(other_brain: &Brain, door_pos: BlockPos) -> bool {
    let Some(path) = other_brain.get_memory(memory_module_types::PATH) else {
        return false;
    };
    if path.is_done() {
        return false;
    }
    let Some(from) = path.previous_node() else {
        return false;
    };
    from.as_block_pos() == door_pos
        || path
            .next_node()
            .is_some_and(|to| to.as_block_pos() == door_pos)
}
