//! Tests for the trade price arithmetic and the offer wire format.
//!
//! The arithmetic is where a villager's price visibly comes from, and the
//! offer's wire layout is fixed-width where most of the protocol is varint, so
//! both are worth pinning.

use std::io::Cursor;

use foton_utils::serial::{ReadFrom as _, WriteTo as _};

use crate::item_stack::ItemStack;
use crate::items::ItemRef;
use crate::trading::{ItemCost, MerchantOffer, MerchantOffers};
use crate::{init_vanilla_registry, vanilla_items};

/// The bool plus six fixed-width fields `MerchantOffer.writeToStream` ends with.
const OFFER_TAIL_LEN: usize = 1 + 4 * 6;

fn cost(item: ItemRef, count: i32) -> ItemCost {
    init_vanilla_registry();
    ItemCost::new(item, count)
}

fn stack(item: ItemRef, count: i32) -> ItemStack {
    init_vanilla_registry();
    ItemStack::with_count(item, count)
}

/// A farmer's "20 wheat for an emerald", which is demand-sensitive.
fn wheat_for_emerald(demand: i32) -> MerchantOffer {
    MerchantOffer::with_uses(
        cost(&vanilla_items::WHEAT, 20),
        None,
        stack(&vanilla_items::EMERALD, 1),
        0,
        16,
        2,
        0.05,
        demand,
    )
}

/// A two-cost trade: five emeralds and three wheat for a loaf.
fn bread_for_emeralds_and_wheat(demand: i32) -> MerchantOffer {
    MerchantOffer::with_uses(
        cost(&vanilla_items::EMERALD, 5),
        Some(cost(&vanilla_items::WHEAT, 3)),
        stack(&vanilla_items::BREAD, 1),
        0,
        12,
        1,
        0.05,
        demand,
    )
}

#[test]
fn a_fresh_offer_asks_exactly_its_base_price() {
    let offer = wheat_for_emerald(0);

    assert_eq!(offer.cost_a().count(), 20);
    assert!(offer.cost_b().is_empty());
    assert!(!offer.is_out_of_stock());
    assert!(!offer.needs_restock());
}

#[test]
fn demand_raises_the_first_price_and_never_lowers_it() {
    // floor(20 * 4 * 0.05) = 4
    assert_eq!(wheat_for_emerald(4).cost_a().count(), 24);
    // floor(20 * 1 * 0.05) = 1
    assert_eq!(wheat_for_emerald(1).cost_a().count(), 21);
    // floor(20 * 9 * 0.05) = 9
    assert_eq!(wheat_for_emerald(9).cost_a().count(), 29);
    // Negative demand is clamped away before it can discount anything.
    assert_eq!(wheat_for_emerald(-40).cost_a().count(), 20);
}

#[test]
fn reputation_discounts_the_price_but_never_below_one_item() {
    let mut offer = wheat_for_emerald(0);

    offer.add_to_special_price_diff(-5);
    assert_eq!(offer.cost_a().count(), 15);

    offer.add_to_special_price_diff(-100);
    assert_eq!(
        offer.cost_a().count(),
        1,
        "a hero of the village still pays something"
    );

    offer.reset_special_price_diff();
    assert_eq!(offer.cost_a().count(), 20);
}

#[test]
fn the_price_never_exceeds_a_stack() {
    let mut offer = wheat_for_emerald(0);
    offer.add_to_special_price_diff(1000);

    assert_eq!(
        offer.cost_a().count(),
        stack(&vanilla_items::WHEAT, 1).max_stack_size()
    );
}

#[test]
fn payment_is_judged_against_the_moved_price_not_the_base_one() {
    let offer = wheat_for_emerald(4); // price is now 24

    assert!(!offer.satisfied_by(&stack(&vanilla_items::WHEAT, 20), &ItemStack::empty()));
    assert!(offer.satisfied_by(&stack(&vanilla_items::WHEAT, 24), &ItemStack::empty()));
    assert!(offer.satisfied_by(&stack(&vanilla_items::WHEAT, 30), &ItemStack::empty()));
}

#[test]
fn a_one_cost_trade_refuses_a_second_payment_stack() {
    let offer = wheat_for_emerald(0);

    assert!(!offer.satisfied_by(
        &stack(&vanilla_items::WHEAT, 20),
        &stack(&vanilla_items::EMERALD, 1)
    ));
}

#[test]
fn the_wrong_item_never_pays_for_a_trade() {
    let offer = wheat_for_emerald(0);

    assert!(!offer.satisfied_by(&stack(&vanilla_items::CARROT, 64), &ItemStack::empty()));
}

#[test]
fn the_second_price_ignores_demand() {
    let offer = bread_for_emeralds_and_wheat(10);

    // floor(5 * 10 * 0.05) = 2
    assert_eq!(offer.cost_a().count(), 7);
    assert_eq!(
        offer.cost_b().count(),
        3,
        "vanilla moves only the primary cost with demand"
    );
    assert!(!offer.satisfied_by(
        &stack(&vanilla_items::EMERALD, 7),
        &stack(&vanilla_items::WHEAT, 2)
    ));
    assert!(offer.satisfied_by(
        &stack(&vanilla_items::EMERALD, 7),
        &stack(&vanilla_items::WHEAT, 3)
    ));
}

#[test]
fn taking_a_trade_spends_the_moved_price_out_of_both_stacks() {
    let offer = bread_for_emeralds_and_wheat(10); // primary price is now 7

    let mut paid_a = stack(&vanilla_items::EMERALD, 10);
    let mut paid_b = stack(&vanilla_items::WHEAT, 5);
    assert!(offer.take(&mut paid_a, &mut paid_b));

    assert_eq!(paid_a.count(), 3);
    assert_eq!(paid_b.count(), 2);
}

#[test]
fn a_trade_that_cannot_be_paid_for_takes_nothing() {
    let offer = wheat_for_emerald(0);
    let mut paid_a = stack(&vanilla_items::WHEAT, 19);
    let mut paid_b = ItemStack::empty();

    assert!(!offer.take(&mut paid_a, &mut paid_b));
    assert_eq!(
        paid_a.count(),
        19,
        "a refused trade must not eat the payment"
    );
}

#[test]
fn stock_runs_out_after_max_uses_and_demand_folds_the_uses_in() {
    let mut offer = wheat_for_emerald(0);

    for _ in 0..15 {
        offer.increase_uses();
    }
    assert!(!offer.is_out_of_stock());
    assert!(offer.needs_restock());

    offer.increase_uses();
    assert!(offer.is_out_of_stock());

    // demand += uses - (maxUses - uses) = 0 + 16 - 0 = 16
    offer.update_demand();
    assert_eq!(offer.demand(), 16);

    offer.reset_uses();
    assert!(!offer.is_out_of_stock());
    assert!(!offer.needs_restock());

    // A restocked, untouched trade sheds the demand it had built up.
    offer.update_demand();
    assert_eq!(offer.demand(), 0);
}

#[test]
fn the_selection_hint_picks_between_two_trades_wanting_the_same_item() {
    let bread = MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 1),
        None,
        stack(&vanilla_items::BREAD, 1),
        16,
        1,
        0.05,
    );
    let carrots = MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 1),
        None,
        stack(&vanilla_items::CARROT, 1),
        16,
        1,
        0.05,
    );
    let offers: MerchantOffers = vec![bread, carrots].into();
    let paid = stack(&vanilla_items::EMERALD, 1);

    let hinted = offers
        .recipe_for(&paid, &ItemStack::empty(), 1)
        .expect("the hinted trade is payable");
    assert!(hinted.result().is(&vanilla_items::CARROT));

    let scanned = offers
        .recipe_for(&paid, &ItemStack::empty(), 0)
        .expect("the scan finds the first payable trade");
    assert!(
        scanned.result().is(&vanilla_items::BREAD),
        "hint 0 falls through to the scan, which reaches trade 0 anyway"
    );
}

#[test]
fn an_out_of_range_selection_hint_falls_back_to_the_scan() {
    let offers: MerchantOffers = vec![wheat_for_emerald(0)].into();
    let paid = stack(&vanilla_items::WHEAT, 20);

    assert!(offers.recipe_for(&paid, &ItemStack::empty(), 99).is_some());
}

#[test]
fn a_hinted_trade_the_payment_does_not_cover_matches_nothing() {
    let cheap = MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 1),
        None,
        stack(&vanilla_items::BREAD, 1),
        16,
        1,
        0.05,
    );
    let dear = MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 8),
        None,
        stack(&vanilla_items::CARROT, 1),
        16,
        1,
        0.05,
    );
    let offers: MerchantOffers = vec![cheap, dear].into();

    assert!(
        offers
            .recipe_for(&stack(&vanilla_items::EMERALD, 1), &ItemStack::empty(), 1)
            .is_none(),
        "the hint is exclusive: it must not fall back to the affordable trade"
    );
}

#[test]
fn an_offer_survives_the_wire_round_trip() {
    let mut offer = MerchantOffer::with_uses(
        cost(&vanilla_items::EMERALD, 5),
        Some(cost(&vanilla_items::WHEAT, 3)),
        stack(&vanilla_items::BREAD, 6),
        2,
        12,
        7,
        0.05,
        4,
    );
    offer.add_to_special_price_diff(-3);

    let mut encoded = Vec::new();
    offer.write(&mut encoded).expect("offer encodes");
    let decoded = MerchantOffer::read(&mut Cursor::new(encoded.as_slice())).expect("offer decodes");

    assert_eq!(decoded, offer);
    assert_eq!(decoded.cost_a().count(), offer.cost_a().count());
}

#[test]
fn an_exhausted_offer_says_so_on_the_wire() {
    // The flag is redundant with uses >= maxUses for anything that reconstructs
    // the offer the way Foton does, so a round-trip alone cannot tell whether it
    // was written at all. The client reads the flag, so the byte is asserted.
    let mut offer = wheat_for_emerald(0);
    offer.set_to_out_of_stock();

    let mut encoded = Vec::new();
    offer.write(&mut encoded).expect("offer encodes");
    assert_eq!(
        encoded[encoded.len() - OFFER_TAIL_LEN],
        1,
        "the out-of-stock flag must carry the offer's actual state"
    );

    let decoded = MerchantOffer::read(&mut Cursor::new(encoded.as_slice())).expect("offer decodes");
    assert!(decoded.is_out_of_stock());
}

#[test]
fn the_offer_tail_is_fixed_width_where_vanilla_says_it_is() {
    // Vanilla's writeToStream ends in a bool and six fixed-width fields. Getting
    // any of them wrong desynchronizes the whole trade screen, and a varint
    // would happen to encode small values identically -- so the bytes are
    // checked directly.
    let offer = MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 1),
        None,
        stack(&vanilla_items::BREAD, 1),
        12,
        7,
        0.5,
    );

    let mut encoded = Vec::new();
    offer.write(&mut encoded).expect("offer encodes");

    let tail = &encoded[encoded.len() - OFFER_TAIL_LEN..];
    assert_eq!(tail[0], 0, "out-of-stock flag is a single byte");
    assert_eq!(&tail[1..5], &0i32.to_be_bytes(), "uses");
    assert_eq!(&tail[5..9], &12i32.to_be_bytes(), "maxUses");
    assert_eq!(&tail[9..13], &7i32.to_be_bytes(), "xp");
    assert_eq!(&tail[13..17], &0i32.to_be_bytes(), "specialPrice");
    assert_eq!(&tail[17..21], &0.5f32.to_be_bytes(), "priceMultiplier");
    assert_eq!(&tail[21..25], &0i32.to_be_bytes(), "demand");
}

#[test]
fn an_offer_list_survives_the_wire_round_trip() {
    let offers: MerchantOffers = vec![wheat_for_emerald(0), bread_for_emeralds_and_wheat(2)].into();

    let mut encoded = Vec::new();
    offers.write(&mut encoded).expect("offers encode");
    let decoded =
        MerchantOffers::read(&mut Cursor::new(encoded.as_slice())).expect("offers decode");

    assert_eq!(decoded, offers);
    assert_eq!(decoded.len(), 2);
}
