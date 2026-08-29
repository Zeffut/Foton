//! Tests for the compiled `villager_trade` / `trade_set` registries.
//!
//! These run against the real generated data, so they are what catches a build
//! script that lowered a trade wrongly -- a price that came out zero, a tag that
//! flattened to the wrong pool, a `merchant_predicate` that stopped gating.

use rand::SeedableRng as _;
use rand::rngs::StdRng;

use crate::loot_table::{EntityRef, LootContext};
use crate::registry::RegistryExt as _;
use crate::trading::{ItemCost, MerchantOffers, TradeSet, VillagerTradeRef};
use crate::{REGISTRY, init_vanilla_registry, vanilla_items};
use foton_utils::Identifier;

fn trade(key: &str) -> VillagerTradeRef {
    init_vanilla_registry();
    REGISTRY
        .villager_trades
        .by_key(&Identifier::vanilla(key.to_string()))
        .unwrap_or_else(|| panic!("{key} should be a registered villager trade"))
}

/// A villager standing in a plains village, which is what most trades see.
fn plains_villager() -> Identifier {
    Identifier::vanilla_static("plains")
}

#[test]
fn every_vanilla_trade_set_is_reachable_from_its_profession_and_level() {
    init_vanilla_registry();

    // The professions that have a workstation are exactly the ones with trades.
    for profession in [
        "armorer",
        "butcher",
        "cartographer",
        "cleric",
        "farmer",
        "fisherman",
        "fletcher",
        "leatherworker",
        "librarian",
        "mason",
        "shepherd",
        "toolsmith",
        "weaponsmith",
    ] {
        let key = Identifier::vanilla(profession.to_string());
        for level in 1..=5 {
            let set = TradeSet::for_profession(&key, level)
                .unwrap_or_else(|| panic!("{profession} should have a trade set at level {level}"));
            assert!(
                !set.trades.is_empty(),
                "{profession} level {level} draws from an empty pool"
            );
        }
    }

    // `none` and `nitwit` register an empty map in `VillagerProfession.bootstrap`.
    for profession in ["none", "nitwit"] {
        let key = Identifier::vanilla(profession.to_string());
        for level in 1..=5 {
            assert!(
                TradeSet::for_profession(&key, level).is_none(),
                "{profession} should offer nothing at level {level}"
            );
        }
    }
}

#[test]
fn a_smith_level_pool_carries_the_shared_common_smith_trades() {
    init_vanilla_registry();
    // `#minecraft:armorer/level_1` nests `#minecraft:common_smith/level_1`, so a
    // build script that stopped resolving nested tags would drop the coal trade.
    let set = TradeSet::for_profession(&Identifier::vanilla_static("armorer"), 1)
        .expect("armorer level 1 should exist");
    assert!(
        set.trades
            .iter()
            .any(|trade| trade.key.path == "smith/1/coal_emerald"),
        "armorer level 1 should inherit the common smith coal trade"
    );
}

#[test]
fn a_plain_trade_prices_itself_from_the_data() {
    init_vanilla_registry();
    let mut rng = StdRng::seed_from_u64(1);
    let mut ctx = LootContext::new(&mut rng);

    let wheat = trade("farmer/1/wheat_emerald");
    let offer = wheat.get_offer(&mut ctx).expect("wheat always trades");

    assert_eq!(offer.base_cost_a().item(), &*vanilla_items::WHEAT);
    assert_eq!(offer.base_cost_a().count(), 20);
    assert_eq!(offer.result().item(), &*vanilla_items::EMERALD);
    assert_eq!(offer.result().count(), 1);
    assert_eq!(offer.max_uses(), 16);
    assert_eq!(offer.xp(), 2);
    assert!(
        (offer.price_multiplier() - 0.05).abs() < f32::EPSILON,
        "the farmer's reputation discount is 0.05"
    );
}

#[test]
fn a_second_price_becomes_the_offers_cost_b() {
    init_vanilla_registry();
    let mut rng = StdRng::seed_from_u64(2);
    let mut ctx = LootContext::new(&mut rng);

    let bread = trade("fletcher/1/gravel_and_emerald_flint");
    let offer = bread.get_offer(&mut ctx).expect("flint always trades");

    assert_eq!(
        offer.item_cost_b().map(ItemCost::item),
        Some(&*vanilla_items::EMERALD),
        "`additional_wants` is the second price the screen draws"
    );
}

#[test]
fn a_merchant_predicate_gates_a_trade_on_the_villagers_variant() {
    init_vanilla_registry();
    let mut rng = StdRng::seed_from_u64(3);

    // The oak boat is sold only by a plains fisherman.
    let oak_boat = trade("fisherman/5/oak_boat_emerald");

    let plains = plains_villager();
    let mut allowed = LootContext::new(&mut rng).with_this_entity(EntityRef {
        villager_variant: Some(&plains),
        ..EntityRef::default()
    });
    assert!(
        oak_boat.get_offer(&mut allowed).is_some(),
        "a plains fisherman sells the oak boat"
    );

    let taiga = Identifier::vanilla_static("taiga");
    let mut rng = StdRng::seed_from_u64(3);
    let mut refused = LootContext::new(&mut rng).with_this_entity(EntityRef {
        villager_variant: Some(&taiga),
        ..EntityRef::default()
    });
    assert!(
        oak_boat.get_offer(&mut refused).is_none(),
        "a taiga fisherman has no oak boat to sell"
    );

    let mut rng = StdRng::seed_from_u64(3);
    let mut unknown = LootContext::new(&mut rng);
    assert!(
        oak_boat.get_offer(&mut unknown).is_none(),
        "a merchant with no villager type fails the predicate rather than passing it"
    );
}

#[test]
fn a_trade_set_draws_the_number_of_offers_its_amount_asks_for() {
    init_vanilla_registry();
    let mut rng = StdRng::seed_from_u64(4);
    let mut ctx = LootContext::new(&mut rng);

    let set = TradeSet::for_profession(&Identifier::vanilla_static("farmer"), 1)
        .expect("farmer level 1 should exist");
    let mut offers = MerchantOffers::new();
    set.add_offers(&mut ctx, &mut offers);

    assert_eq!(
        offers.len(),
        2,
        "every vanilla level pool but the librarian's fifth draws two"
    );
}

#[test]
fn a_trade_set_without_duplicates_never_offers_the_same_trade_twice() {
    init_vanilla_registry();

    for seed in 0..32 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng);
        let set = TradeSet::for_profession(&Identifier::vanilla_static("shepherd"), 2)
            .expect("shepherd level 2 should exist");
        assert!(
            !set.allow_duplicates,
            "vanilla trade sets forbid duplicates"
        );

        let mut offers = MerchantOffers::new();
        set.add_offers(&mut ctx, &mut offers);

        let mut seen: Vec<&crate::item_stack::ItemStack> = Vec::new();
        for offer in offers.iter() {
            assert!(
                !seen
                    .iter()
                    .any(|result| result.item() == offer.result().item()
                        && result.count() == offer.result().count()),
                "seed {seed} drew the same trade twice"
            );
            seen.push(offer.result());
        }
    }
}

#[test]
fn a_librarian_never_sells_a_book_with_nothing_written_in_it() {
    init_vanilla_registry();

    // `emerald_and_book_enchanted_book` enchants the book and then filters on
    // `stored_enchantments`, discarding on failure. Whatever the roll, the offer
    // is either a genuinely enchanted book or no offer at all -- never a plain one.
    let book = trade("librarian/1/emerald_and_book_enchanted_book");
    for seed in 0..64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng);
        let Some(offer) = book.get_offer(&mut ctx) else {
            continue;
        };
        let stored = offer
            .result()
            .get(crate::data_components::vanilla_components::STORED_ENCHANTMENTS);
        assert!(
            stored.is_some_and(|stored| !stored.is_empty()),
            "seed {seed} offered a book with no stored enchantment"
        );
    }
}

#[test]
fn a_cartographer_never_sells_a_map_that_points_nowhere() {
    init_vanilla_registry();

    // This is the trade whose `on_fail: discard` branch is actually reachable.
    // `minecraft:exploration_map` needs to locate a structure, which a
    // `LootContext` cannot do, so the result stays a blank `map` rather than
    // becoming a `filled_map`; the filter then discards it and the trade
    // withdraws. That is the behavior worth pinning: a cartographer that has
    // no monument to point at offers nothing, it does not sell blank paper.
    //
    // The assertion is written as the invariant rather than as "there is no
    // offer", so it keeps its meaning once exploration maps land.
    let ocean_map = trade("cartographer/3/emerald_and_compass_ocean_explorer_map");
    for seed in 0..64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng);
        let Some(offer) = ocean_map.get_offer(&mut ctx) else {
            continue;
        };
        assert_eq!(
            offer.result().item(),
            &*vanilla_items::FILLED_MAP,
            "seed {seed} offered a map the exploration function never filled in"
        );
    }
}

#[test]
fn an_enchanted_trade_banks_its_cost_instead_of_shipping_the_component() {
    init_vanilla_registry();

    // `enchant_with_levels` writes `ADDITIONAL_TRADE_COST`, and `getOffer` moves
    // it into the price and removes it, so the sold item never carries it.
    let sword = trade("weaponsmith/1/emerald_enchanted_iron_sword");
    let mut priced_above_base = false;

    for seed in 0..64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).allowing_additional_cost_component();
        let Some(offer) = sword.get_offer(&mut ctx) else {
            continue;
        };
        assert!(
            !offer
                .result()
                .has(crate::data_components::vanilla_components::ADDITIONAL_TRADE_COST),
            "the banked cost must be removed from the item that is sold"
        );
        if offer.base_cost_a().count() > 2 {
            priced_above_base = true;
        }
    }

    assert!(
        priced_above_base,
        "an enchanted sword should sometimes cost more than its base two emeralds"
    );
}

#[test]
fn a_trade_only_banks_its_enchanting_cost_when_the_merchant_allows_it() {
    init_vanilla_registry();

    // Vanilla gates the component on `ADDITIONAL_COST_COMPONENT_ALLOWED`, which
    // only a merchant supplies. Without it the sword sells at its base price.
    let sword = trade("weaponsmith/1/emerald_enchanted_iron_sword");
    for seed in 0..64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng);
        let Some(offer) = sword.get_offer(&mut ctx) else {
            continue;
        };
        assert_eq!(
            offer.base_cost_a().count(),
            2,
            "seed {seed} raised the price without the merchant parameter"
        );
    }
}

#[test]
fn a_wandering_traders_water_bottle_asks_for_water_and_not_any_potion() {
    init_vanilla_registry();
    let mut rng = StdRng::seed_from_u64(5);
    let mut ctx = LootContext::new(&mut rng);

    // The one trade in the game whose `wants` carries a component predicate.
    let bottle = trade("wandering_trader/water_bottle_emerald");
    let offer = bottle
        .get_offer(&mut ctx)
        .expect("water bottles always trade");

    let mut water = crate::item_stack::ItemStack::new(&vanilla_items::POTION);
    water.set_potion(&Identifier::vanilla_static("water"));
    assert!(
        offer.item_cost_a().test(&water),
        "a water bottle should pay for this trade"
    );

    let mut healing = crate::item_stack::ItemStack::new(&vanilla_items::POTION);
    healing.set_potion(&Identifier::vanilla_static("healing"));
    assert!(
        !offer.item_cost_a().test(&healing),
        "a healing potion is not a water bottle"
    );
}
