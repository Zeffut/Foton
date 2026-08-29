//! Tests for the rules that decide what a village charges a player.

use std::io::Cursor;

use rand::SeedableRng as _;
use rand::rngs::StdRng;
use simdnbt::borrow::{
    NbtCompound as BorrowedNbtCompoundView, read_compound as read_borrowed_compound,
};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use uuid::Uuid;

use super::{GossipContainer, GossipType};

/// Writes a container the way an entity's `save_additional` does, then reads it
/// back the way `load_additional` does.
fn round_trip(gossips: &GossipContainer) -> GossipContainer {
    let mut nbt = NbtCompound::new();
    nbt.insert("Gossips", gossips.save());

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("gossip nbt should reborrow: {error}"));

    let view: BorrowedNbtCompoundView<'_, '_> = (&borrowed).into();
    let mut restored = GossipContainer::new();
    if let Some(list) = view.list("Gossips") {
        restored.load(&list);
    }
    restored
}

fn player() -> Uuid {
    Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0)
}

fn other_player() -> Uuid {
    Uuid::from_u128(0x0fed_cba9_8765_4321_0fed_cba9_8765_4321)
}

fn anything(_: GossipType) -> bool {
    true
}

#[test]
fn reputation_is_the_weighted_sum_and_a_major_negative_outweighs_five_trades() {
    let mut gossips = GossipContainer::new();

    // Five trades at one point each, weight one.
    gossips.add(player(), GossipType::Trading, 5);
    assert_eq!(gossips.reputation(player(), anything), 5);

    // One major negative point is worth minus five.
    gossips.add(player(), GossipType::MajorNegative, 2);
    assert_eq!(
        gossips.reputation(player(), anything),
        5 - 10,
        "a major negative is weighted five times a trade"
    );
}

#[test]
fn a_filter_can_read_one_kind_of_memory_without_the_others() {
    let mut gossips = GossipContainer::new();
    gossips.add(player(), GossipType::Trading, 10);
    gossips.add(player(), GossipType::MajorNegative, 10);

    let only_trading = gossips.reputation(player(), |kind| kind == GossipType::Trading);
    assert_eq!(only_trading, 10);
}

#[test]
fn an_entry_is_capped_at_its_types_maximum_however_often_it_is_added() {
    let mut gossips = GossipContainer::new();
    for _ in 0..20 {
        gossips.add(
            player(),
            GossipType::Trading,
            GossipType::REPUTATION_CHANGE_PER_TRADE,
        );
    }

    assert_eq!(
        gossips.reputation(player(), anything),
        GossipType::Trading.max(),
        "trading tops out at twenty-five points, so grinding trades stops paying"
    );
}

#[test]
fn a_days_decay_wears_a_memory_down_and_finally_forgets_it() {
    let mut gossips = GossipContainer::new();
    gossips.add(player(), GossipType::MinorNegative, 25);

    // Minor negative loses twenty a day.
    gossips.decay();
    assert_eq!(gossips.reputation(player(), anything), -5);

    // Five is below the discard threshold once twenty more comes off, so the
    // entry is dropped rather than left at a negative value.
    gossips.decay();
    assert_eq!(gossips.reputation(player(), anything), 0);
    assert!(
        gossips.is_empty(),
        "an exhausted memory leaves no target behind"
    );
}

#[test]
fn curing_a_zombie_villager_leaves_a_discount_that_no_amount_of_decay_removes() {
    let mut gossips = GossipContainer::new();
    // Vanilla's `onReputationEventFrom` for ZOMBIE_VILLAGER_CURED.
    gossips.add(
        player(),
        GossipType::MajorPositive,
        GossipType::REPUTATION_CHANGE_PER_EVERLASTING_MEMORY,
    );
    gossips.add(
        player(),
        GossipType::MinorPositive,
        GossipType::REPUTATION_CHANGE_PER_EVENT,
    );

    let fresh = gossips.reputation(player(), anything);
    assert_eq!(
        fresh, 125,
        "twenty major-positive points at weight five plus twenty-five minor at weight one"
    );

    for _ in 0..100 {
        gossips.decay();
    }
    let weathered = gossips.reputation(player(), anything);

    assert!(weathered < fresh, "the minor half should wear off");
    // The literal matters: `Villager.updateSpecialPrices` discounts each trade
    // by `floor(reputation * priceMultiplier)`, so a hundred points is five
    // emeralds off a 0.05 trade and twenty off a 0.2 one -- for good. That is
    // the whole return on building a cure farm, and it must not drift.
    assert_eq!(
        weathered, 100,
        "the major half decays by zero a day, so a cure is remembered forever"
    );
}

#[test]
fn retelling_a_memory_costs_it_the_transfer_decay() {
    let mut teller = GossipContainer::new();
    teller.add(player(), GossipType::MajorNegative, 100);

    let mut listener = GossipContainer::new();
    let mut rng = StdRng::seed_from_u64(1);
    listener.transfer_from(&teller, &mut rng, 10);

    assert_eq!(
        listener.reputation(player(), anything),
        (100 - GossipType::MajorNegative.decay_per_transfer()) * GossipType::MajorNegative.weight(),
        "a retold memory arrives ten points weaker"
    );
    assert_eq!(
        teller.reputation(player(), anything),
        100 * GossipType::MajorNegative.weight(),
        "telling someone does not cost the teller anything"
    );
}

#[test]
fn a_memory_too_weak_to_survive_the_retelling_is_not_passed_on_at_all() {
    let mut teller = GossipContainer::new();
    // Minor positive loses twenty in transfer, so twenty-one arrives at one --
    // under the discard threshold of two.
    teller.add(player(), GossipType::MinorPositive, 21);

    let mut listener = GossipContainer::new();
    let mut rng = StdRng::seed_from_u64(2);
    listener.transfer_from(&teller, &mut rng, 10);

    assert_eq!(listener.reputation(player(), anything), 0);
    assert!(listener.is_empty());
}

#[test]
fn a_retold_memory_never_compounds_on_one_already_held() {
    let mut teller = GossipContainer::new();
    teller.add(player(), GossipType::MajorNegative, 100);

    let mut listener = GossipContainer::new();
    listener.add(player(), GossipType::MajorNegative, 100);

    let mut rng = StdRng::seed_from_u64(3);
    listener.transfer_from(&teller, &mut rng, 10);

    assert_eq!(
        listener.reputation(player(), anything),
        100 * GossipType::MajorNegative.weight(),
        "hearing what you already know more strongly changes nothing"
    );
}

#[test]
fn a_transfer_never_carries_more_entries_than_it_was_allowed() {
    let mut teller = GossipContainer::new();
    for index in 0..8u128 {
        teller.add(Uuid::from_u128(index + 1), GossipType::MajorNegative, 100);
    }

    let mut listener = GossipContainer::new();
    let mut rng = StdRng::seed_from_u64(4);
    listener.transfer_from(&teller, &mut rng, 3);

    let carried = (0..8u128)
        .filter(|index| listener.reputation(Uuid::from_u128(index + 1), anything) != 0)
        .count();
    assert!(
        carried <= 3,
        "three draws cannot produce more than three distinct memories, got {carried}"
    );
    assert!(
        carried > 0,
        "three draws over eight strong memories should carry something"
    );
}

#[test]
fn a_container_survives_the_save_and_load_round_trip() {
    let mut gossips = GossipContainer::new();
    gossips.add(player(), GossipType::MajorPositive, 20);
    gossips.add(player(), GossipType::Trading, 7);
    gossips.add(other_player(), GossipType::MinorNegative, 30);

    let restored = round_trip(&gossips);

    assert_eq!(
        restored, gossips,
        "a saved village remembers exactly what it knew"
    );
}

#[test]
fn a_saved_entry_with_no_value_left_is_not_restored() {
    // Vanilla's `ExtraCodecs.POSITIVE_INT` refuses it, so a corrupt or
    // hand-edited save cannot resurrect an emptied memory.
    let mut entry = NbtCompound::new();
    entry.insert(
        "Target",
        NbtTag::IntArray(foton_utils::UuidExt::to_int_array(&player()).to_vec()),
    );
    entry.insert("Type", GossipType::Trading.id());
    entry.insert("Value", 0_i32);

    let mut nbt = NbtCompound::new();
    nbt.insert("Gossips", NbtTag::List(NbtList::from(vec![entry])));
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("gossip nbt should reborrow: {error}"));

    let view: BorrowedNbtCompoundView<'_, '_> = (&borrowed).into();
    let mut restored = GossipContainer::new();
    if let Some(list) = view.list("Gossips") {
        restored.load(&list);
    }
    assert!(restored.is_empty());
}
