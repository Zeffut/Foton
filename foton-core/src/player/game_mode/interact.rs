//! `PlayerInteractEvent`, raised where `CraftBukkit` raises it.

use super::{BlockPos, ClipBlockShape, ClipFluid, Entity as _, InteractionHand, ItemStack, Player};
use crate::event::{InteractAction, InteractTarget, PlayerInteractEvent, UseResult};

/// The block interaction a use-item packet may repeat.
///
/// One right click on a block can reach the server twice: as use-item-on,
/// and -- when the block did not take the action -- as use-item. Bukkit
/// raises the event once for the click and lets the second packet reuse its
/// decision. Paper parity: `ServerPlayerGameMode.firedInteract`.
pub struct InteractMemo {
    pos: BlockPos,
    hand: InteractionHand,
    item: ItemStack,
    use_item: UseResult,
}

impl Player {
    /// Raises the event for this hand and what it holds.
    pub(super) fn fire_interact(
        &self,
        action: InteractAction,
        hand: InteractionHand,
        target: Option<InteractTarget>,
    ) -> PlayerInteractEvent {
        let item = self.inventory.lock().get_item_in_hand(hand).clone();
        let mut event = PlayerInteractEvent::new(
            self.gameprofile.id,
            self.get_world().key.to_string(),
            action,
            (hand, item),
            target,
        );
        self.fire_event(&mut event);
        event
    }

    /// Remembers a right click on a block for the use-item packet that may
    /// follow it.
    pub(super) fn remember_interact(&self, event: &PlayerInteractEvent) {
        let Some(target) = event.target() else {
            return;
        };
        *self.interact_memo.lock() = Some(InteractMemo {
            pos: target.pos,
            hand: event.hand(),
            item: event.item().clone(),
            use_item: event.use_item(),
        });
    }

    /// Whether a use-item packet may go ahead, raising the event for it.
    ///
    /// `CraftBukkit` traces the player's reach: with no block in it the use is
    /// `RIGHT_CLICK_AIR`; with one, it is the click on that block, answered
    /// from the remembered decision when this packet is that click's second
    /// half and raised afresh otherwise.
    pub(super) fn allow_use_item(&self, hand: InteractionHand) -> bool {
        let memo = self.interact_memo.lock().take();
        let Some(hit) = self.block_in_reach() else {
            let event = self.fire_interact(InteractAction::RightClickAir, hand, None);
            return event.use_item() != UseResult::Deny;
        };
        let held = self.inventory.lock().get_item_in_hand(hand).clone();
        if let Some(memo) = memo
            && memo.pos == hit.pos
            && memo.hand == hand
            && ItemStack::is_same_item_same_components(&memo.item, &held)
        {
            return memo.use_item != UseResult::Deny;
        }
        let event = self.fire_interact(InteractAction::RightClickBlock, hand, Some(hit));
        event.use_item() != UseResult::Deny
    }

    /// The block the player's crosshair rests on within their reach.
    pub(super) fn block_in_reach(&self) -> Option<InteractTarget> {
        let eye = self.eye_position();
        let end = eye + self.look_angle() * self.block_interaction_range();
        let hit = self
            .get_world()
            .clip(eye, end, ClipBlockShape::Outline, ClipFluid::None);
        if hit.is_miss() {
            return None;
        }
        Some(InteractTarget {
            pos: hit.block_pos,
            face: hit.direction,
            location: Some(hit.location),
        })
    }
}
