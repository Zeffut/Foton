use super::*;
use foton_registry::{
    data_components::vanilla_components::{DAMAGE, GLIDER},
    vanilla_items,
};

pub(crate) fn check(env: &mut JNIEnv<'_>) {
    let mut ingredient = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    ingredient.set(GLIDER, ());
    ingredient.remove(DAMAGE);
    let mut output = ItemStack::new(&vanilla_items::DIAMOND_CHESTPLATE);
    output.set(GLIDER, ());
    output.remove(DAMAGE);
    let mut offer = MerchantOffer::with_uses(
        ItemCost::try_from_ingredient(ingredient.clone()).expect("cost"),
        Some(ItemCost::new(&vanilla_items::EMERALD, 3)),
        output.clone(),
        4,
        17,
        9,
        0.125,
        -3,
    );
    offer.set_special_price_diff(-2);
    offer.set_reward_exp(false);
    let source: MerchantOffers = vec![offer].into();
    let outputs = transfers(env, capture(&source).expect("borrowed offer snapshots"))
        .expect("owning recipe results");
    let value = env
        .get_object_array_element(&outputs, 0)
        .expect("typed offer transfer");
    let recipe = env
        .call_method(
            &value,
            "recipe",
            "(Ljava/lang/String;I)Lorg/bukkit/inventory/MerchantRecipe;",
            &[JValue::Object(&JObject::null()), JValue::Int(0)],
        )
        .expect("hydrate offer")
        .l()
        .expect("recipe");
    let input = env
        .new_object(
            "foton/item/MerchantOfferMutation",
            "(Lorg/bukkit/inventory/MerchantRecipe;)V",
            &[JValue::Object(&recipe)],
        )
        .expect("owning recipe mutation");
    let inputs = env
        .new_object_array(1, "foton/item/MerchantOfferMutation", &input)
        .expect("offer batch");
    let batch = prepare(env, &inputs).expect("complete batch admission");
    assert_eq!(batch.offers[0].result(), &output);
    assert_eq!(batch.offers[0].item_cost_a().cost_stack(), &ingredient);
    assert_eq!(
        batch.offers[0], source[0],
        "all item and scalar state must round-trip"
    );
    let invalid = env
        .new_object_array(2, "foton/item/MerchantOfferMutation", JObject::null())
        .expect("invalid second recipe");
    env.set_object_array_element(&invalid, 0, input)
        .expect("valid first recipe");
    assert!(
        prepare(env, &invalid).is_err(),
        "invalid trailing recipe must reject whole batch"
    );
    assert_eq!(
        source[0].result(),
        &output,
        "staging leaves source unmodified"
    );

    assert!(
        env.call_method(
            &recipe,
            "addIngredient",
            "(Lorg/bukkit/inventory/ItemStack;)V",
            &[JValue::Object(&JObject::null())]
        )
        .is_err()
    );
    let error = env.exception_occurred().expect("third ingredient refusal");
    env.exception_clear().expect("clear third refusal");
    assert!(
        env.is_instance_of(error, "java/lang/IllegalStateException")
            .expect("third category")
    );
}
