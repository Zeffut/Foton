//! Book and quill, and the signed book it becomes.
//!
//! Vanilla parity: `WritableBookItem` and `WrittenBookItem`. Both classes have
//! the same body: open the item GUI and count the use. Steel has no statistics
//! system, so only the first half is ported.

use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};

/// Book and quill.
#[steel_macros::item_behavior]
pub struct WritableBookItem;

impl ItemBehavior for WritableBookItem {
    /// Vanilla parity: `WritableBookItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        open_item_gui(context);
        InteractionResult::Success
    }
}

/// A signed book.
#[steel_macros::item_behavior]
pub struct WrittenBookItem;

impl ItemBehavior for WrittenBookItem {
    /// Vanilla parity: `WrittenBookItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        open_item_gui(context);
        InteractionResult::Success
    }
}

fn open_item_gui(context: &UseItemContext<'_>) {
    let stack = context
        .inv
        .with_item(|item| item.copy_with_count(item.count()));
    context.player.open_item_gui(&stack, context.hand);
}
