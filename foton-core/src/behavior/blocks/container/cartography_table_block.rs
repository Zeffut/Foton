//! Cartography table behavior.
//!
//! Vanilla parity: `CartographyTableBlock`. It opens a menu and nothing else --
//! it does not even face a direction.

use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::inventory::menu::kinds::cartography;
use crate::player::Player;
use crate::world::World;

/// Behavior for the cartography table.
#[block_behavior]
pub struct CartographyTableBlock {
    block: BlockRef,
}

impl CartographyTableBlock {
    /// Creates a cartography table behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for CartographyTableBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    /// Vanilla parity: `CartographyTableBlock.useWithoutItem`.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(maps) = player.server().map_data.for_world(world).map(Arc::clone) else {
            return InteractionResult::Fail;
        };
        let inventory = player.inventory.clone();
        let world = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_CARTOGRAPHY_TABLE.msg()),
            move |context| cartography(inventory, context.container_id, pos, &world, maps),
        );
        InteractionResult::Success
    }
}
