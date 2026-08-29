//! Stonecutter behavior.
//!
//! Vanilla parity: `StonecutterBlock`. The block does almost nothing -- it
//! faces the player and opens a menu -- because the interesting part is the
//! three hundred and nineteen ways a stonecutter can cut a block, and those
//! live in the recipe registry.

use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use foton_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::inventory::menu::kinds::stonecutter;
use crate::player::Player;
use crate::world::World;

/// Which way the saw faces.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Behavior for the stonecutter.
#[block_behavior]
pub struct StonecutterBlock {
    block: BlockRef,
}

impl StonecutterBlock {
    /// Creates a stonecutter behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for StonecutterBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.horizontal_direction().opposite()),
        )
    }

    /// Vanilla parity: `StonecutterBlock.useWithoutItem`.
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
            TextComponent::translated(translations::CONTAINER_STONECUTTER.msg()),
            move |context| stonecutter(inventory, context.container_id, pos),
        );
        InteractionResult::Success
    }
}
