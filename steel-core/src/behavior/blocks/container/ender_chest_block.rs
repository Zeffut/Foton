//! Ender chest behavior.
//!
//! Vanilla parity: `EnderChestBlock`. The one container whose contents belong
//! to the player rather than the block: every ender chest in the world opens
//! the same twenty-seven slots for whoever opens it, and those slots travel
//! with the player between worlds and through death.
//!
//! That is why there is almost nothing here. The block only decides *whether*
//! to open, and hands over the player's own container.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{BLOCK_BEHAVIORS, InventoryAccess};
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::inventory::menu::kinds::chest;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Which way the chest faces.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Rows the menu shows.
const MENU_ROWS: usize = 3;

/// Behavior for the ender chest.
#[block_behavior]
pub struct EnderChestBlock {
    block: BlockRef,
}

impl EnderChestBlock {
    /// Creates an ender chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for EnderChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.horizontal_direction().opposite()),
        )
    }

    /// Opens the player's own ender inventory.
    ///
    /// Vanilla parity: `EnderChestBlock.useWithoutItem`, including the refusal
    /// when something solid sits on the lid -- the same rule that stops a chest
    /// under a block from opening.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let above = pos.above();
        let above_state = world.get_block_state(above);
        if BLOCK_BEHAVIORS
            .get_behavior(above_state.get_block())
            .is_redstone_conductor(above_state, world.as_ref(), above)
        {
            return InteractionResult::Success;
        }

        let shared: SharedContainer = player.ender_chest.clone();
        let container = ContainerRef::from(shared);
        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_ENDERCHEST.msg()),
            move |context| chest(inventory, context.container_id, container, MENU_ROWS),
        );

        // TODO: vanilla angers nearby piglins and animates the lid; Steel has
        // neither piglins nor a container opener count yet.
        InteractionResult::Success
    }

    /// An ender chest has no block entity worth keeping.
    ///
    /// Vanilla has one only for the lid animation and the portal particles,
    /// both of which are client-side.
    fn new_block_entity(
        &self,
        _level: Weak<World>,
        _pos: BlockPos,
        _state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::NoEntity
    }
}
