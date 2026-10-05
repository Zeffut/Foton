use super::*;
use foton_registry::{
    data_components::vanilla_components::{DAMAGE, GLIDER},
    vanilla_items,
};

pub(crate) fn check(env: &mut JNIEnv<'_>) {
    check_custom_single_cost_live_writeback(env);
    let (source, ingredient, output) = example_offer();
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
    env.set_object_array_element(&invalid, 0, &input)
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

    // JNI admission validates independently of the Java DTO constructor.
    let malformed = env
        .new_object(
            "foton/item/MerchantOfferMutation",
            "(Lorg/bukkit/inventory/MerchantRecipe;)V",
            &[JValue::Object(&recipe)],
        )
        .expect("second owning offer");
    let empty = env
        .call_static_method(
            "foton/item/ItemMutation",
            "empty",
            "()Lfoton/item/ItemMutation;",
            &[],
        )
        .expect("empty result mutation")
        .l()
        .expect("mutation");
    env.set_field(
        &malformed,
        "result",
        "Lfoton/item/ItemMutation;",
        JValue::Object(&empty),
    )
    .expect("malformed native ingress fixture");
    env.set_object_array_element(&invalid, 1, malformed)
        .expect("invalid trailing result");
    let mut destination = source.clone();
    let refused = match prepare(env, &invalid) {
        Ok(batch) => {
            destination = batch.offers;
            false
        }
        Err(ItemBridgeError::InvalidEdit("empty merchant result")) => true,
        Err(error) => panic!("wrong empty-result error: {error:?}"),
    };
    assert!(refused, "whole mixed batch must reject empty result");
    assert_eq!(
        destination, source,
        "invalid result leaves destination unchanged"
    );
    check_invalid_source_and_extra_ingredient(env, &recipe);
}

fn check_invalid_source_and_extra_ingredient(env: &mut JNIEnv<'_>, recipe: &JObject<'_>) {
    let invalid_source: MerchantOffers = vec![MerchantOffer::with_uses(
        ItemCost::new(&vanilla_items::EMERALD, 1),
        None,
        ItemStack::empty(),
        0,
        8,
        0,
        0.0,
        0,
    )]
    .into();
    assert!(
        matches!(
            capture(&invalid_source),
            Err(ItemBridgeError::NativeState(_))
        ),
        "preexisting invalid offers refuse explicitly, never emit unreadable transfers"
    );

    assert!(
        env.call_method(
            recipe,
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

fn check_custom_single_cost_live_writeback(env: &mut JNIEnv<'_>) {
    let source = vec![MerchantOffer::with_uses(
        ItemCost::new(&vanilla_items::EMERALD, 2),
        None,
        ItemStack::new(&vanilla_items::DIAMOND),
        0,
        8,
        3,
        0.0,
        0,
    )]
    .into();
    let encoded = transfers(env, capture(&source).expect("single cost capture")).expect("transfer");
    let transfer = env.get_object_array_element(&encoded, 0).expect("offer");
    let recipe = env
        .call_method(
            &transfer,
            "recipe",
            "(Ljava/lang/String;I)Lorg/bukkit/inventory/MerchantRecipe;",
            &[JValue::Object(&JObject::null()), JValue::Int(-1)],
        )
        .expect("null second ingredient hydration")
        .l()
        .expect("recipe");
    let merchant = env
        .new_object(
            "foton/FotonMerchant",
            "(Lnet/kyori/adventure/text/Component;)V",
            &[JValue::Object(&JObject::null())],
        )
        .expect("custom merchant");
    let recipes = env
        .call_static_method(
            "java/util/Collections",
            "singletonList",
            "(Ljava/lang/Object;)Ljava/util/List;",
            &[JValue::Object(&recipe)],
        )
        .expect("recipe list")
        .l()
        .expect("list");
    env.call_method(
        &merchant,
        "setRecipes",
        "(Ljava/util/List;)V",
        &[JValue::Object(&recipes)],
    )
    .expect("install owning offer");
    let live = read_recipe(env, &merchant);
    env.call_method(&live, "setUses", "(I)V", &[JValue::Int(5)])
        .expect("owning live writeback");
    let reread = read_recipe(env, &merchant);
    assert_eq!(
        env.call_method(&reread, "getUses", "()I", &[])
            .expect("read uses")
            .i()
            .expect("uses"),
        5
    );
    let ingredients = env
        .call_method(&reread, "getIngredients", "()Ljava/util/List;", &[])
        .expect("ingredients")
        .l()
        .expect("list");
    assert_eq!(
        env.call_method(&ingredients, "size", "()I", &[])
            .expect("single cost")
            .i()
            .expect("size"),
        1
    );
    let mutation = env
        .new_object(
            "foton/item/MerchantOfferMutation",
            "(Lorg/bukkit/inventory/MerchantRecipe;)V",
            &[JValue::Object(&reread)],
        )
        .expect("owning reread");
    let mutations = env
        .new_object_array(1, "foton/item/MerchantOfferMutation", mutation)
        .expect("array");
    let result = prepare(env, &mutations).expect("canonical offers");
    let mut expected = source;
    expected[0].set_uses(5);
    assert_eq!(result.offers, expected);
}

fn read_recipe<'local>(env: &mut JNIEnv<'local>, merchant: &JObject<'_>) -> JObject<'local> {
    let recipes = env
        .call_method(merchant, "getRecipes", "()Ljava/util/List;", &[])
        .expect("custom offers")
        .l()
        .expect("list");
    env.call_method(&recipes, "get", "(I)Ljava/lang/Object;", &[JValue::Int(0)])
        .expect("offer read")
        .l()
        .expect("offer")
}

fn example_offer() -> (MerchantOffers, ItemStack, ItemStack) {
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
    (source, ingredient, output)
}
