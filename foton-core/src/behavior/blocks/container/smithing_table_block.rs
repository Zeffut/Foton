//! Smithing table behavior.
//!
//! Vanilla parity: `SmithingTableBlock`. It opens a menu and nothing else --
//! it does not even face a direction.

use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::inventory::menu::kinds::smithing;
use crate::player::Player;
use crate::world::World;

/// Behavior for the smithing table.
#[block_behavior]
pub struct SmithingTableBlock {
    block: BlockRef,
}

impl SmithingTableBlock {
    /// Creates a smithing table behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for SmithingTableBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    /// Vanilla parity: `SmithingTableBlock.useWithoutItem`.
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
            TextComponent::translated(translations::CONTAINER_UPGRADE.msg()),
            move |context| smithing(inventory, context.container_id, pos),
        );
        InteractionResult::Success
    }
}
