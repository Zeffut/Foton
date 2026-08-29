//! Furnace fuel values.
//!
//! Vanilla parity: `FuelValues.vanillaBurnTimes`. The table is built once in the
//! same insertion order as vanilla, because later entries overwrite earlier ones
//! for items that a tag already covered, and the final `NON_FLAMMABLE_WOOD`
//! removal drops crimson and warped wood again.

use std::sync::LazyLock;

use foton_utils::Identifier;
use rustc_hash::FxHashMap;

use crate::{
    REGISTRY, TaggedRegistryExt, item_stack::ItemStack, items::ItemRef, vanilla_item_tags::ItemTag,
    vanilla_items,
};

/// Ticks one smelting operation takes, and the unit every burn time is a multiple of.
///
/// Vanilla parity: the `baseUnit` argument of `FuelValues.vanillaBurnTimes`.
const BASE_UNIT: i32 = 200;

/// Burn time in ticks for every item that can fuel a furnace.
static FUEL_VALUES: LazyLock<FxHashMap<Identifier, i32>> = LazyLock::new(build_vanilla_burn_times);

/// Returns whether the stack can be used as furnace fuel.
///
/// Vanilla parity: `FuelValues.isFuel`.
#[must_use]
pub fn is_fuel(stack: &ItemStack) -> bool {
    !stack.is_empty() && FUEL_VALUES.contains_key(&stack.item().key)
}

/// Returns how many ticks the stack burns for, or zero if it is not fuel.
///
/// Vanilla parity: `FuelValues.burnDuration`.
#[must_use]
pub fn burn_duration(stack: &ItemStack) -> i32 {
    if stack.is_empty() {
        return 0;
    }
    FUEL_VALUES
        .get(&stack.item().key)
        .copied()
        .unwrap_or_default()
}

/// Builds the vanilla fuel table.
///
/// The order of the statements mirrors `FuelValues.vanillaBurnTimes` exactly: a
/// value inserted later replaces one inserted earlier for the same item.
fn build_vanilla_burn_times() -> FxHashMap<Identifier, i32> {
    let mut values = FxHashMap::default();

    add_item(&mut values, &vanilla_items::LAVA_BUCKET, BASE_UNIT * 100);
    add_item(&mut values, &vanilla_items::COAL_BLOCK, BASE_UNIT * 8 * 10);
    add_item(&mut values, &vanilla_items::BLAZE_ROD, BASE_UNIT * 12);
    add_item(&mut values, &vanilla_items::COAL, BASE_UNIT * 8);
    add_item(&mut values, &vanilla_items::CHARCOAL, BASE_UNIT * 8);
    add_tag(&mut values, &ItemTag::LOGS, BASE_UNIT * 3 / 2);
    add_tag(&mut values, &ItemTag::BAMBOO_BLOCKS, BASE_UNIT * 3 / 2);
    add_tag(&mut values, &ItemTag::PLANKS, BASE_UNIT * 3 / 2);
    add_item(
        &mut values,
        &vanilla_items::BAMBOO_MOSAIC,
        BASE_UNIT * 3 / 2,
    );
    add_tag(&mut values, &ItemTag::WOODEN_STAIRS, BASE_UNIT * 3 / 2);
    add_item(
        &mut values,
        &vanilla_items::BAMBOO_MOSAIC_STAIRS,
        BASE_UNIT * 3 / 2,
    );
    add_tag(&mut values, &ItemTag::WOODEN_SLABS, BASE_UNIT * 3 / 4);
    add_item(
        &mut values,
        &vanilla_items::BAMBOO_MOSAIC_SLAB,
        BASE_UNIT * 3 / 4,
    );
    add_tag(&mut values, &ItemTag::WOODEN_TRAPDOORS, BASE_UNIT * 3 / 2);
    add_tag(
        &mut values,
        &ItemTag::WOODEN_PRESSURE_PLATES,
        BASE_UNIT * 3 / 2,
    );
    add_tag(&mut values, &ItemTag::WOODEN_SHELVES, BASE_UNIT * 3 / 2);
    add_tag(&mut values, &ItemTag::WOODEN_FENCES, BASE_UNIT * 3 / 2);
    add_tag(&mut values, &ItemTag::FENCE_GATES, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::NOTE_BLOCK, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::BOOKSHELF, BASE_UNIT * 3 / 2);
    add_item(
        &mut values,
        &vanilla_items::CHISELED_BOOKSHELF,
        BASE_UNIT * 3 / 2,
    );
    add_item(&mut values, &vanilla_items::LECTERN, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::JUKEBOX, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::CHEST, BASE_UNIT * 3 / 2);
    add_item(
        &mut values,
        &vanilla_items::TRAPPED_CHEST,
        BASE_UNIT * 3 / 2,
    );
    add_item(
        &mut values,
        &vanilla_items::CRAFTING_TABLE,
        BASE_UNIT * 3 / 2,
    );
    add_item(
        &mut values,
        &vanilla_items::DAYLIGHT_DETECTOR,
        BASE_UNIT * 3 / 2,
    );
    add_tag(&mut values, &ItemTag::BANNERS, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::BOW, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::FISHING_ROD, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::LADDER, BASE_UNIT * 3 / 2);
    add_tag(&mut values, &ItemTag::SIGNS, BASE_UNIT);
    add_tag(&mut values, &ItemTag::HANGING_SIGNS, BASE_UNIT * 4);
    add_item(&mut values, &vanilla_items::WOODEN_SHOVEL, BASE_UNIT);
    add_item(&mut values, &vanilla_items::WOODEN_SWORD, BASE_UNIT);
    add_item(&mut values, &vanilla_items::WOODEN_SPEAR, BASE_UNIT);
    add_item(&mut values, &vanilla_items::WOODEN_HOE, BASE_UNIT);
    add_item(&mut values, &vanilla_items::WOODEN_AXE, BASE_UNIT);
    add_item(&mut values, &vanilla_items::WOODEN_PICKAXE, BASE_UNIT);
    add_tag(&mut values, &ItemTag::WOODEN_DOORS, BASE_UNIT);
    add_tag(&mut values, &ItemTag::BOATS, BASE_UNIT * 6);
    add_tag(&mut values, &ItemTag::WOOL, BASE_UNIT / 2);
    add_tag(&mut values, &ItemTag::WOODEN_BUTTONS, BASE_UNIT / 2);
    add_item(&mut values, &vanilla_items::STICK, BASE_UNIT / 2);
    add_tag(&mut values, &ItemTag::SAPLINGS, BASE_UNIT / 2);
    add_item(&mut values, &vanilla_items::BOWL, BASE_UNIT / 2);
    add_tag(&mut values, &ItemTag::WOOL_CARPETS, 1 + BASE_UNIT / 3);
    add_item(
        &mut values,
        &vanilla_items::DRIED_KELP_BLOCK,
        1 + BASE_UNIT * 20,
    );
    add_item(&mut values, &vanilla_items::CROSSBOW, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::BAMBOO, BASE_UNIT / 4);
    add_item(&mut values, &vanilla_items::DEAD_BUSH, BASE_UNIT / 2);
    add_item(&mut values, &vanilla_items::SHORT_DRY_GRASS, BASE_UNIT / 2);
    add_item(&mut values, &vanilla_items::TALL_DRY_GRASS, BASE_UNIT / 2);
    add_item(&mut values, &vanilla_items::SCAFFOLDING, BASE_UNIT / 4);
    add_item(&mut values, &vanilla_items::LOOM, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::BARREL, BASE_UNIT * 3 / 2);
    add_item(
        &mut values,
        &vanilla_items::CARTOGRAPHY_TABLE,
        BASE_UNIT * 3 / 2,
    );
    add_item(
        &mut values,
        &vanilla_items::FLETCHING_TABLE,
        BASE_UNIT * 3 / 2,
    );
    add_item(
        &mut values,
        &vanilla_items::SMITHING_TABLE,
        BASE_UNIT * 3 / 2,
    );
    add_item(&mut values, &vanilla_items::COMPOSTER, BASE_UNIT * 3 / 2);
    add_item(&mut values, &vanilla_items::AZALEA, BASE_UNIT / 2);
    add_item(&mut values, &vanilla_items::FLOWERING_AZALEA, BASE_UNIT / 2);
    add_item(
        &mut values,
        &vanilla_items::MANGROVE_ROOTS,
        BASE_UNIT * 3 / 2,
    );
    add_item(&mut values, &vanilla_items::LEAF_LITTER, BASE_UNIT / 2);

    remove_tag(&mut values, &ItemTag::NON_FLAMMABLE_WOOD);

    values
}

/// Inserts one item, replacing any value a tag already assigned to it.
fn add_item(values: &mut FxHashMap<Identifier, i32>, item: ItemRef, time: i32) {
    values.insert(item.key.clone(), time);
}

/// Inserts every item carrying `tag`.
fn add_tag(values: &mut FxHashMap<Identifier, i32>, tag: &Identifier, time: i32) {
    for item in REGISTRY.items.iter_tag(tag) {
        values.insert(item.key.clone(), time);
    }
}

/// Drops every item carrying `tag`, whatever assigned it.
fn remove_tag(values: &mut FxHashMap<Identifier, i32>, tag: &Identifier) {
    for item in REGISTRY.items.iter_tag(tag) {
        values.remove(&item.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_vanilla_registry;

    #[test]
    fn direct_entries_match_vanilla_burn_times() {
        init_vanilla_registry();
        assert_eq!(burn_duration(&ItemStack::new(&vanilla_items::COAL)), 1600);
        assert_eq!(
            burn_duration(&ItemStack::new(&vanilla_items::LAVA_BUCKET)),
            20_000
        );
        assert_eq!(
            burn_duration(&ItemStack::new(&vanilla_items::COAL_BLOCK)),
            16_000
        );
        assert_eq!(burn_duration(&ItemStack::new(&vanilla_items::STICK)), 100);
    }

    #[test]
    fn integer_division_matches_java() {
        init_vanilla_registry();
        // Java computes `1 + baseUnit / 3` with integer division: 1 + 66, not 1 + 66.67.
        assert_eq!(
            burn_duration(&ItemStack::new(&vanilla_items::WHITE_CARPET)),
            67
        );
        // `baseUnit * 3 / 4` is 150, not 200 * 0.75 rounded some other way.
        assert_eq!(
            burn_duration(&ItemStack::new(&vanilla_items::OAK_SLAB)),
            150
        );
    }

    #[test]
    fn non_flammable_wood_is_removed_after_the_tags_that_added_it() {
        init_vanilla_registry();
        // Crimson and warped planks are in PLANKS, which grants 300, but the final
        // NON_FLAMMABLE_WOOD removal takes them back out.
        assert!(!is_fuel(&ItemStack::new(&vanilla_items::CRIMSON_PLANKS)));
        assert!(!is_fuel(&ItemStack::new(&vanilla_items::WARPED_PLANKS)));
        assert!(is_fuel(&ItemStack::new(&vanilla_items::OAK_PLANKS)));
    }

    #[test]
    fn non_fuel_items_report_zero() {
        init_vanilla_registry();
        assert!(!is_fuel(&ItemStack::new(&vanilla_items::STONE)));
        assert_eq!(burn_duration(&ItemStack::new(&vanilla_items::STONE)), 0);
        assert_eq!(burn_duration(&ItemStack::empty()), 0);
    }
}
