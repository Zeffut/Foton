//! A pool of trades a merchant draws its offers from.
//!
//! Vanilla parity: `net.minecraft.world.item.trading.TradeSet` and the two
//! draw loops of `AbstractVillager`. Vanilla holds the pool as a `HolderSet`
//! resolved from a `#villager_trade` tag; the build script flattens those tags,
//! so the pool is already a plain slice by the time it reaches here.

use rand::{Rng, RngExt as _};

use crate::REGISTRY;
use crate::loot_table::{LootContext, NumberProvider};
use crate::registry::RegistryExt as _;
use crate::trading::{MerchantOffers, VillagerTradeRef};
use rustc_hash::FxHashMap;
use steel_utils::Identifier;

/// The trades one merchant level draws from, and how many it draws.
///
/// Vanilla parity: `net.minecraft.world.item.trading.TradeSet`.
#[derive(Debug)]
pub struct TradeSet {
    pub key: Identifier,
    pub trades: &'static [VillagerTradeRef],
    pub amount: NumberProvider,
    pub allow_duplicates: bool,
    pub random_sequence: Option<Identifier>,
}

pub type TradeSetRef = &'static TradeSet;

impl TradeSet {
    /// Vanilla parity: `TradeSet.calculateNumberOfTrades`.
    pub fn calculate_number_of_trades<R: Rng>(&self, ctx: &mut LootContext<'_, R>) -> i32 {
        self.amount.get_int(ctx.rng)
    }

    /// Appends the offers this set produces to `offers`.
    ///
    /// Vanilla parity: the two static draw loops of `AbstractVillager`,
    /// `addOffersFromItemListings` and `addOffersFromItemListingsWithoutDuplicates`.
    /// They differ in one thing: without duplicates a candidate is removed the
    /// moment it is drawn, so a trade that declines still costs a slot's worth
    /// of pool; with duplicates it is removed only when it declines.
    pub fn add_offers<R: Rng>(&self, ctx: &mut LootContext<'_, R>, offers: &mut MerchantOffers) {
        let number_of_offers = self.calculate_number_of_trades(ctx);
        let mut candidates: Vec<VillagerTradeRef> = self.trades.to_vec();
        let mut found = 0;

        while found < number_of_offers && !candidates.is_empty() {
            let roll = ctx.rng.random_range(0..candidates.len());
            if self.allow_duplicates {
                let trade = candidates[roll];
                match trade.get_offer(ctx) {
                    None => {
                        candidates.remove(roll);
                    }
                    Some(offer) => {
                        offers.push(offer);
                        found += 1;
                    }
                }
            } else {
                let trade = candidates.remove(roll);
                if let Some(offer) = trade.get_offer(ctx) {
                    offers.push(offer);
                    found += 1;
                }
            }
        }
    }
}

impl TradeSet {
    /// The trade set a profession offers at `level`, if it offers one.
    ///
    /// Vanilla parity: `VillagerProfession.getTrades(int)`, whose
    /// `Int2ObjectMap` is built in `VillagerProfession.bootstrap` by pairing
    /// each profession with `TradeSets.<PROFESSION>_LEVEL_<n>`. Those keys are
    /// exactly `<profession path>/level_<n>`, which is also the path the data
    /// registry files sit at, so the pairing is read off the data rather than
    /// transcribed. A profession with no directory -- `none` and `nitwit` --
    /// answers `None` at every level, matching their empty vanilla maps.
    #[must_use]
    pub fn for_profession(profession: &Identifier, level: i32) -> Option<TradeSetRef> {
        let key = Identifier::new(
            profession.namespace.to_string(),
            format!("{}/level_{level}", profession.path),
        );
        REGISTRY.trade_sets.by_key(&key)
    }
}

/// Registry for the `villager_trade` data registry.
pub struct VillagerTradeRegistry {
    villager_trades_by_id: Vec<VillagerTradeRef>,
    villager_trades_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl VillagerTradeRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            villager_trades_by_id: Vec::new(),
            villager_trades_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    VillagerTradeRegistry,
    VillagerTradeRef,
    villager_trades_by_id,
    villager_trades_by_key,
    allows_registering
);

crate::impl_registry!(
    VillagerTradeRegistry,
    crate::trading::VillagerTrade,
    villager_trades_by_id,
    villager_trades_by_key,
    villager_trades
);

/// Registry for the `trade_set` data registry.
pub struct TradeSetRegistry {
    trade_sets_by_id: Vec<TradeSetRef>,
    trade_sets_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl TradeSetRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            trade_sets_by_id: Vec::new(),
            trade_sets_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    TradeSetRegistry,
    TradeSetRef,
    trade_sets_by_id,
    trade_sets_by_key,
    allows_registering
);

crate::impl_registry!(
    TradeSetRegistry,
    TradeSet,
    trade_sets_by_id,
    trade_sets_by_key,
    trade_sets
);
