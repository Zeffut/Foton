//! Choosing which enchantments an enchanting table offers, and rolling them.
//!
//! Vanilla parity: the selection half of `EnchantmentHelper` --
//! `getEnchantmentCost`, `getAvailableEnchantmentResults`, `selectEnchantment`
//! and `enchantItem`. Foton's [`crate::enchantment_helper`] covers the other
//! half, applying enchantments once an item has them; nothing here had an
//! implementation, which is why an enchanting table had nothing to offer.

use std::sync::LazyLock;

use foton_registry::REGISTRY;
use foton_registry::TaggedRegistryExt as _;
use foton_registry::data_components::vanilla_components::ENCHANTABLE;
use foton_registry::enchantment::{Enchantment, EnchantmentRef};
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_enchantment_tags::EnchantmentTag;
use foton_registry::vanilla_items;
use foton_utils::random::Random;

/// Most bookshelves an enchanting table counts.
///
/// Vanilla parity: the `if (bookcases > 15)` clamp of `getEnchantmentCost`.
/// Fifteen is the ring of shelves around a table; a sixteenth adds nothing.
pub const MAX_BOOKSHELVES: i32 = 15;

/// Enchantment offers a table shows at once.
pub const OFFER_COUNT: usize = 3;

/// One enchantment at one level, with the weight it is drawn by.
///
/// Vanilla parity: `EnchantmentInstance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentInstance {
    /// The enchantment itself.
    pub enchantment: EnchantmentRef,
    /// The level rolled for it.
    pub level: u32,
}

/// Returns the experience level the given offer slot asks for.
///
/// Vanilla parity: `EnchantmentHelper.getEnchantmentCost`. The three slots read
/// the same roll differently, which is why the top offer is always cheap and
/// the bottom one only reaches thirty with a full ring of shelves.
#[must_use]
pub fn enchantment_cost(
    random: &mut impl Random,
    slot: usize,
    bookshelves: i32,
    item: &ItemStack,
) -> i32 {
    if item.get(ENCHANTABLE).is_none() {
        return 0;
    }

    let bookshelves = bookshelves.min(MAX_BOOKSHELVES);
    let selected = random.next_i32_bounded(8)
        + 1
        + (bookshelves >> 1)
        + random.next_i32_bounded(bookshelves + 1);

    match slot {
        0 => (selected / 3).max(1),
        1 => selected * 2 / 3 + 1,
        _ => selected.max(bookshelves * 2),
    }
}

/// The enchantments an enchanting table is allowed to roll.
///
/// Vanilla parity: `EnchantmentTags.IN_ENCHANTING_TABLE`, which the menu passes
/// to `selectEnchantment` instead of the whole registry. It is why Mending and
/// the curses never appear at a table however many shelves surround it.
#[must_use]
pub fn enchanting_table_candidates() -> &'static [EnchantmentRef] {
    static CANDIDATES: LazyLock<Vec<EnchantmentRef>> = LazyLock::new(|| {
        REGISTRY
            .enchantments
            .iter()
            .map(|(_id, enchantment)| enchantment)
            .filter(|enchantment| {
                REGISTRY
                    .enchantments
                    .is_in_tag(*enchantment, &EnchantmentTag::IN_ENCHANTING_TABLE)
            })
            .collect()
    });
    &CANDIDATES
}

/// Returns every enchantment in `source` that could be rolled onto this item at
/// `value`.
///
/// Vanilla parity: `EnchantmentHelper.getAvailableEnchantmentResults`. Only the
/// highest level whose cost window contains `value` is offered per enchantment,
/// which is what makes a high-level table offer Sharpness V rather than a
/// choice between all five. The candidate set is the caller's, because a table
/// draws from a narrower list than a loot table does.
#[must_use]
pub fn available_enchantment_results(
    value: i32,
    item: &ItemStack,
    source: &[EnchantmentRef],
) -> Vec<EnchantmentInstance> {
    let is_book = item.is(&vanilla_items::BOOK);
    let mut results = Vec::new();

    for &enchantment in source {
        if !is_book && !is_primary_item(enchantment, item) {
            continue;
        }

        // Walk down from the top level and take the first that fits, exactly as
        // vanilla does; the windows overlap, so walking up would offer level one
        // of everything.
        for level in (1..=enchantment.max_level).rev() {
            if value >= min_cost(enchantment, level) && value <= max_cost(enchantment, level) {
                results.push(EnchantmentInstance { enchantment, level });
                break;
            }
        }
    }

    results
}

/// Rolls the enchantments one offer actually grants.
///
/// Vanilla parity: `EnchantmentHelper.selectEnchantment`. The cost is nudged by
/// the item's enchantability and a small random span before anything is looked
/// up, then extra enchantments are drawn with halving odds -- which is why a
/// thirty-level offer on a golden item often lands three enchantments and the
/// same offer on iron lands one.
#[must_use]
pub fn select_enchantment(
    random: &mut impl Random,
    item: &ItemStack,
    cost: i32,
    source: &[EnchantmentRef],
) -> Vec<EnchantmentInstance> {
    let mut results: Vec<EnchantmentInstance> = Vec::new();
    let Some(enchantable) = item.get(ENCHANTABLE) else {
        return results;
    };

    let quarter = enchantable.value() / 4 + 1;
    let mut cost = cost + 1 + random.next_i32_bounded(quarter) + random.next_i32_bounded(quarter);

    let span = (random.next_f32() + random.next_f32() - 1.0) * 0.15;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "vanilla rounds this to an int and the value is a small cost"
    )]
    let nudged = (cost as f32).mul_add(span, cost as f32).round() as i32;
    cost = nudged.max(1);

    let mut candidates = available_enchantment_results(cost, item, source);
    if candidates.is_empty() {
        return results;
    }

    if let Some(first) = draw_weighted(random, &candidates) {
        results.push(first);
    }

    while random.next_i32_bounded(50) <= cost {
        if let Some(last) = results.last().copied() {
            retain_compatible(&mut candidates, last);
        }
        if candidates.is_empty() {
            break;
        }
        if let Some(next) = draw_weighted(random, &candidates) {
            results.push(next);
        }
        cost /= 2;
    }

    results
}

/// Writes an already-rolled offer onto an item.
///
/// Vanilla parity: the second half of `EnchantmentHelper.enchantItem`. A plain
/// book becomes an enchanted book, which is the only case where the item itself
/// changes -- and the only reason this is worth naming, because the enchanting
/// table rolls its offer in two steps (once for the clue it shows before the
/// click, once for the click) and must not re-implement the swap.
#[must_use]
pub fn apply_enchantments(item: &ItemStack, rolled: &[EnchantmentInstance]) -> ItemStack {
    let mut result = if item.is(&vanilla_items::BOOK) {
        ItemStack::new(&vanilla_items::ENCHANTED_BOOK)
    } else {
        item.clone()
    };

    let applied: Vec<_> = rolled
        .iter()
        .map(|instance| (instance.enchantment.key.clone(), instance.level))
        .collect();
    result.set_enchantments(&applied, true);
    result
}

/// Drops every candidate that clashes with one already chosen.
///
/// Vanilla parity: `EnchantmentHelper.filterCompatibleEnchantments`.
fn retain_compatible(candidates: &mut Vec<EnchantmentInstance>, chosen: EnchantmentInstance) {
    candidates
        .retain(|candidate| Enchantment::are_compatible(chosen.enchantment, candidate.enchantment));
}

/// Picks one candidate, weighted by how common vanilla makes it.
///
/// Vanilla parity: `WeightedRandom.getRandomItem`.
fn draw_weighted(
    random: &mut impl Random,
    candidates: &[EnchantmentInstance],
) -> Option<EnchantmentInstance> {
    let total: i32 = candidates
        .iter()
        .map(|candidate| i32::try_from(candidate.enchantment.weight).unwrap_or(i32::MAX))
        .sum();
    if total <= 0 {
        return None;
    }

    let mut roll = random.next_i32_bounded(total);
    for candidate in candidates {
        roll -= i32::try_from(candidate.enchantment.weight).unwrap_or(i32::MAX);
        if roll < 0 {
            return Some(*candidate);
        }
    }
    None
}

/// Returns whether an enchanting table may roll this enchantment onto this item.
///
/// Vanilla parity: `Enchantment.isPrimaryItem`. An enchantment with no primary
/// set may go on anything it supports; one with a primary set is only rolled
/// onto that narrower list, which is how Mending stays off the table entirely
/// while still being applicable from a book.
#[must_use]
pub fn is_primary_item(enchantment: EnchantmentRef, item: &ItemStack) -> bool {
    enchantment.can_enchant(item.item()) && enchantment.is_primary_item(item.item())
}

/// Vanilla parity: `Enchantment.getMinCost`.
fn min_cost(enchantment: EnchantmentRef, level: u32) -> i32 {
    let level = i32::try_from(level).unwrap_or(i32::MAX);
    enchantment.min_cost.base + enchantment.min_cost.per_level_above_first * (level - 1)
}

/// Vanilla parity: `Enchantment.getMaxCost`.
fn max_cost(enchantment: EnchantmentRef, level: u32) -> i32 {
    let level = i32::try_from(level).unwrap_or(i32::MAX);
    enchantment.max_cost.base + enchantment.max_cost.per_level_above_first * (level - 1)
}

#[cfg(test)]
mod tests {
    use foton_registry::init_vanilla_registry;
    use foton_registry::vanilla_enchantments;
    use foton_utils::random::legacy_random::LegacyRandom;

    use super::*;

    fn diamond_sword() -> ItemStack {
        ItemStack::new(&vanilla_items::DIAMOND_SWORD)
    }

    #[test]
    fn an_unenchantable_item_costs_nothing() {
        init_vanilla_registry();
        let mut random = LegacyRandom::from_seed(1);
        let stone = ItemStack::new(&vanilla_items::STONE);
        for slot in 0..OFFER_COUNT {
            assert_eq!(enchantment_cost(&mut random, slot, 15, &stone), 0);
        }
    }

    #[test]
    fn the_bottom_offer_needs_a_full_ring_of_shelves() {
        init_vanilla_registry();
        let sword = diamond_sword();

        // With no shelves the third offer cannot reach the thirty levels a
        // player expects; with fifteen it always does, because the slot takes
        // the greater of the roll and twice the shelf count.
        let mut bare = LegacyRandom::from_seed(7);
        let mut ringed = LegacyRandom::from_seed(7);
        let without = enchantment_cost(&mut bare, 2, 0, &sword);
        let with = enchantment_cost(&mut ringed, 2, MAX_BOOKSHELVES, &sword);

        assert!(without < 30);
        assert_eq!(with, 30);
    }

    #[test]
    fn more_shelves_than_the_ring_change_nothing() {
        init_vanilla_registry();
        let sword = diamond_sword();
        let mut fifteen = LegacyRandom::from_seed(3);
        let mut fifty = LegacyRandom::from_seed(3);

        assert_eq!(
            enchantment_cost(&mut fifteen, 2, MAX_BOOKSHELVES, &sword),
            enchantment_cost(&mut fifty, 2, 50, &sword)
        );
    }

    #[test]
    fn the_first_offer_is_always_the_cheapest() {
        init_vanilla_registry();
        let sword = diamond_sword();
        for seed in 0..40 {
            let mut first = LegacyRandom::from_seed(seed);
            let mut third = LegacyRandom::from_seed(seed);
            let cheap = enchantment_cost(&mut first, 0, MAX_BOOKSHELVES, &sword);
            let dear = enchantment_cost(&mut third, 2, MAX_BOOKSHELVES, &sword);
            assert!(
                cheap <= dear,
                "seed {seed}: {cheap} should not exceed {dear}"
            );
        }
    }

    #[test]
    fn a_low_offer_never_reaches_the_top_levels() {
        init_vanilla_registry();
        let results =
            available_enchantment_results(1, &diamond_sword(), enchanting_table_candidates());
        assert!(
            results
                .iter()
                .all(|instance| instance.level < vanilla_enchantments::SHARPNESS.max_level),
            "a one-level offer must not grant a maxed enchantment"
        );
    }

    #[test]
    fn only_one_level_of_each_enchantment_is_offered() {
        init_vanilla_registry();
        let results =
            available_enchantment_results(30, &diamond_sword(), enchanting_table_candidates());
        // Identifier is not Ord, so compare by the rendered key instead.
        let mut seen: Vec<String> = results
            .iter()
            .map(|instance| instance.enchantment.key.to_string())
            .collect();
        let before = seen.len();
        seen.sort();
        seen.dedup();
        assert_eq!(before, seen.len(), "an enchantment was offered twice");
    }

    #[test]
    fn a_book_can_take_enchantments_a_sword_cannot() {
        init_vanilla_registry();
        let book_results = available_enchantment_results(
            30,
            &ItemStack::new(&vanilla_items::BOOK),
            enchanting_table_candidates(),
        );
        let sword_results =
            available_enchantment_results(30, &diamond_sword(), enchanting_table_candidates());

        // A book is the one item every enchantment is willing to sit on, so it
        // must offer strictly more than a sword does.
        assert!(book_results.len() > sword_results.len());
    }

    #[test]
    fn rolled_enchantments_never_clash() {
        init_vanilla_registry();
        let sword = diamond_sword();

        for seed in 0..60 {
            let mut random = LegacyRandom::from_seed(seed);
            let rolled = select_enchantment(&mut random, &sword, 30, enchanting_table_candidates());
            for (index, one) in rolled.iter().enumerate() {
                for other in &rolled[index + 1..] {
                    assert!(
                        Enchantment::are_compatible(one.enchantment, other.enchantment),
                        "seed {seed} rolled {} with {}",
                        one.enchantment.key,
                        other.enchantment.key
                    );
                }
            }
        }
    }

    #[test]
    fn the_table_draws_from_a_narrower_list_than_the_registry() {
        init_vanilla_registry();
        let table = enchanting_table_candidates();
        let everything = REGISTRY.enchantments.iter().count();

        assert!(!table.is_empty());
        assert!(
            table.len() < everything,
            "the enchanting table tag must exclude something, or curses would be rollable"
        );
    }

    #[test]
    fn enchanting_a_plain_book_produces_an_enchanted_one() {
        init_vanilla_registry();
        let mut random = LegacyRandom::from_seed(11);
        let book = ItemStack::new(&vanilla_items::BOOK);
        let rolled = select_enchantment(&mut random, &book, 30, enchanting_table_candidates());
        let result = apply_enchantments(&book, &rolled);
        assert!(result.is(&vanilla_items::ENCHANTED_BOOK));
    }

    #[test]
    fn a_thirty_level_offer_actually_enchants() {
        init_vanilla_registry();
        let mut random = LegacyRandom::from_seed(5);
        let sword = diamond_sword();
        let rolled = select_enchantment(&mut random, &sword, 30, enchanting_table_candidates());
        let result = apply_enchantments(&sword, &rolled);

        let enchantments = result
            .get_enchantments_for_crafting()
            .expect("an enchanted sword carries enchantments");
        assert!(!enchantments.is_empty());
    }
}
