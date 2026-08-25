//! Merchant trades: what a villager or a wandering trader is willing to swap.
//!
//! Vanilla parity: `net.minecraft.world.item.trading`. The half that lives here
//! is the half the protocol and the trading menu both need -- a price, an offer,
//! and a merchant's list of them. The half that builds those offers out of the
//! `villager_trade` and `trade_set` data registries is a separate concern and
//! does not belong in the wire types.

mod item_cost;
mod merchant_offer;

#[cfg(test)]
mod tests;

pub use item_cost::ItemCost;
pub use merchant_offer::{MerchantOffer, MerchantOffers};
