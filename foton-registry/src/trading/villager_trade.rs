//! One entry of the `villager_trade` data registry, and the price it resolves to.
//!
//! Vanilla parity: `net.minecraft.world.item.trading.VillagerTrade` and its
//! `TradeCost`. Since 26.2 a trade is data, not code: everything a villager is
//! willing to swap lives in `data/minecraft/villager_trade/**`, and building an
//! offer means running that entry's loot functions over the item it gives.

use rand::Rng;

use crate::REGISTRY;
use crate::data_component_predicate::DataComponentExactPredicate;
use crate::data_components::vanilla_components::{ADDITIONAL_TRADE_COST, MAX_STACK_SIZE};
use crate::item_stack::ItemStack;
use crate::items::LazyItemRef;
use crate::loot_table::{
    ConditionalLootFunction, EnchantmentOptions, LootCondition, LootContext, NumberProvider,
};
use crate::registry::{RegistryExt as _, TaggedRegistryExt as _};
use crate::trading::{ItemCost, MerchantOffer};
use foton_utils::Identifier;

/// The `components` an [`ItemCost`] demands of the stack that pays it.
///
/// Vanilla parity: the `DataComponentExactPredicate` of `TradeCost`. Foton has
/// no JSON-to-component-value path, so the build script lowers the one shape the
/// vanilla data contains and fails on anything else. Dropping the requirement
/// instead would let a wandering trader take any potion for its emerald.
#[derive(Debug, Clone)]
pub enum TradeCostComponents {
    /// `minecraft:potion_contents` naming exactly this potion.
    Potion(Identifier),
}

impl TradeCostComponents {
    /// Resolves this into the predicate an [`ItemCost`] carries.
    ///
    /// Returns [`DataComponentExactPredicate::EMPTY`] when the value does not
    /// round-trip -- the same refusal `DataComponentExactPredicate::new` makes
    /// -- rather than producing a cost nothing can satisfy.
    fn resolve(&self) -> DataComponentExactPredicate {
        match self {
            Self::Potion(id) => {
                let (Some(entry), Some(potion)) = (
                    REGISTRY
                        .data_components
                        .by_key(&crate::data_components::vanilla_components::POTION_CONTENTS.key),
                    REGISTRY.potions.by_key(id),
                ) else {
                    return DataComponentExactPredicate::EMPTY;
                };
                let contents = crate::data_components::components::PotionContents::empty()
                    .with_potion(crate::registry::reference::RegistryReference::new(potion));
                DataComponentExactPredicate::new(vec![(
                    entry,
                    crate::data_components::ComponentData::new(contents),
                )])
                .unwrap_or(DataComponentExactPredicate::EMPTY)
            }
        }
    }
}

/// One side of a trade's price, before demand and reputation touch it.
///
/// Vanilla parity: `net.minecraft.world.item.trading.TradeCost`.
#[derive(Debug)]
pub struct TradeCost {
    /// Vanilla parity: the `Holder<Item>` of `TradeCost`.
    pub item: LazyItemRef,
    pub count: NumberProvider,
    pub components: Option<TradeCostComponents>,
}

impl TradeCost {
    /// Resolves this cost against a roll, adding `additional_cost` to the count.
    ///
    /// Vanilla parity: `TradeCost.toItemCost`. The clamp floor of zero is what
    /// lets `VillagerTrade.getOffer` reject a trade whose price came out empty;
    /// the ceiling is the item's *default* stack size, not the built stack's.
    pub fn to_item_cost<R: Rng>(
        &self,
        ctx: &mut LootContext<'_, R>,
        additional_cost: i32,
    ) -> ItemCost {
        let max_stack_size = self.item.components.get(MAX_STACK_SIZE).unwrap_or(1);
        let count = (self.count.get_int(ctx.rng) + additional_cost).clamp(0, max_stack_size);
        match &self.components {
            None => ItemCost::new(self.item, count),
            Some(components) => ItemCost::with_components(self.item, count, components.resolve()),
        }
    }
}

/// One trade a profession can offer at one level.
///
/// Vanilla parity: `net.minecraft.world.item.trading.VillagerTrade`.
#[derive(Debug)]
pub struct VillagerTrade {
    pub key: Identifier,
    pub wants: TradeCost,
    pub additional_wants: Option<TradeCost>,
    /// Vanilla parity: the item of the `ItemStackTemplate` a trade `gives`.
    ///
    /// Foton's [`crate::item_stack_template::ItemStackTemplate`] cannot be named
    /// from a `static` -- it holds an `ItemRef`, and generated items are lazy --
    /// and no vanilla trade gives an item carrying components up front, since
    /// every one of them builds its result through `given_item_modifiers`. So
    /// the template collapses to an item and a count, and the build script fails
    /// on a `components` field rather than dropping it.
    pub gives: LazyItemRef,
    pub gives_count: i32,
    pub max_uses: NumberProvider,
    pub reputation_discount: NumberProvider,
    pub xp: NumberProvider,
    pub merchant_predicate: Option<LootCondition>,
    pub given_item_modifiers: &'static [ConditionalLootFunction],
    pub double_trade_price_enchantments: Option<EnchantmentOptions>,
}

pub type VillagerTradeRef = &'static VillagerTrade;

impl VillagerTrade {
    /// Builds the offer this trade makes, or `None` when it makes none.
    ///
    /// Vanilla parity: `VillagerTrade.getOffer`. There are four ways to get
    /// `None`, and all four matter: the merchant fails the predicate (a plains
    /// cartographer has no jungle map to sell), a modifier emptied the result
    /// (`filtered`'s `on_fail: discard`, which is how an enchantment that did
    /// not take withdraws the trade), or either price clamped to zero.
    pub fn get_offer<R: Rng>(&self, ctx: &mut LootContext<'_, R>) -> Option<MerchantOffer> {
        if let Some(predicate) = &self.merchant_predicate
            && !predicate.test(ctx)
        {
            return None;
        }

        let mut result = ItemStack::with_count(self.gives, self.gives_count);
        let mut additional_cost = 0;

        for modifier in self.given_item_modifiers {
            if modifier
                .conditions
                .iter()
                .all(|condition| condition.test(ctx))
            {
                modifier.function.apply(&mut result, ctx);
            }
            if result.is_empty() {
                return None;
            }
        }

        if let Some(banked) = result.get(ADDITIONAL_TRADE_COST).copied() {
            result.remove(ADDITIONAL_TRADE_COST);
            additional_cost += banked;
        }

        if let Some(enchantments) = &self.double_trade_price_enchantments
            && result_stores_any_of(&result, enchantments)
        {
            additional_cost *= 2;
        }

        let item_cost = self.wants.to_item_cost(ctx, additional_cost);
        if item_cost.count() < 1 {
            return None;
        }

        let additional_item_cost = self
            .additional_wants
            .as_ref()
            .map(|cost| cost.to_item_cost(ctx, 0));
        if additional_item_cost
            .as_ref()
            .is_some_and(|cost| cost.count() < 1)
        {
            return None;
        }

        Some(MerchantOffer::new(
            item_cost,
            additional_item_cost,
            result,
            self.max_uses.get_int(ctx.rng).max(1),
            self.xp.get_int(ctx.rng).max(0),
            self.reputation_discount.get(ctx.rng, None).max(0.0),
        ))
    }
}

/// Whether `result`'s stored enchantments include any of `options`.
///
/// Vanilla parity: the `STORED_ENCHANTMENTS` half of `VillagerTrade.getOffer`,
/// which is why the doubled price only ever lands on an enchanted book.
fn result_stores_any_of(result: &ItemStack, options: &EnchantmentOptions) -> bool {
    let Some(stored) = result.get(crate::data_components::vanilla_components::STORED_ENCHANTMENTS)
    else {
        return false;
    };

    match options {
        EnchantmentOptions::Tag(tag) => REGISTRY
            .enchantments
            .iter_tag(tag)
            .any(|enchantment| stored.get_level(&enchantment.key) > 0),
        EnchantmentOptions::List(ids) => ids.iter().any(|id| stored.get_level(id) > 0),
    }
}
