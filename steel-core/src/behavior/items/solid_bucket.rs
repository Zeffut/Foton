//! The powder snow bucket.

use steel_macros::item_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_items;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};

use super::BlockItem;

/// A bucket that places a block instead of a fluid.
///
/// Vanilla parity: `SolidBucketItem`, a `BlockItem` with its own place sound
/// that hands back an empty bucket.
///
/// Steel gap: Vanilla also implements `DispensibleContainerItem.emptyContents`
/// so a dispenser can place powder snow. Steel has no `DispenseItemBehavior`
/// registry yet, so only the hand-held path exists.
#[item_behavior]
pub struct SolidBucketItem {
    #[json_arg(vanilla_blocks, json = "block")]
    _block: BlockRef,
    #[json_arg(sound_events, json = "place_sound")]
    _place_sound: SoundEventRef,
    base: BlockItem,
}

impl SolidBucketItem {
    /// Creates a solid bucket behavior.
    #[must_use]
    pub const fn new(block: BlockRef, place_sound: SoundEventRef) -> Self {
        Self {
            _block: block,
            _place_sound: place_sound,
            base: BlockItem::new(block).with_place_sound(place_sound),
        }
    }
}

impl ItemBehavior for SolidBucketItem {
    /// Vanilla parity: `SolidBucketItem.useOn`.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let result = self.base.place(context.build_place_context());
        if !result.consumes_action() {
            return result;
        }

        // Vanilla parity: `BucketItem.getEmptySuccessItem` -- an empty bucket,
        // or the untouched stack in creative.
        let empty = if context.player.has_infinite_materials() {
            context
                .inv
                .with_item(|item| item.copy_with_count(item.count()))
        } else {
            ItemStack::new(&vanilla_items::BUCKET)
        };
        context.inv.with_item(|item| *item = empty);

        result
    }
}
