//! The monster spawner block.
//!
//! Vanilla parity: `net.minecraft.world.level.block.SpawnerBlock`.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_block_entity_types;
use foton_utils::{BlockPos, BlockStateId};

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::BlockPlaceContext;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::world::World;

/// Vanilla parity: the `15 + random.nextInt(15) + random.nextInt(15)` of
/// `SpawnerBlock.spawnAfterBreak`.
const EXPERIENCE_BASE: i32 = 15;
/// The width of each of the two extra rolls.
const EXPERIENCE_ROLL: i32 = 15;

/// Vanilla `SpawnerBlock`.
#[block_behavior]
pub struct SpawnerBlock {
    _block: BlockRef,
}

impl SpawnerBlock {
    /// Creates the spawner block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { _block: block }
    }
}

impl BlockBehavior for SpawnerBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        None
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::MOB_SPAWNER,
            level,
            pos,
            state,
        ))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::MOB_SPAWNER,
        )
    }

    /// Vanilla parity: `BaseEntityBlock.triggerEvent`, which hands the event to
    /// the block entity and lets its answer decide whether the packet goes out.
    /// A spawner sends itself one every time it re-arms, and that packet is
    /// what resets the spinning mob on the client.
    fn trigger_event(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        param_a: i32,
        param_b: i32,
    ) -> bool {
        world
            .get_block_entity(pos)
            .is_some_and(|block_entity| block_entity.trigger_event(param_a, param_b))
    }

    /// Vanilla parity: `SpawnerBlock.spawnAfterBreak`, which pops its
    /// experience directly rather than through `tryDropExperience` -- a spawner
    /// pays the same whatever the pickaxe was enchanted with.
    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _tool: &ItemStack,
        drop_experience: bool,
    ) {
        if !drop_experience {
            return;
        }
        let experience = EXPERIENCE_BASE
            + rand::random_range(0..EXPERIENCE_ROLL)
            + rand::random_range(0..EXPERIENCE_ROLL);
        world.pop_experience(pos, experience);
    }
}
