//! Choosing which enchantments to roll onto an item, and at what level.
//!
//! Vanilla parity: the selection half of `EnchantmentHelper` --
//! `selectEnchantment`, `getAvailableEnchantmentResults` and
//! `filterCompatibleEnchantments`, plus the `WeightedRandom.getRandomItem`
//! draw they share. Applying the result is [`crate::item_stack::ItemStack`]'s
//! job, because vanilla's `enchantItem` replaces a plain book with an enchanted
//! one and only the stack can do that.

use rand::{Rng, RngExt as _};

use crate::data_components::vanilla_components::ENCHANTABLE;
use crate::enchantment::{Enchantment, EnchantmentRef};
use crate::item_stack::ItemStack;
use crate::loot_table::EnchantmentOptions;
use crate::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _, vanilla_items};

/// One enchantment at one level, carrying the weight it is drawn by.
///
/// Vanilla parity: `EnchantmentInstance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentInstance {
    /// The enchantment itself.
    pub enchantment: EnchantmentRef,
    /// The level rolled for it.
    pub level: u32,
}

/// Returns every enchantment `options` stands for.
///
/// Vanilla parity: the `Optional<HolderSet<Enchantment>>` each caller of
/// `EnchantmentHelper` passes, which falls back to every element of the
/// enchantment registry when the caller left it out.
#[must_use]
pub fn resolve_options(options: Option<&EnchantmentOptions>) -> Vec<EnchantmentRef> {
    match options {
        None => REGISTRY
            .enchantments
            .iter()
            .map(|(_id, enchantment)| enchantment)
            .collect(),
        Some(EnchantmentOptions::Tag(tag)) => {
            REGISTRY.enchantments.get_tag(tag).unwrap_or_default()
        }
        Some(EnchantmentOptions::List(keys)) => keys
            .iter()
            .filter_map(|key| REGISTRY.enchantments.by_key(key))
            .collect(),
    }
}

/// Returns every enchantment in `source` that could be rolled onto `item` at
/// `value`.
///
/// Vanilla parity: `EnchantmentHelper.getAvailableEnchantmentResults`. Levels
/// are walked from the top down and only the first whose cost window contains
/// `value` is kept, which is what makes an expensive offer yield Sharpness V
/// rather than a choice between all five. A plain book skips the primary-item
/// filter entirely, so any enchantment at all can land on one.
#[must_use]
pub fn available_enchantment_results(
    value: i32,
    item: &ItemStack,
    source: &[EnchantmentRef],
) -> Vec<EnchantmentInstance> {
    let is_book = item.is(&vanilla_items::BOOK);
    let mut results = Vec::new();

    for &enchantment in source {
        if !is_book && !enchantment.is_primary_item(item.item()) {
            continue;
        }

        for level in (Enchantment::MIN_LEVEL..=enchantment.max_level).rev() {
            let level_cost = i32::try_from(level).unwrap_or(i32::MAX);
            if value >= enchantment.min_cost.calculate(level_cost)
                && value <= enchantment.max_cost.calculate(level_cost)
            {
                results.push(EnchantmentInstance { enchantment, level });
                break;
            }
        }
    }

    results
}

/// Rolls the enchantments one offer of `cost` actually grants.
///
/// Vanilla parity: `EnchantmentHelper.selectEnchantment`. The cost is first
/// nudged by the item's enchantability and a small random span, then extra
/// enchantments are drawn with halving odds -- which is why the same offer
/// lands three enchantments on gold and one on iron. An item with no
/// `Enchantable` component yields nothing at all.
#[must_use]
pub fn select_enchantment<R: Rng>(
    rng: &mut R,
    item: &ItemStack,
    cost: i32,
    source: &[EnchantmentRef],
) -> Vec<EnchantmentInstance> {
    let mut results: Vec<EnchantmentInstance> = Vec::new();
    let Some(enchantable) = item.get(ENCHANTABLE) else {
        return results;
    };

    let quarter = enchantable.value() / 4 + 1;
    let mut cost = cost + 1 + rng.random_range(0..quarter) + rng.random_range(0..quarter);

    // Left unfused on purpose: vanilla evaluates `cost + cost * span` as two
    // separately rounded float operations, and the result feeds a rounding
    // boundary.
    let span = (rng.random::<f32>() + rng.random::<f32>() - 1.0) * 0.15;
    cost = java_round(cost as f32 + cost as f32 * span).max(1);

    let mut candidates = available_enchantment_results(cost, item, source);
    if candidates.is_empty() {
        return results;
    }

    if let Some(first) = random_item(rng, &candidates) {
        results.push(first);
    }

    while rng.random_range(0..50) <= cost {
        if let Some(last) = results.last().copied() {
            filter_compatible_enchantments(&mut candidates, last);
        }
        if candidates.is_empty() {
            break;
        }
        if let Some(next) = random_item(rng, &candidates) {
            results.push(next);
        }
        cost /= 2;
    }

    results
}

/// Drops every candidate that clashes with one already chosen.
///
/// Vanilla parity: `EnchantmentHelper.filterCompatibleEnchantments`.
fn filter_compatible_enchantments(
    candidates: &mut Vec<EnchantmentInstance>,
    target: EnchantmentInstance,
) {
    candidates
        .retain(|candidate| Enchantment::are_compatible(target.enchantment, candidate.enchantment));
}

/// Picks one candidate in proportion to its enchantment's weight.
///
/// Vanilla parity: `WeightedRandom.getRandomItem`, which is a different class
/// from the `WeightedList` [`foton_utils::random::weighted_list`] mirrors: it
/// weights a plain list through a getter so the caller can keep mutating that
/// list between draws, which `filterCompatibleEnchantments` relies on.
fn random_item<R: Rng>(
    rng: &mut R,
    candidates: &[EnchantmentInstance],
) -> Option<EnchantmentInstance> {
    let total: i32 = candidates
        .iter()
        .map(|candidate| i32::try_from(candidate.enchantment.weight).unwrap_or(i32::MAX))
        .sum();
    if total <= 0 {
        return None;
    }

    let mut selection = rng.random_range(0..total);
    for candidate in candidates {
        selection -= i32::try_from(candidate.enchantment.weight).unwrap_or(i32::MAX);
        if selection < 0 {
            return Some(*candidate);
        }
    }
    None
}

/// Vanilla parity: `Math.round(float)`, which is `floor(value + 0.5)` and so
/// breaks ties upwards even for negatives, unlike Rust's `f32::round`. The
/// saturating cast stands in for vanilla's `Mth.clamp(.., 1, Integer.MAX_VALUE)`.
fn java_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

#[cfg(test)]
mod tests {
    use foton_utils::Identifier;
    use rand::SeedableRng as _;
    use rand::rngs::StdRng;

    use super::{EnchantmentInstance, available_enchantment_results, resolve_options};
    use crate::data_components::vanilla_components::{
        ENCHANTABLE, ENCHANTMENTS, ItemEnchantments, STORED_ENCHANTMENTS,
    };
    use crate::enchantment::{Enchantment, EnchantmentRef};
    use crate::item_stack::ItemStack;
    use crate::loot_table::EnchantmentOptions;
    use crate::{init_vanilla_registry, vanilla_enchantments, vanilla_items};

    /// One sword-only enchantment and one mining-only one, so a draw that
    /// ignores what the item supports is visible immediately.
    static SHARPNESS_AND_EFFICIENCY: [Identifier; 2] = [
        Identifier::vanilla_static("sharpness"),
        Identifier::vanilla_static("efficiency"),
    ];

    fn seeded_rng(seed: u64) -> StdRng {
        StdRng::seed_from_u64(seed)
    }

    fn efficiency_only() -> [EnchantmentRef; 1] {
        [&vanilla_enchantments::EFFICIENCY]
    }

    /// Efficiency names no primary list, so the only thing keeping it off a
    /// sword is the `isSupportedItem` half of `isPrimaryItem`.
    #[test]
    fn an_enchantment_with_no_primary_list_is_still_only_offered_on_what_it_supports() {
        init_vanilla_registry();
        let source = efficiency_only();
        let pickaxe = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        let sword = ItemStack::new(&vanilla_items::DIAMOND_SWORD);

        assert_eq!(
            available_enchantment_results(30, &pickaxe, &source).len(),
            1
        );
        assert!(available_enchantment_results(30, &sword, &source).is_empty());
    }

    /// Efficiency's windows are `[1,51] [11,61] [21,71] [31,81] [41,91]`, so 45
    /// sits inside all five. Vanilla walks them downwards and keeps the first
    /// that fits, which is what makes an expensive offer worth taking.
    #[test]
    fn a_cost_inside_several_windows_yields_the_highest_level_that_fits() {
        init_vanilla_registry();
        let source = efficiency_only();
        let pickaxe = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);

        let results = available_enchantment_results(45, &pickaxe, &source);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].level, 5);
    }

    /// A plain book is the one item the primary filter is skipped for, and the
    /// swap that follows is what moves the result out of `enchantments`.
    #[test]
    fn a_book_stores_its_enchantment_rather_than_wearing_it() {
        init_vanilla_registry();
        let mut book = ItemStack::new(&vanilla_items::BOOK);
        let mut rng = seeded_rng(7);

        assert!(book.enchant_randomly(None, true, &mut rng).is_some());

        assert!(book.is(&vanilla_items::ENCHANTED_BOOK));
        assert!(
            book.get(STORED_ENCHANTMENTS)
                .is_some_and(|stored| !stored.is_empty())
        );
        assert!(
            book.get(ENCHANTMENTS)
                .is_none_or(ItemEnchantments::is_empty)
        );
    }

    /// `selectEnchantment` bails on a missing `Enchantable` component before it
    /// draws anything. Being a supported item is not enough, which is why the
    /// same sword goes both ways here.
    #[test]
    fn a_roll_needs_the_enchantable_component_and_not_merely_a_supported_item() {
        init_vanilla_registry();
        let mut rng = seeded_rng(11);

        let mut stripped = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        stripped.remove(ENCHANTABLE);
        stripped.enchant_with_levels(30, None, &mut rng);

        let mut intact = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        intact.enchant_with_levels(30, None, &mut rng);

        assert!(
            stripped
                .get_enchantments_for_crafting()
                .is_none_or(ItemEnchantments::is_empty)
        );
        assert!(
            intact
                .get_enchantments_for_crafting()
                .is_some_and(|rolled| !rolled.is_empty())
        );
    }

    /// `only_compatible` is the flag that keeps a loot table from putting a
    /// sword enchantment on a pickaxe.
    #[test]
    fn only_compatible_never_lands_an_enchantment_the_item_does_not_support() {
        init_vanilla_registry();
        let options = EnchantmentOptions::List(&SHARPNESS_AND_EFFICIENCY);
        let mut rng = seeded_rng(3);

        for _ in 0..200 {
            let mut pickaxe = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
            assert!(
                pickaxe
                    .enchant_randomly(Some(&options), true, &mut rng)
                    .is_some()
            );

            assert_eq!(
                pickaxe.get_enchantment_level(&vanilla_enchantments::SHARPNESS.key),
                0
            );
            assert!(pickaxe.get_enchantment_level(&vanilla_enchantments::EFFICIENCY.key) > 0);
        }
    }

    /// The same draw without the flag is what fills a dungeon chest with
    /// nonsense, so the filter has to be the only thing stopping it.
    #[test]
    fn allowing_incompatible_enchantments_lets_a_sword_enchantment_onto_a_pickaxe() {
        init_vanilla_registry();
        let options = EnchantmentOptions::List(&SHARPNESS_AND_EFFICIENCY);
        let mut rng = seeded_rng(3);

        let landed = (0..200).any(|_| {
            let mut pickaxe = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
            pickaxe.enchant_randomly(Some(&options), false, &mut rng);
            pickaxe.get_enchantment_level(&vanilla_enchantments::SHARPNESS.key) > 0
        });

        assert!(landed, "an unfiltered draw should sometimes pick sharpness");
    }

    /// The halving loop can draw several enchantments, and
    /// `filterCompatibleEnchantments` is the only thing keeping Sharpness and
    /// Smite off the same sword.
    #[test]
    fn a_multi_enchantment_roll_never_pairs_two_that_exclude_each_other() {
        init_vanilla_registry();
        let sword = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        let source = resolve_options(None);
        let mut rng = seeded_rng(5);

        let mut saw_multiple = false;
        for _ in 0..200 {
            let rolled = super::select_enchantment(&mut rng, &sword, 30, &source);
            saw_multiple |= rolled.len() > 1;
            assert!(
                all_compatible(&rolled),
                "rolled a clashing pair: {rolled:?}"
            );
        }

        assert!(
            saw_multiple,
            "a thirty-level roll should sometimes multi-hit"
        );
    }

    fn all_compatible(rolled: &[EnchantmentInstance]) -> bool {
        rolled.iter().enumerate().all(|(index, one)| {
            rolled[index + 1..]
                .iter()
                .all(|other| Enchantment::are_compatible(one.enchantment, other.enchantment))
        })
    }
}
