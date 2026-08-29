//! The trial spawner block.
//!
//! Vanilla parity: `net.minecraft.world.level.block.TrialSpawnerBlock`.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::vanilla_block_entity_types;
use foton_utils::{BlockPos, BlockStateId};

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::BlockPlaceContext;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::world::World;

/// Vanilla `TrialSpawnerBlock`.
///
/// The block itself is nearly empty: it owns the two state properties, which
/// the extracted block registry already carries, and hands the tick to the
/// block entity.
#[block_behavior]
pub struct TrialSpawnerBlock {
    _block: BlockRef,
}

impl TrialSpawnerBlock {
    /// Creates the trial spawner block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { _block: block }
    }
}

impl BlockBehavior for TrialSpawnerBlock {
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
            &vanilla_block_entity_types::TRIAL_SPAWNER,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: `TrialSpawnerBlock.getTicker`, whose server branch reads
    /// the ominous property off the live state and hands it to `tickServer`.
    /// Foton's block entity reads the same property itself, so the ticker is the
    /// plain one.
    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::TRIAL_SPAWNER,
        )
    }
}
