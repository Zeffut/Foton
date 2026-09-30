//! The moment an eaten or drunk item is about to take effect.

use foton_registry::item_stack::ItemStack;
use foton_utils::types::InteractionHand;

use crate::entity::LivingEntity as _;
use crate::event::{Event as _, PlayerItemConsumeEvent};
use crate::player::Player;

impl Player {
    /// Asks plugins about the item this player is about to finish using.
    ///
    /// Paper parity: the `PlayerItemConsumeEvent` in `completeUsingItem`.
    /// Returns what to consume and, when a listener chose one, what the hand
    /// holds afterwards. `None` means a listener cancelled: the item is back
    /// in the hand, the use is stopped, and the client is resynchronized.
    ///
    /// The caller has taken `item` out of the hand; it goes back for the
    /// length of the event, because a listener reading the hand must find it
    /// there.
    pub(crate) fn fire_item_consume(
        &self,
        hand: InteractionHand,
        item: ItemStack,
    ) -> Option<(ItemStack, Option<ItemStack>)> {
        self.set_item_in_hand(hand, item.clone());
        let mut event = PlayerItemConsumeEvent::new(self.gameprofile.id, hand, item.clone());
        self.fire_event(&mut event);
        let held = self.take_item_in_hand(hand);
        if event.is_cancelled() {
            self.set_item_in_hand(hand, held);
            self.stop_using_item();
            self.send_inventory_to_remote();
            return None;
        }
        // Paper consumes the listener's stack only when it differs from the
        // one offered; otherwise the live stack, which a listener may have
        // touched through the inventory, is what gets eaten.
        let consumed = if event.item() == &item {
            held
        } else {
            event.item().clone()
        };
        Some((consumed, event.replacement().cloned()))
    }
}
