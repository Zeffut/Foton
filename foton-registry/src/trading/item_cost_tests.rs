use super::*;
use crate::{
    data_components::{
        DataComponentPatch,
        vanilla_components::{ADDITIONAL_TRADE_COST, CUSTOM_NAME, DAMAGE, GLIDER},
    },
    init_vanilla_registry, vanilla_items,
};
use text_components::TextComponent;

#[test]
fn ingredient_predicate_uses_positive_patch_but_keeps_original_display() {
    init_vanilla_registry();
    let mut ingredient = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    ingredient.set(GLIDER, ());
    ingredient.remove(DAMAGE);
    let cost = ItemCost::try_from_ingredient(ingredient.clone()).expect("valid cost");
    assert_eq!(cost.cost_stack(), &ingredient);
    assert!(!cost.cost_stack().has(DAMAGE));
    let mut payment = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    assert!(!cost.test(&payment));
    payment.set(GLIDER, ());
    payment.set(CUSTOM_NAME, TextComponent::plain("extra is allowed"));
    assert!(
        cost.test(&payment),
        "removed ingredient DAMAGE is not an absence requirement"
    );

    let mut bytes = Vec::new();
    cost.write(&mut bytes).expect("cost stream");
    let decoded =
        ItemCost::read(&mut Cursor::new(bytes.as_slice())).expect("derived stream display");
    assert!(
        decoded.cost_stack().has(DAMAGE),
        "codec reconstructs positive predicate, not removed display patch"
    );
    assert!(decoded.cost_stack().has(GLIDER));
    assert_eq!(decoded.components(), cost.components());
}

#[test]
fn ingredient_defaults_transient_values_and_invalid_values_are_distinct() {
    init_vanilla_registry();
    let plain = ItemCost::try_from_ingredient(ItemStack::new(&vanilla_items::IRON_CHESTPLATE))
        .expect("plain cost");
    assert!(
        plain.components().is_empty(),
        "prototype values are not ingredient constraints"
    );
    let mut patch = DataComponentPatch::new();
    patch.set(DAMAGE, 0);
    let explicit = ItemStack::with_count_and_patch(&vanilla_items::IRON_CHESTPLATE, 1, patch);
    let cost = ItemCost::try_from_ingredient(explicit).expect("explicit default cost");
    assert!(
        cost.components().is_empty(),
        "stack construction sanitizes prototype-equal positive entries"
    );
    let predicate = DataComponentExactPredicate::new(vec![(
        REGISTRY
            .data_components
            .by_key(DAMAGE.key())
            .expect("damage registered"),
        crate::data_components::ComponentData::new(0_i32),
    )])
    .expect("default predicate");
    let cost = ItemCost::with_components(&vanilla_items::IRON_CHESTPLATE, 1, predicate);
    assert!(!cost.components().is_empty());
    assert!(
        cost.test(&ItemStack::new(&vanilla_items::IRON_CHESTPLATE)),
        "effective prototype satisfies explicit predicate value"
    );

    let mut transient = ItemStack::new(&vanilla_items::STONE);
    transient.set(ADDITIONAL_TRADE_COST, 12);
    let cost = ItemCost::try_from_ingredient(transient.clone()).expect("typed transient live cost");
    assert!(cost.test(&transient));
    assert!(!cost.test(&ItemStack::new(&vanilla_items::STONE)));
    let mut bytes = Vec::new();
    cost.write(&mut bytes).expect("transient stream");
    assert!(
        ItemCost::read(&mut Cursor::new(bytes.as_slice()))
            .expect("transient stream cost")
            .test(&transient)
    );
    assert!(ItemCost::try_from_ingredient(ItemStack::empty()).is_none());
    let mut invalid = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    invalid.set(DAMAGE, -1);
    assert!(
        ItemCost::try_from_ingredient(invalid).is_none(),
        "invalid predicate is not EMPTY fallback"
    );
}
