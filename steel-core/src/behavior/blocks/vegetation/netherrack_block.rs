//! Vanilla `NetherrackBlock` behavior.

use std::sync::Arc;

use rand::{Rng, RngExt as _};
use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use super::BlockRef;
use super::bonemealable::{BonemealAction, Bonemealable};
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::chunk::light::propagates_skylight_down;
use crate::world::{LevelReader, World};

/// Vanilla `NetherrackBlock`.
///
/// The whole class is the `BonemealableBlock` half: bone meal on netherrack
/// next to nylium turns it into that nylium.
#[block_behavior]
pub struct NetherrackBlock {
    block: BlockRef,
}

impl NetherrackBlock {
    /// Creates a netherrack behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// The `BlockPos.betweenClosed(pos.offset(-1, -1, -1), pos.offset(1, 1, 1))`
    /// cube both bone meal methods scan.
    fn surrounding(pos: BlockPos) -> impl Iterator<Item = BlockPos> {
        BlockPos::between_closed(pos.offset(-1, -1, -1), pos.offset(1, 1, 1))
    }
}

impl BlockBehavior for NetherrackBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn as_bonemealable(&self) -> Option<&dyn Bonemealable> {
        Some(self)
    }
}

impl Bonemealable for NetherrackBlock {
    fn is_valid_bonemeal_target(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> bool {
        if !propagates_skylight_down(world.get_block_state(pos.above())) {
            return false;
        }

        Self::surrounding(pos).any(|neighbour_pos| {
            world
                .get_block_state(neighbour_pos)
                .get_block()
                .has_tag(&BlockTag::NYLIUM)
        })
    }

    fn perform_bonemeal(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        rng: &mut dyn Rng,
        pos: BlockPos,
    ) {
        let mut found_crimson = false;
        let mut found_warped = false;

        for neighbour_pos in Self::surrounding(pos) {
            let block = world.get_block_state(neighbour_pos).get_block();
            if block == &vanilla_blocks::WARPED_NYLIUM {
                found_warped = true;
            }
            if block == &vanilla_blocks::CRIMSON_NYLIUM {
                found_crimson = true;
            }
            if found_warped && found_crimson {
                break;
            }
        }

        let grown = match (found_warped, found_crimson) {
            (true, true) => {
                if rng.random::<bool>() {
                    &vanilla_blocks::WARPED_NYLIUM
                } else {
                    &vanilla_blocks::CRIMSON_NYLIUM
                }
            }
            (true, false) => &vanilla_blocks::WARPED_NYLIUM,
            (false, true) => &vanilla_blocks::CRIMSON_NYLIUM,
            (false, false) => return,
        };

        world.set_block(pos, grown.default_state(), UpdateFlags::UPDATE_ALL);
    }

    fn bonemeal_action_type(&self) -> BonemealAction {
        BonemealAction::NeighborSpreader
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::TestLevel;

    #[test]
    fn netherrack_only_takes_bonemeal_under_open_sky_and_beside_nylium() {
        init_vanilla_registry();
        init_behaviors();

        let pos = BlockPos::new(0, 64, 0);
        let behavior = NetherrackBlock::new(&vanilla_blocks::NETHERRACK);
        let state = vanilla_blocks::NETHERRACK.default_state();

        let bare = TestLevel::default();
        assert!(!behavior.is_valid_bonemeal_target(state, &bare, pos));

        let beside_nylium = TestLevel::default().with_block(
            pos.offset(1, 0, 0),
            vanilla_blocks::CRIMSON_NYLIUM.default_state(),
        );
        assert!(behavior.is_valid_bonemeal_target(state, &beside_nylium, pos));

        // Netherrack cannot grow with a light-blocking block above it, even
        // with nylium in reach.
        let covered = TestLevel::default()
            .with_block(
                pos.offset(1, 0, 0),
                vanilla_blocks::CRIMSON_NYLIUM.default_state(),
            )
            .with_block(pos.above(), vanilla_blocks::NETHERRACK.default_state());
        assert!(!behavior.is_valid_bonemeal_target(state, &covered, pos));
    }
}
