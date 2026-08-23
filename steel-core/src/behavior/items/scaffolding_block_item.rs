//! The scaffolding item, which walks its own placement up or sideways.

use steel_macros::item_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::Direction;

use crate::behavior::blocks::ScaffoldingBlock;
use crate::behavior::{
    BlockPlaceContext, BlockStateBehaviorExt as _, InteractionResult, ItemBehavior, UseOnContext,
};
use crate::world::LevelReader as _;

use super::BlockItem;

/// Vanilla parity: the `7` bound of `ScaffoldingBlockItem.updatePlacementContext`.
const MAX_SCAFFOLDING_REACH: i32 = 7;

/// The scaffolding item.
///
/// Vanilla parity: `ScaffoldingBlockItem`, whose `updatePlacementContext` walks
/// along the placement direction until it finds a replaceable block, and whose
/// `mustSurvive` is false so a tower may extend into the air.
///
/// Steel gap: Vanilla warns a player who runs into the build limit with
/// `ServerPlayer.sendBuildLimitMessage`; Steel has no such message and simply
/// stops walking.
///
/// Steel gap: `ScaffoldingBlock` does not yet maintain its `distance` property,
/// so `ScaffoldingBlock::get_distance` reports the default `7` for anything
/// resting on other scaffolding. Placing upwards works, because that path
/// never consults the distance; placing sideways off an existing tower is
/// refused until the block keeps the property up to date.
#[item_behavior]
pub struct ScaffoldingBlockItem {
    #[json_arg(vanilla_blocks, json = "block")]
    block: BlockRef,
    base: BlockItem,
}

impl ScaffoldingBlockItem {
    /// Creates a scaffolding item behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            block,
            base: BlockItem::new(block).without_must_survive(),
        }
    }

    /// Vanilla parity: `ScaffoldingBlockItem.updatePlacementContext`.
    fn update_placement_context<'a>(
        &self,
        context: BlockPlaceContext<'a>,
    ) -> Option<BlockPlaceContext<'a>> {
        let pos = context.place_pos();
        let world = context.world;

        if world.get_block_state(pos).get_block() != self.block {
            let distance = ScaffoldingBlock::get_distance(world.as_ref(), pos);
            return (distance != MAX_SCAFFOLDING_REACH).then_some(context);
        }

        let direction = if context.is_secondary_use_active() {
            if context.is_inside() {
                context.clicked_face().opposite()
            } else {
                context.clicked_face()
            }
        } else if context.clicked_face() == Direction::Up {
            context.horizontal_direction()
        } else {
            Direction::Up
        };

        let mut horizontal_distance = 0;
        let mut placement_pos = direction.relative(pos);
        while horizontal_distance < MAX_SCAFFOLDING_REACH {
            if !world.is_in_valid_bounds(placement_pos) {
                break;
            }

            let replaced = world.get_block_state(placement_pos);
            if replaced.get_block() != self.block {
                if !replaced.can_be_replaced(&context) {
                    break;
                }
                return Some(context.at(placement_pos, direction));
            }

            placement_pos = direction.relative(placement_pos);
            if direction.is_horizontal() {
                horizontal_distance += 1;
            }
        }

        None
    }
}

impl ItemBehavior for ScaffoldingBlockItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        // Vanilla parity: `BlockItem.place` checks `canPlace` on the original
        // context, then lets the item redirect it.
        let place_context = context.build_place_context();
        if !place_context.can_place() {
            return InteractionResult::Fail;
        }
        let Some(place_context) = self.update_placement_context(place_context) else {
            return InteractionResult::Fail;
        };
        self.base.place(place_context)
    }
}
