//! Clientbound packet carrying a merchant's trades to the open trading screen.

use steel_macros::{ClientPacket, WriteTo};
use steel_registry::{packets::play::C_MERCHANT_OFFERS, trading::MerchantOffers};

/// Sent right after a trading menu opens, and again whenever the offers change.
///
/// Vanilla parity: `ClientboundMerchantOffersPacket`. The client will not draw a
/// trading screen at all until this arrives, which is why vanilla sends it from
/// `Merchant.openTradingScreen` immediately after the menu packet.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_MERCHANT_OFFERS)]
pub struct CMerchantOffers {
    #[write(as = VarInt)]
    pub container_id: i32,
    pub offers: MerchantOffers,
    /// The merchant's level, 1..=5, which picks the badge the screen draws.
    #[write(as = VarInt)]
    pub villager_level: i32,
    /// Experience toward the next level.
    #[write(as = VarInt)]
    pub villager_xp: i32,
    /// Whether to draw the level badge and the experience bar at all.
    ///
    /// Vanilla parity: `showProgress`, which a wandering trader sends as false.
    pub show_progress: bool,
    /// Whether the screen may tell the player that a sold-out trade will return.
    pub can_restock: bool,
}
