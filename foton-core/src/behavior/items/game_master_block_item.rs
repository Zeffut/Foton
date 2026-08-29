//! Command, structure, jigsaw and test block items.

use foton_macros::item_behavior;
use foton_registry::blocks::BlockRef;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};

use super::BlockItem;

/// A block item only a gamemaster may place.
///
/// Vanilla parity: `GameMasterBlockItem`, whose `getPlacementState` returns null
/// for anyone without `Player.canUseGameMasterBlocks`.
///
/// Foton deviation: the check runs just before placement rather than inside
/// `getPlacementState`, which Foton routes through the block behavior. Both
/// orderings refuse the same clicks with the same `FAIL`.
#[item_behavior]
pub struct GameMasterBlockItem {
    #[json_arg(vanilla_blocks, json = "block")]
    _block: BlockRef,
    base: BlockItem,
}

impl GameMasterBlockItem {
    /// Creates a gamemaster block item behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            _block: block,
            base: BlockItem::new(block),
        }
    }
}

impl ItemBehavior for GameMasterBlockItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let place_context = context.build_place_context();
        if !place_context.can_place() {
            return InteractionResult::Fail;
        }
        if !context.player.can_use_game_master_blocks() {
            return InteractionResult::Fail;
        }
        self.base.place(place_context)
    }
}
