//! Grindstone behavior.
//!
//! Vanilla parity: `GrindstoneBlock`. It faces whichever way it was placed and
//! opens a menu; everything a grindstone actually does lives in that menu.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    AttachFace, BlockStateProperties, Direction, EnumProperty,
};
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::inventory::menu::kinds::grindstone;
use crate::player::Player;
use crate::world::World;

/// Whether the grindstone is on the floor, a wall or a ceiling.
const FACE: &EnumProperty<AttachFace> = &BlockStateProperties::ATTACH_FACE;

/// Which way it points.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Behavior for the grindstone.
#[block_behavior]
pub struct GrindstoneBlock {
    block: BlockRef,
}

impl GrindstoneBlock {
    /// Creates a grindstone behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for GrindstoneBlock {
    /// Vanilla parity: `FaceAttachedHorizontalDirectionalBlock.getStateForPlacement`,
    /// which is why a grindstone can be stuck to a wall or hung from a ceiling
    /// rather than only standing on the floor.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let clicked_face = context.clicked_face();
        let (face, facing) = match clicked_face {
            Direction::Up => (AttachFace::Floor, context.horizontal_direction().opposite()),
            Direction::Down => (
                AttachFace::Ceiling,
                context.horizontal_direction().opposite(),
            ),
            horizontal => (AttachFace::Wall, horizontal),
        };

        Some(
            self.block
                .default_state()
                .set_value(FACE, face)
                .set_value(FACING, facing),
        )
    }

    /// Vanilla parity: `GrindstoneBlock.useWithoutItem`.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let inventory = player.inventory.clone();
        let world = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_GRINDSTONE_TITLE.msg()),
            move |context| grindstone(inventory, context.container_id, pos, &world),
        );
        InteractionResult::Success
    }
}
