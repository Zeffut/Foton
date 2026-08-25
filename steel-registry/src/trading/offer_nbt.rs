//! Saving and loading a merchant's offers.
//!
//! Vanilla parity: `MerchantOffer.CODEC` and `MerchantOffers.CODEC`. This is
//! what stops a server restart from rerolling every villager in the world --
//! without it, a player could reroll a librarian by bouncing the server.

use simdnbt::FromNbtTag as _;
use simdnbt::borrow::NbtList as BorrowedNbtList;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_utils::Identifier;

use crate::REGISTRY;
use crate::data_component_predicate::DataComponentExactPredicate;
use crate::data_components::{ComponentData, ComponentPatchEntry, DataComponentPatch};
use crate::item_stack::ItemStack;
use crate::registry::RegistryExt as _;
use crate::trading::{ItemCost, MerchantOffer, MerchantOffers};

/// Vanilla parity: the `maxUses` default of `MerchantOffer.CODEC`.
const DEFAULT_MAX_USES: i32 = 4;
/// Vanilla parity: the `xp` default of `MerchantOffer.CODEC`.
const DEFAULT_XP: i32 = 1;

/// Writes one price the way `ItemCost.CODEC` does.
fn save_cost(cost: &ItemCost) -> NbtCompound {
    let mut compound = NbtCompound::new();
    compound.insert("id", cost.item().key.to_string());
    compound.insert("count", cost.count());
    // Vanilla's `DataComponentExactPredicate` serializes as a component map,
    // which is the same shape a patch's set entries take.
    let patch = cost.components().as_patch();
    if !patch.is_empty() {
        compound.insert("components", patch.to_nbt_tag_ref());
    }
    compound
}

/// Reads one price back, rebuilding its component requirement from the patch.
fn load_cost(compound: &simdnbt::borrow::NbtCompound<'_, '_>) -> Option<ItemCost> {
    let key: Identifier = compound.string("id")?.to_str().parse().ok()?;
    let item = REGISTRY.items.by_key(&key)?;
    // Vanilla's `optionalAlwaysPresentFieldOf(POSITIVE_INT, "count", 1)`.
    let count = compound.int("count").unwrap_or(1);

    let Some(patch) = compound
        .get("components")
        .and_then(DataComponentPatch::from_nbt_tag)
    else {
        return Some(ItemCost::new(item, count));
    };

    let values: Vec<(_, ComponentData)> = patch
        .iter()
        .filter_map(|(key, entry)| match entry {
            ComponentPatchEntry::Set(data) => {
                Some((REGISTRY.data_components.by_key(key)?, data.clone()))
            }
            // A predicate cannot demand a component's absence, so a removal
            // entry has no meaning here and is dropped the way vanilla's codec
            // would never have written one.
            ComponentPatchEntry::Removed => None,
        })
        .collect();

    Some(DataComponentExactPredicate::new(values).map_or_else(
        || ItemCost::new(item, count),
        |components| ItemCost::with_components(item, count, components),
    ))
}

/// Writes one offer the way `MerchantOffer.CODEC` does.
fn save_offer(offer: &MerchantOffer) -> NbtCompound {
    let mut compound = NbtCompound::new();
    compound.insert("buy", save_cost(offer.item_cost_a()));
    if let Some(cost_b) = offer.item_cost_b() {
        compound.insert("buyB", save_cost(cost_b));
    }
    compound.insert("sell", offer.result().to_nbt_tag_ref());
    compound.insert("uses", offer.uses());
    compound.insert("maxUses", offer.max_uses());
    compound.insert("rewardExp", offer.should_reward_exp());
    compound.insert("specialPrice", offer.special_price_diff());
    compound.insert("demand", offer.demand());
    compound.insert("priceMultiplier", offer.price_multiplier());
    compound.insert("xp", offer.xp());
    compound
}

/// Reads one offer back.
fn load_offer(compound: &simdnbt::borrow::NbtCompound<'_, '_>) -> Option<MerchantOffer> {
    let base_cost_a = load_cost(&compound.compound("buy")?)?;
    let cost_b = compound.compound("buyB").as_ref().and_then(load_cost);
    let result = ItemStack::from_borrowed_compound(&compound.compound("sell")?)?;

    let mut offer = MerchantOffer::with_uses(
        base_cost_a,
        cost_b,
        result,
        compound.int("uses").unwrap_or(0),
        compound.int("maxUses").unwrap_or(DEFAULT_MAX_USES),
        compound.int("xp").unwrap_or(DEFAULT_XP),
        compound.float("priceMultiplier").unwrap_or(0.0),
        compound.int("demand").unwrap_or(0),
    );
    offer.set_special_price_diff(compound.int("specialPrice").unwrap_or(0));
    Some(offer)
}

/// Writes a merchant's whole offer list.
///
/// Vanilla parity: `MerchantOffers.CODEC`.
#[must_use]
pub fn save(offers: &MerchantOffers) -> NbtTag {
    NbtTag::List(NbtList::from(
        offers.iter().map(save_offer).collect::<Vec<_>>(),
    ))
}

/// Reads a merchant's offer list back, skipping any entry that no longer loads.
///
/// An offer naming an item this build does not know is dropped rather than
/// failing the whole villager: the alternative is a villager that will not load
/// at all because one of its trades referenced something removed.
#[must_use]
pub fn load(list: &BorrowedNbtList<'_, '_>) -> MerchantOffers {
    let Some(compounds) = list.compounds() else {
        return MerchantOffers::new();
    };
    compounds
        .into_iter()
        .filter_map(|c| load_offer(&c))
        .collect()
}
