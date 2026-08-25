//! The trading screen's two payment slots and its result slot.
//!
//! Vanilla parity: `MerchantContainer` plus `MerchantResultSlot`. Vanilla makes
//! one three-slot container and gives slot 2 special behavior; Steel splits it
//! the way every other result-bearing menu here is split -- a plain container
//! for the payment and a [`ResultContainer`] for what comes out -- so nothing
//! can write into the result by clicking it.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use steel_registry::item_stack::ItemStack;
use steel_registry::trading::MerchantOffer;
use steel_utils::locks::Shared;

use crate::inventory::container::{Container as _, ResultContainer, SimpleContainer};
use crate::inventory::lock::{ContainerId, ContainerLockGuard, ContainerRef};
use crate::inventory::slots::ResultHandler;
use crate::player::Player;
use crate::trading::Merchant;

/// The selection hint a freshly opened trading screen carries.
///
/// Vanilla parity: the `0` a new `MerchantContainer.selectionHint` holds. It is
/// not a sentinel: `getRecipeFor` only honors a hint above zero, so trade 0 is
/// found by the scan rather than by the hint.
pub const NO_TRADE_SELECTED: i32 = 0;

/// Keeps the trading screen's result in step with what the player has paid.
#[derive(Clone)]
pub struct MerchantHandler {
    payment_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    merchant: Arc<dyn Merchant>,
    /// The trade the player clicked, shared with the menu because the click and
    /// the take arrive as separate packets.
    selection_hint: Arc<AtomicI32>,
}

impl MerchantHandler {
    /// Creates a handler over the trading screen's two containers.
    #[must_use]
    pub const fn new(
        payment_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
        merchant: Arc<dyn Merchant>,
        selection_hint: Arc<AtomicI32>,
    ) -> Self {
        Self {
            payment_container,
            result_container,
            merchant,
            selection_hint,
        }
    }

    /// The merchant this screen trades with.
    #[must_use]
    pub const fn merchant(&self) -> &Arc<dyn Merchant> {
        &self.merchant
    }

    /// The payment container's id.
    #[must_use]
    pub fn payment_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.payment_container)
    }

    /// The result container's id.
    #[must_use]
    pub fn result_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.result_container)
    }

    /// The two stacks the player has put up, in the order the trade reads them.
    ///
    /// Vanilla parity: the opening of `MerchantContainer.updateSellItem`, which
    /// slides a lone second payment into first position so that paying into
    /// either slot works.
    fn payment(&self, guard: &ContainerLockGuard) -> (ItemStack, ItemStack) {
        let Some(container) = guard.get(self.payment_id()) else {
            return (ItemStack::empty(), ItemStack::empty());
        };
        let first = container.get_item(0).clone();
        let second = container.get_item(1).clone();
        if first.is_empty() {
            (second, ItemStack::empty())
        } else {
            (first, second)
        }
    }

    /// Finds the trade the current payment buys, as `(index, snapshot)`.
    ///
    /// Vanilla parity: the body of `MerchantContainer.updateSellItem`. It tries
    /// the payment both ways round, so a two-cost trade can be paid into either
    /// slot, and it refuses a trade that is out of stock.
    ///
    /// The index is what makes this usable from Rust: vanilla keeps a reference
    /// into the merchant's live list, which an `Arc<dyn Merchant>` cannot hand
    /// out, so the caller re-locks and indexes when it needs to change the trade.
    fn active_trade(&self, guard: &ContainerLockGuard) -> Option<(usize, MerchantOffer)> {
        let (buy_a, buy_b) = self.payment(guard);
        if buy_a.is_empty() {
            return None;
        }

        let hint = self.selection_hint.load(Ordering::Relaxed);
        let offers = self.merchant.offers().lock();

        let index = offers
            .recipe_index_for(&buy_a, &buy_b, hint)
            .or_else(|| offers.recipe_index_for(&buy_b, &buy_a, hint))
            .filter(|&index| !offers[index].is_out_of_stock())?;

        Some((index, offers[index].clone()))
    }

    /// The experience the merchant would gain from the trade now set up.
    ///
    /// Vanilla parity: `MerchantContainer.getFutureXp`.
    #[must_use]
    pub fn future_xp(&self, guard: &ContainerLockGuard) -> i32 {
        self.active_trade(guard).map_or(0, |(_, offer)| offer.xp())
    }
}

impl ResultHandler for MerchantHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.payment_container.clone())]
    }

    fn update_result(&self, guard: &mut ContainerLockGuard) {
        let result = self
            .active_trade(guard)
            .map_or_else(ItemStack::empty, |(_, offer)| offer.assemble());

        let result_id = self.result_id();
        if let Some(container) = guard.get_typed_mut::<ResultContainer>(result_id) {
            container.set_item(0, result.clone());
            container.set_changed();
        }

        // Vanilla parity: `updateSellItem` reaches `notifyTradeUpdated` only
        // down the branch where a payment exists. Emptying the slots clears the
        // result silently -- a villager grunts at what you offer it, not at you
        // taking your things back.
        let (buy_a, _) = self.payment(guard);
        if !buy_a.is_empty() {
            self.merchant.notify_trade_updated(&result);
        }
    }

    /// Vanilla parity: `MerchantResultSlot.onTake`.
    ///
    /// The order is vanilla's and it matters: the payment is spent through the
    /// offer itself, so demand and reputation decide how much is taken; the
    /// merchant is told only if that succeeded; and `overrideXp` runs either
    /// way, which on a real mob is the no-op vanilla makes it.
    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        let (index, offer) = self.active_trade(guard)?;

        let payment_id = self.payment_id();
        let (mut buy_a, mut buy_b) = {
            let container = guard.get(payment_id)?;
            (container.get_item(0).clone(), container.get_item(1).clone())
        };

        if offer.take(&mut buy_a, &mut buy_b) || offer.take(&mut buy_b, &mut buy_a) {
            self.merchant.notify_trade(index);
            if let Some(container) = guard.get_mut(payment_id) {
                container.set_item(0, buy_a);
                container.set_item(1, buy_b);
                container.set_changed();
            }
        }

        self.merchant
            .override_xp(self.merchant.villager_xp() + offer.xp());

        self.update_result(guard);
        None
    }

    fn is_result_valid(&self, guard: &ContainerLockGuard, _player: &Player) -> bool {
        self.active_trade(guard).is_some()
    }
}
