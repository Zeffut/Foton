//! Vanilla `SoulSandBlock` behavior.

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_utils::BlockStateId;

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::entity::ai::path::PathComputationType;

/// Vanilla `SoulSandBlock`.
///
/// Every other override in the vanilla class is extracted data: the fourteen
/// pixel collision box, the full support and visual boxes, and the shade
/// brightness. Only `isPathfindable` is left, and it cannot be derived -- the
/// shared default says yes to anything whose collision box is not a full cube,
/// which soul sand's deliberately is not, so mobs would happily path across it
/// and then sink.
#[block_behavior]
pub struct SoulSandBlock {
    block: BlockRef,
}

impl SoulSandBlock {
    /// Creates a soul sand behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for SoulSandBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::blocks::block_state_ext::BlockStateExt as _;
    use steel_registry::blocks::shapes::is_shape_full_block;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;

    #[test]
    fn soul_sand_refuses_the_land_path_its_sunken_collision_box_would_otherwise_allow() {
        init_vanilla_registry();
        let behavior = SoulSandBlock::new(&vanilla_blocks::SOUL_SAND);
        let state = vanilla_blocks::SOUL_SAND.default_state();

        // The shared default reads the collision shape, which stops two pixels
        // short of the top, and would therefore allow land and air paths.
        assert!(!is_shape_full_block(state.get_static_collision_shape()));

        assert!(!behavior.is_pathfindable(state, PathComputationType::Land));
        assert!(!behavior.is_pathfindable(state, PathComputationType::Air));
        assert!(!behavior.is_pathfindable(state, PathComputationType::Water));
    }
}
