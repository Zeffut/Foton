//! The villager trading screen.
//!
//! Vanilla parity: `MerchantMenu`. Three slots and a list of trades: two the
//! player pays into, one the trade comes out of, and a selection the player
//! makes by clicking a trade rather than by filling a slot.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use foton_protocol::packets::game::CMerchantOffers;
use foton_registry::vanilla_menu_types;

use crate::entity::Entity as _;
use crate::inventory::container::{ResultContainer, SimpleContainer};
use crate::inventory::prelude::*;
use crate::inventory::slots::{MerchantHandler, NO_TRADE_SELECTED};
use crate::player::player_inventory::PlayerInventory;
use crate::trading::Merchant;

/// Builds the trading menu for `merchant`.
#[must_use]
pub fn merchant_menu(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    merchant: Arc<dyn Merchant>,
) -> Menu {
    // Vanilla's MerchantContainer is one three-slot container. The payment half
    // is the only half a click may write into, so it is the only half that is a
    // plain container here.
    let payment_container = SimpleContainer::new(2).into_shared();
    let result_container = ResultContainer::new().into_shared();
    let selection_hint = Arc::new(AtomicI32::new(NO_TRADE_SELECTED));

    let handler = MerchantHandler::new(
        payment_container.clone(),
        result_container.clone(),
        Arc::clone(&merchant),
        Arc::clone(&selection_hint),
    );

    let mut builder = MenuBuilder::new(&vanilla_menu_types::MERCHANT, container_id);
    let payment = builder.section_all(&payment_container);
    let result = builder.result_slot(handler.clone());
    let player = builder.player_inventory(&inventory);

    // Vanilla parity: `MerchantMenu.quickMoveStack`. Taking the result fills the
    // inventory back to front, paying fills it front to back.
    builder.route(result, player.all(), FillDirection::Backward);
    builder.route(payment, player.all(), FillDirection::Forward);
    builder.route(player.all(), payment, FillDirection::Forward);
    // Vanilla parity: the half of `MerchantMenu.removed` that hands the payment
    // back to the player rather than eating it.
    builder.drain(payment);

    builder.build(MerchantKind {
        handler,
        merchant,
        selection_hint,
    })
}

/// Per-menu trading state.
pub struct MerchantKind {
    handler: MerchantHandler,
    merchant: Arc<dyn Merchant>,
    /// The trade the player clicked, shared with the handler.
    selection_hint: Arc<AtomicI32>,
}

impl MerchantKind {
    /// The container holding the two payment slots.
    #[cfg(test)]
    pub(crate) fn payment_id_for_tests(&self) -> ContainerId {
        self.handler.payment_id()
    }

    /// The container holding what the trade produces.
    #[cfg(test)]
    pub(crate) fn result_id_for_tests(&self) -> ContainerId {
        self.handler.result_id()
    }
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for MerchantKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/merchant");
}

impl MenuKind for MerchantKind {
    /// Vanilla parity: `Merchant.openTradingScreen`, which sends the offers
    /// right after the menu packet. An empty list is not sent at all, which is
    /// what leaves an unemployed villager's screen blank rather than
    /// desynchronized.
    fn on_open(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        self.merchant.set_trading_player(Some(player.uuid()));

        let offers = self.merchant.offers().lock().clone();
        if !offers.is_empty() {
            player.send_packet(CMerchantOffers {
                container_id: i32::from(behavior.container_id()),
                offers,
                villager_level: self.merchant.merchant_level(),
                villager_xp: self.merchant.villager_xp(),
                show_progress: self.merchant.show_progress_bar(),
                can_restock: self.merchant.can_restock(),
            });
        }

        self.handler.update_result(guard);
    }

    /// Vanilla parity: `MerchantMenu.slotsChanged`, which rebuilds the result
    /// from whatever now sits in the payment slots.
    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.handler.update_result(guard);
    }

    fn on_select_trade(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
        selected_trade: i32,
    ) {
        self.selection_hint.store(selected_trade, Ordering::Relaxed);
        self.handler.update_result(guard);
    }

    /// Vanilla parity: `MerchantMenu.canTakeItemForPickAll`, which answers false
    /// for every slot -- double-clicking in a trading screen gathers nothing.
    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, _slot_index: usize) -> bool {
        false
    }

    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.merchant.still_valid(player)
    }

    /// Vanilla parity: the rest of `MerchantMenu.removed`. Handing the payment
    /// back is the builder's `drain`; what is left is releasing the merchant so
    /// the next customer can open the screen.
    fn removed(&mut self, _behavior: &mut MenuBehavior, _player: &Player) {
        self.merchant.set_trading_player(None);
    }
}

#[cfg(test)]
mod tests;
