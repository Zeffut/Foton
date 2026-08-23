//! Loom block behavior.
//!
//! Vanilla parity: `LoomBlock`. It faces the way it was placed and opens a
//! menu; everything else a loom does lives in that menu.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::inventory::menu::kinds::loom;
use crate::player::Player;
use crate::world::World;

/// Which way the loom faces.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Behavior for the loom block.
#[block_behavior]
pub struct LoomBlock {
    block: BlockRef,
}

impl LoomBlock {
    /// Creates the loom behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for LoomBlock {
    /// Vanilla parity: `LoomBlock.getStateForPlacement`.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.horizontal_direction().opposite()),
        )
    }

    /// Vanilla parity: `LoomBlock.useWithoutItem`.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_LOOM.msg()),
            move |context| loom(inventory.clone(), context.container_id, pos),
        );

        // TODO: Award stat INTERACT_WITH_LOOM; Steel has no statistics
        // registry.
        InteractionResult::Success
    }
}
