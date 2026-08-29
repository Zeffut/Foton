//! Which potion a brewing stand turns another into.
//!
//! Vanilla parity: `PotionBrewing`. Vanilla assembles this table in code rather
//! than shipping it as data, so it is assembled in code here too, in the same
//! order and with the same shape: a list of containers a potion may sit in, a
//! list of container-to-container conversions, and a list of potion-to-potion
//! conversions.
//!
//! The one structural difference is `addStartMix`. Vanilla's builder expands
//! each start mix into two ordinary mixes as it runs; expanding them by hand
//! here would double every entry and invite a typo, so the start mixes keep
//! their own short table and the expansion happens where the mixes are read.

use std::sync::LazyLock;

use crate::data_components::vanilla_components::{POTION_CONTENTS, PotionContents};
use crate::item_stack::ItemStack;
use crate::items::ItemRef;
use crate::potion::PotionRef;
use crate::registry::reference::RegistryReference;
use crate::vanilla_item_tags::ItemTag;
use crate::{vanilla_items, vanilla_potions};

/// Ticks one brew takes.
///
/// Vanilla parity: the `400` assigned to `brewTime`, which is the twenty
/// seconds of `PotionBrewing.BREWING_TIME_SECONDS`.
pub const BREWING_TIME_TICKS: i32 = 400;

/// Brews one bottle of blaze powder is worth.
///
/// Vanilla parity: `BrewingStandBlockEntity.FUEL_USES`.
pub const FUEL_USES: i32 = 20;

/// One potion turning into another.
///
/// Vanilla parity: `PotionBrewing.Mix<Potion>`.
struct PotionMix {
    from: PotionRef,
    ingredient: ItemRef,
    to: PotionRef,
}

/// One kind of potion bottle turning into another.
///
/// Vanilla parity: `PotionBrewing.Mix<Item>`.
struct ContainerMix {
    from: ItemRef,
    ingredient: ItemRef,
    to: ItemRef,
}

/// The bottles a potion may be held in.
///
/// Vanilla parity: the three `addContainer` calls.
static CONTAINERS: LazyLock<Vec<ItemRef>> = LazyLock::new(|| {
    vec![
        &vanilla_items::POTION,
        &vanilla_items::SPLASH_POTION,
        &vanilla_items::LINGERING_POTION,
    ]
});

/// How one bottle becomes another, keeping whatever potion it holds.
///
/// Vanilla parity: the two `addContainerRecipe` calls.
static CONTAINER_MIXES: LazyLock<Vec<ContainerMix>> = LazyLock::new(|| {
    vec![
        ContainerMix {
            from: &vanilla_items::POTION,
            ingredient: &vanilla_items::GUNPOWDER,
            to: &vanilla_items::SPLASH_POTION,
        },
        ContainerMix {
            from: &vanilla_items::SPLASH_POTION,
            ingredient: &vanilla_items::DRAGON_BREATH,
            to: &vanilla_items::LINGERING_POTION,
        },
    ]
});

/// An ingredient that turns awkward potion into something, and water into
/// mundane.
///
/// Vanilla parity: the `addStartMix` calls. Each one stands for two mixes; see
/// [`for_each_potion_mix`].
static START_MIXES: LazyLock<Vec<(ItemRef, PotionRef)>> = LazyLock::new(|| {
    vec![
        (&vanilla_items::BREEZE_ROD, &vanilla_potions::WIND_CHARGED),
        (&vanilla_items::SLIME_BLOCK, &vanilla_potions::OOZING),
        (&vanilla_items::STONE, &vanilla_potions::INFESTED),
        (&vanilla_items::COBWEB, &vanilla_potions::WEAVING),
        (
            &vanilla_items::MAGMA_CREAM,
            &vanilla_potions::FIRE_RESISTANCE,
        ),
        (&vanilla_items::RABBIT_FOOT, &vanilla_potions::LEAPING),
        (&vanilla_items::SUGAR, &vanilla_potions::SWIFTNESS),
        (
            &vanilla_items::GLISTERING_MELON_SLICE,
            &vanilla_potions::HEALING,
        ),
        (&vanilla_items::SPIDER_EYE, &vanilla_potions::POISON),
        (&vanilla_items::GHAST_TEAR, &vanilla_potions::REGENERATION),
        (&vanilla_items::BLAZE_POWDER, &vanilla_potions::STRENGTH),
    ]
});

/// Every potion-to-potion conversion vanilla states outright.
///
/// Vanilla parity: the `addMix` calls, in their order.
static POTION_MIXES: LazyLock<Vec<PotionMix>> = LazyLock::new(|| {
    vec![
        mix(
            &vanilla_potions::WATER,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::THICK,
        ),
        mix(
            &vanilla_potions::WATER,
            &vanilla_items::REDSTONE,
            &vanilla_potions::MUNDANE,
        ),
        mix(
            &vanilla_potions::WATER,
            &vanilla_items::NETHER_WART,
            &vanilla_potions::AWKWARD,
        ),
        mix(
            &vanilla_potions::AWKWARD,
            &vanilla_items::GOLDEN_CARROT,
            &vanilla_potions::NIGHT_VISION,
        ),
        mix(
            &vanilla_potions::NIGHT_VISION,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_NIGHT_VISION,
        ),
        mix(
            &vanilla_potions::NIGHT_VISION,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::INVISIBILITY,
        ),
        mix(
            &vanilla_potions::LONG_NIGHT_VISION,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::LONG_INVISIBILITY,
        ),
        mix(
            &vanilla_potions::INVISIBILITY,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_INVISIBILITY,
        ),
        mix(
            &vanilla_potions::FIRE_RESISTANCE,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_FIRE_RESISTANCE,
        ),
        mix(
            &vanilla_potions::LEAPING,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_LEAPING,
        ),
        mix(
            &vanilla_potions::LEAPING,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_LEAPING,
        ),
        mix(
            &vanilla_potions::LEAPING,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::SLOWNESS,
        ),
        mix(
            &vanilla_potions::LONG_LEAPING,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::LONG_SLOWNESS,
        ),
        mix(
            &vanilla_potions::SLOWNESS,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_SLOWNESS,
        ),
        mix(
            &vanilla_potions::SLOWNESS,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_SLOWNESS,
        ),
        mix(
            &vanilla_potions::AWKWARD,
            &vanilla_items::TURTLE_HELMET,
            &vanilla_potions::TURTLE_MASTER,
        ),
        mix(
            &vanilla_potions::TURTLE_MASTER,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_TURTLE_MASTER,
        ),
        mix(
            &vanilla_potions::TURTLE_MASTER,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_TURTLE_MASTER,
        ),
        mix(
            &vanilla_potions::SWIFTNESS,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::SLOWNESS,
        ),
        mix(
            &vanilla_potions::LONG_SWIFTNESS,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::LONG_SLOWNESS,
        ),
        mix(
            &vanilla_potions::SWIFTNESS,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_SWIFTNESS,
        ),
        mix(
            &vanilla_potions::SWIFTNESS,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_SWIFTNESS,
        ),
        mix(
            &vanilla_potions::AWKWARD,
            &vanilla_items::PUFFERFISH,
            &vanilla_potions::WATER_BREATHING,
        ),
        mix(
            &vanilla_potions::WATER_BREATHING,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_WATER_BREATHING,
        ),
        mix(
            &vanilla_potions::HEALING,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_HEALING,
        ),
        mix(
            &vanilla_potions::HEALING,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::HARMING,
        ),
        mix(
            &vanilla_potions::STRONG_HEALING,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::STRONG_HARMING,
        ),
        mix(
            &vanilla_potions::HARMING,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_HARMING,
        ),
        mix(
            &vanilla_potions::POISON,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::HARMING,
        ),
        mix(
            &vanilla_potions::LONG_POISON,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::HARMING,
        ),
        mix(
            &vanilla_potions::STRONG_POISON,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::STRONG_HARMING,
        ),
        mix(
            &vanilla_potions::POISON,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_POISON,
        ),
        mix(
            &vanilla_potions::POISON,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_POISON,
        ),
        mix(
            &vanilla_potions::REGENERATION,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_REGENERATION,
        ),
        mix(
            &vanilla_potions::REGENERATION,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_REGENERATION,
        ),
        mix(
            &vanilla_potions::STRENGTH,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_STRENGTH,
        ),
        mix(
            &vanilla_potions::STRENGTH,
            &vanilla_items::GLOWSTONE_DUST,
            &vanilla_potions::STRONG_STRENGTH,
        ),
        mix(
            &vanilla_potions::WATER,
            &vanilla_items::FERMENTED_SPIDER_EYE,
            &vanilla_potions::WEAKNESS,
        ),
        mix(
            &vanilla_potions::WEAKNESS,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_WEAKNESS,
        ),
        mix(
            &vanilla_potions::AWKWARD,
            &vanilla_items::PHANTOM_MEMBRANE,
            &vanilla_potions::SLOW_FALLING,
        ),
        mix(
            &vanilla_potions::SLOW_FALLING,
            &vanilla_items::REDSTONE,
            &vanilla_potions::LONG_SLOW_FALLING,
        ),
    ]
});

const fn mix(from: PotionRef, ingredient: ItemRef, to: PotionRef) -> PotionMix {
    PotionMix {
        from,
        ingredient,
        to,
    }
}

/// Calls `visit` with every potion-to-potion conversion there is.
///
/// The stated mixes come first, then the two each start mix stands for: water
/// into mundane, and awkward into whatever that ingredient makes. No ingredient
/// appears on both sides of that split, so the order does not decide any
/// outcome; it only decides which entry is found first.
fn for_each_potion_mix(mut visit: impl FnMut(PotionRef, ItemRef, PotionRef) -> bool) {
    for entry in POTION_MIXES.iter() {
        if visit(entry.from, entry.ingredient, entry.to) {
            return;
        }
    }
    for (ingredient, potion) in START_MIXES.iter() {
        if visit(
            &vanilla_potions::WATER,
            ingredient,
            &vanilla_potions::MUNDANE,
        ) {
            return;
        }
        if visit(&vanilla_potions::AWKWARD, ingredient, potion) {
            return;
        }
    }
}

/// Returns whether this item does anything at all in the ingredient slot.
///
/// Vanilla parity: `PotionBrewing.isIngredient`.
#[must_use]
pub fn is_ingredient(ingredient: &ItemStack) -> bool {
    is_container_ingredient(ingredient) || is_potion_ingredient(ingredient)
}

/// Returns whether this item converts one bottle into another.
///
/// Vanilla parity: `PotionBrewing.isContainerIngredient`.
#[must_use]
pub fn is_container_ingredient(ingredient: &ItemStack) -> bool {
    CONTAINER_MIXES
        .iter()
        .any(|entry| ingredient.is(entry.ingredient))
}

/// Returns whether this item converts one potion into another.
///
/// Vanilla parity: `PotionBrewing.isPotionIngredient`.
#[must_use]
pub fn is_potion_ingredient(ingredient: &ItemStack) -> bool {
    let mut found = false;
    for_each_potion_mix(|_, mix_ingredient, _| {
        found = ingredient.is(mix_ingredient);
        found
    });
    found
}

/// Returns whether this item is a bottle a potion can sit in.
///
/// Vanilla parity: `PotionBrewing.isContainer`.
#[must_use]
pub fn is_container(input: &ItemStack) -> bool {
    CONTAINERS.iter().any(|container| input.is(container))
}

/// Returns whether this ingredient changes this bottle.
///
/// Vanilla parity: `PotionBrewing.hasMix`. A bottle that is not a potion
/// container is refused outright, which is what stops a brewing stand working
/// on an arbitrary item someone hoppered in.
#[must_use]
pub fn has_mix(source: &ItemStack, ingredient: &ItemStack) -> bool {
    is_container(source)
        && (has_container_mix(source, ingredient) || has_potion_mix(source, ingredient))
}

/// Returns whether this ingredient turns this bottle into another kind.
///
/// Vanilla parity: `PotionBrewing.hasContainerMix`.
#[must_use]
pub fn has_container_mix(source: &ItemStack, ingredient: &ItemStack) -> bool {
    CONTAINER_MIXES
        .iter()
        .any(|entry| source.is(entry.from) && ingredient.is(entry.ingredient))
}

/// Returns whether this ingredient turns this bottle's potion into another.
///
/// Vanilla parity: `PotionBrewing.hasPotionMix`.
#[must_use]
pub fn has_potion_mix(source: &ItemStack, ingredient: &ItemStack) -> bool {
    let Some(potion) = potion_of(source) else {
        return false;
    };

    let mut found = false;
    for_each_potion_mix(|from, mix_ingredient, _| {
        found = std::ptr::eq(from, potion) && ingredient.is(mix_ingredient);
        found
    });
    found
}

/// Returns what this bottle becomes when brewed with this ingredient.
///
/// Vanilla parity: `PotionBrewing.mix`. A bottle nothing applies to comes back
/// unchanged, which is how a stand with one matching bottle and two others
/// leaves the other two alone.
#[must_use]
pub fn mix_with(ingredient: &ItemStack, source: &ItemStack) -> ItemStack {
    if source.is_empty() {
        return source.clone();
    }
    let Some(potion) = potion_of(source) else {
        return source.clone();
    };

    for entry in CONTAINER_MIXES.iter() {
        if source.is(entry.from) && ingredient.is(entry.ingredient) {
            return potion_item(entry.to, potion);
        }
    }

    let mut result = None;
    for_each_potion_mix(|from, mix_ingredient, to| {
        if std::ptr::eq(from, potion) && ingredient.is(mix_ingredient) {
            result = Some(to);
            return true;
        }
        false
    });

    result.map_or_else(|| source.clone(), |to| potion_item(source.item(), to))
}

/// Returns whether this item can fuel a brewing stand.
///
/// Vanilla parity: the `ItemTags.BREWING_FUEL` test of
/// `BrewingStandBlockEntity.serverTick`.
#[must_use]
pub fn is_brewing_fuel(stack: &ItemStack) -> bool {
    stack.item().has_tag(&ItemTag::BREWING_FUEL)
}

/// Returns the potion a bottle holds, if it holds one.
fn potion_of(stack: &ItemStack) -> Option<PotionRef> {
    stack
        .get(POTION_CONTENTS)
        .and_then(PotionContents::potion)
        .map(|reference| reference.value())
}

/// Builds a bottle of `item` holding `potion`.
///
/// Vanilla parity: `PotionContents.createItemStack`.
#[must_use]
pub fn potion_item(item: ItemRef, potion: PotionRef) -> ItemStack {
    let mut stack = ItemStack::new(item);
    stack.set(
        POTION_CONTENTS,
        PotionContents::new(Some(RegistryReference::new(potion)), None, Vec::new(), None),
    );
    stack
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_vanilla_registry;

    fn water_bottle() -> ItemStack {
        potion_item(&vanilla_items::POTION, &vanilla_potions::WATER)
    }

    #[test]
    fn nether_wart_turns_water_into_awkward() {
        init_vanilla_registry();
        let result = mix_with(
            &ItemStack::new(&vanilla_items::NETHER_WART),
            &water_bottle(),
        );
        assert!(result.is(&vanilla_items::POTION));
        assert!(std::ptr::eq(
            potion_of(&result).expect("brewed bottle holds a potion"),
            &raw const vanilla_potions::AWKWARD
        ));
    }

    #[test]
    fn a_start_mix_needs_the_awkward_step_first() {
        init_vanilla_registry();
        // Sugar on water gives mundane, not swiftness. Getting swiftness out of
        // water in one step is the mistake this table exists to prevent.
        let from_water = mix_with(&ItemStack::new(&vanilla_items::SUGAR), &water_bottle());
        assert!(std::ptr::eq(
            potion_of(&from_water).expect("holds a potion"),
            &raw const vanilla_potions::MUNDANE
        ));

        let awkward = potion_item(&vanilla_items::POTION, &vanilla_potions::AWKWARD);
        let from_awkward = mix_with(&ItemStack::new(&vanilla_items::SUGAR), &awkward);
        assert!(std::ptr::eq(
            potion_of(&from_awkward).expect("holds a potion"),
            &raw const vanilla_potions::SWIFTNESS
        ));
    }

    #[test]
    fn gunpowder_changes_the_bottle_and_keeps_the_potion() {
        init_vanilla_registry();
        let swiftness = potion_item(&vanilla_items::POTION, &vanilla_potions::SWIFTNESS);
        let result = mix_with(&ItemStack::new(&vanilla_items::GUNPOWDER), &swiftness);

        assert!(result.is(&vanilla_items::SPLASH_POTION));
        assert!(std::ptr::eq(
            potion_of(&result).expect("holds a potion"),
            &raw const vanilla_potions::SWIFTNESS
        ));
    }

    #[test]
    fn redstone_lengthens_and_glowstone_strengthens() {
        init_vanilla_registry();
        let swiftness = potion_item(&vanilla_items::POTION, &vanilla_potions::SWIFTNESS);

        let long = mix_with(&ItemStack::new(&vanilla_items::REDSTONE), &swiftness);
        assert!(std::ptr::eq(
            potion_of(&long).expect("holds a potion"),
            &raw const vanilla_potions::LONG_SWIFTNESS
        ));

        let strong = mix_with(&ItemStack::new(&vanilla_items::GLOWSTONE_DUST), &swiftness);
        assert!(std::ptr::eq(
            potion_of(&strong).expect("holds a potion"),
            &raw const vanilla_potions::STRONG_SWIFTNESS
        ));
    }

    #[test]
    fn an_ingredient_nothing_applies_to_leaves_the_bottle_alone() {
        init_vanilla_registry();
        let awkward = potion_item(&vanilla_items::POTION, &vanilla_potions::AWKWARD);
        let result = mix_with(&ItemStack::new(&vanilla_items::DIRT), &awkward);
        assert!(std::ptr::eq(
            potion_of(&result).expect("holds a potion"),
            &raw const vanilla_potions::AWKWARD
        ));
    }

    #[test]
    fn only_a_potion_bottle_can_be_brewed() {
        init_vanilla_registry();
        let wart = ItemStack::new(&vanilla_items::NETHER_WART);
        assert!(has_mix(&water_bottle(), &wart));
        // A glass bottle sits in the same slot but holds nothing to convert.
        assert!(!has_mix(
            &ItemStack::new(&vanilla_items::GLASS_BOTTLE),
            &wart
        ));
    }

    #[test]
    fn blaze_powder_is_fuel_and_is_also_an_ingredient() {
        init_vanilla_registry();
        // It is both, which is why the stand keeps them in separate slots.
        let powder = ItemStack::new(&vanilla_items::BLAZE_POWDER);
        assert!(is_brewing_fuel(&powder));
        assert!(is_ingredient(&powder));
        assert!(!is_brewing_fuel(&ItemStack::new(&vanilla_items::REDSTONE)));
    }
}
