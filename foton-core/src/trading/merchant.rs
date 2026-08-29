//! The seam between a mob that trades and the screen a player trades through.
//!
//! Vanilla parity: `net.minecraft.world.item.trading.Merchant` and the parts of
//! `AbstractVillager` that implement it. The offer types themselves live in
//! `foton_registry::trading`, because the protocol needs them too.

use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::trading::MerchantOffers;
use foton_utils::locks::SyncMutex;
use text_components::TextComponent;
use uuid::Uuid;

use crate::inventory::menu::kinds::merchant_menu;
use crate::player::Player;

/// Anything a player can open a trading screen against.
///
/// Two shape differences from vanilla, both forced by `Arc<dyn Merchant>` not
/// being able to hand out a `&mut`:
///
/// * `getOffers` returns the live list; [`Self::offers`] returns the lock
///   holding it, and callers that change a trade take the lock.
/// * `notifyTrade(MerchantOffer)` is handed the offer the player bought;
///   [`Self::notify_trade`] is handed its index, and the implementation looks it
///   up. It is still the merchant's own offer that gets its use count raised.
pub trait Merchant: Send + Sync {
    /// The merchant's live offer list.
    ///
    /// Vanilla parity: `getOffers`.
    fn offers(&self) -> &SyncMutex<MerchantOffers>;

    /// Replaces every offer.
    ///
    /// Vanilla parity: `overrideOffers`.
    fn override_offers(&self, offers: MerchantOffers) {
        *self.offers().lock() = offers;
    }

    /// The player currently in this merchant's trading screen.
    ///
    /// Vanilla parity: `getTradingPlayer`. Foton keeps the UUID rather than the
    /// `Player` so a merchant never holds a connection alive.
    fn trading_player(&self) -> Option<Uuid>;

    /// Vanilla parity: `setTradingPlayer`.
    fn set_trading_player(&self, player: Option<Uuid>);

    /// Records that the trade at `offer_index` was completed.
    ///
    /// Vanilla parity: `notifyTrade`, whose first act is `offer.increaseUses()`.
    /// An implementation must do the same, then bank its experience.
    fn notify_trade(&self, offer_index: usize);

    /// Announces that the result slot changed.
    ///
    /// Vanilla parity: `notifyTradeUpdated`, which a villager uses to grunt yes
    /// or no as the player builds up a payment.
    fn notify_trade_updated(&self, result: &ItemStack);

    /// Vanilla parity: `getVillagerXp`.
    fn villager_xp(&self) -> i32;

    /// Vanilla parity: `overrideXp`. `AbstractVillager` deliberately makes this
    /// a no-op -- the result slot calls it on every take, and a villager has
    /// already banked that experience in `rewardTradeXp` -- so only a merchant
    /// mirroring a remote one should store the value.
    fn override_xp(&self, _xp: i32) {}

    /// The badge level the trading screen draws, 1..=5.
    ///
    /// Vanilla parity: the `level` argument of `openTradingScreen`.
    fn merchant_level(&self) -> i32;

    /// Whether the screen draws the level badge and the experience bar.
    ///
    /// Vanilla parity: `showProgressBar`. A wandering trader answers false.
    fn show_progress_bar(&self) -> bool;

    /// Vanilla parity: `getNotifyTradeSound`.
    fn notify_trade_sound(&self) -> SoundEventRef;

    /// Whether the screen may promise that a sold-out trade will come back.
    ///
    /// Vanilla parity: `canRestock`, false for everything but a villager.
    fn can_restock(&self) -> bool {
        false
    }

    /// Whether this merchant is still a valid trading partner for `player`.
    ///
    /// Vanilla parity: `stillValid`, which for a mob means alive, still trading
    /// with this player, and in range.
    fn still_valid(&self, player: &Player) -> bool;
}

/// Opens `merchant`'s trading screen for `player`.
///
/// Vanilla parity: `Merchant.openTradingScreen`. The offers packet is sent from
/// the menu's `on_open`, which runs after the open-screen packet and before the
/// first content sync -- the same order vanilla sends them in, and the order the
/// client needs, since it drops merchant offers naming a container it has not
/// opened yet.
pub fn open_trading_screen(
    merchant: &Arc<dyn Merchant>,
    player: &Player,
    title: impl Into<TextComponent>,
) {
    let merchant = Arc::clone(merchant);
    player.open_menu(title, move |context| {
        merchant_menu(
            context.player.inventory.clone(),
            context.container_id,
            merchant,
        )
    });
}
