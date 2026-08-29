//! Sculk catalyst behavior.
//!
//! Vanilla parity: `SculkCatalystBlock`.
//!
//! The catalyst is the block that pays for sculk. Its block entity listens for a mob dying
//! within eight blocks, takes the experience that death was about to drop, and spends it as
//! sculk growing outward; the block itself only carries the `bloom` pulse that shows it
//! happened and the experience it drops when mined. See `SculkCatalystBlockEntity`.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_block_entity_types;
use foton_utils::types::UpdateFlags;
use foton_utils::value_providers::IntProvider;
use foton_utils::{BlockPos, BlockStateId};

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::BlockPlaceContext;
use crate::behavior::try_drop_experience;
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::BlockEntityTicker;
use crate::world::World;

/// Vanilla `SculkCatalystBlock.PULSE`, which is the `bloom` block state property.
const PULSE: &BoolProperty = &BlockStateProperties::BLOOM;

/// Vanilla `SculkCatalystBlock`.
#[block_behavior]
pub struct SculkCatalystBlock {
    block: BlockRef,
    #[json_arg(int_provider, json = "xp_range")]
    experience: IntProvider,
}

impl SculkCatalystBlock {
    /// Creates the sculk catalyst behavior with its extracted experience provider.
    #[must_use]
    pub const fn new(block: BlockRef, experience: IntProvider) -> Self {
        Self { block, experience }
    }
}

impl BlockBehavior for SculkCatalystBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state().set_value(PULSE, false))
    }

    /// Vanilla `SculkCatalystBlock.tick`: the bloom lasts eight ticks and then clears.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !state.get_value(PULSE) {
            return;
        }
        world.set_block(pos, state.set_value(PULSE, false), UpdateFlags::UPDATE_ALL);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::SCULK_CATALYST,
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
            &vanilla_block_entity_types::SCULK_CATALYST,
        )
    }

    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            try_drop_experience(world, pos, tool, &self.experience);
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use foton_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The bloom is a one-shot pulse driven by a scheduled tick. A catalyst that never
    /// cleared it would sit lit forever after the first death near it.
    #[test]
    fn a_blooming_catalyst_goes_dark_again_on_its_scheduled_tick() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let pos = BlockPos::new(8, 70, 8);
        let world = fresh_test_world("sculk_catalyst_bloom");
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let behavior =
            SculkCatalystBlock::new(&vanilla_blocks::SCULK_CATALYST, IntProvider::Constant(5));
        let blooming = vanilla_blocks::SCULK_CATALYST
            .default_state()
            .set_value(PULSE, true);
        assert!(world.set_block(pos, blooming, UpdateFlags::UPDATE_ALL));

        behavior.tick(blooming, &world, pos);

        assert!(!world.get_block_state(pos).get_value(PULSE));
    }
}
