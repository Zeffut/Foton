//! The swap-hands key.

use crate::entity::LivingEntity as _;
use crate::event::{Event as _, PlayerSwapHandItemsEvent};
use crate::player::Player;

impl Player {
    /// Exchanges the main hand with the offhand, after asking plugins.
    ///
    /// Paper parity: `SWAP_ITEM_WITH_OFFHAND` in `handlePlayerAction`. A
    /// listener may cancel the swap or name what either hand ends up holding;
    /// a hand it left alone gets the plain exchange.
    pub(crate) fn swap_hand_items(&self) {
        let (main, off) = {
            let inventory = self.inventory.lock();
            (
                inventory.get_selected_item().clone(),
                inventory.get_offhand_item().clone(),
            )
        };
        let mut event =
            PlayerSwapHandItemsEvent::new(self.gameprofile.id, off.clone(), main.clone());
        self.fire_event(&mut event);
        if event.is_cancelled() {
            return;
        }

        let changed = {
            let mut inventory = self.inventory.lock();
            if event.main_hand_item() == &off && event.off_hand_item() == &main {
                inventory.swap_hands()
            } else {
                // A hand the listener left alone takes the live stack of the
                // other one, which the listener may have touched.
                let live_main = inventory.get_selected_item().clone();
                let live_off = inventory.get_offhand_item().clone();
                let new_off = if event.off_hand_item() == &main {
                    live_main
                } else {
                    event.off_hand_item().clone()
                };
                let new_main = if event.main_hand_item() == &off {
                    live_off
                } else {
                    event.main_hand_item().clone()
                };
                inventory.set_offhand_item(new_off);
                inventory.set_selected_item(new_main);
                true
            }
        };
        self.stop_using_item();
        if changed {
            self.broadcast_inventory_changes();
        }
    }
}
