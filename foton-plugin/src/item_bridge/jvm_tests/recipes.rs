use foton_registry::REGISTRY;
use foton_registry::data_components::vanilla_components::CUSTOM_NAME;
use jni::JNIEnv;
use jni::objects::{JObject, JObjectArray, JValue};

/// A plugin recipe crafts the stack the plugin gave it, name and all.
pub(super) fn check(env: &mut JNIEnv<'_>) {
    let material = env
        .get_static_field("org/bukkit/Material", "STICK", "Lorg/bukkit/Material;")
        .expect("material")
        .l()
        .expect("material object");
    let stack = env
        .new_object(
            "org/bukkit/inventory/ItemStack",
            "(Lorg/bukkit/Material;I)V",
            &[JValue::Object(&material), JValue::Int(3)],
        )
        .expect("result stack");
    let meta = env
        .call_method(
            &stack,
            "getItemMeta",
            "()Lorg/bukkit/inventory/meta/ItemMeta;",
            &[],
        )
        .expect("meta")
        .l()
        .expect("meta object");
    let name = env.new_string("Vigorous radish").expect("name");
    env.call_method(
        &meta,
        "setDisplayName",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&name)],
    )
    .expect("name the result");
    env.call_method(
        &stack,
        "setItemMeta",
        "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
        &[JValue::Object(&meta)],
    )
    .expect("store meta");
    let mutation = env
        .call_method(&stack, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("mutation")
        .l()
        .expect("mutation object");

    let key = env.new_string("jvm_test:vigorous_radish").expect("key");
    let shape = strings(env, &["B ", "S "]);
    let ingredients = strings(env, &["B=minecraft:beetroot", "S=minecraft:sugar"]);
    let added = env
        .call_static_method(
            "foton/Native",
            "recipeAddShaped",
            "(Ljava/lang/String;Lfoton/item/ItemMutation;[Ljava/lang/String;[Ljava/lang/String;)Z",
            &[
                JValue::Object(&key),
                JValue::Object(&mutation),
                JValue::Object(&shape),
                JValue::Object(&ingredients),
            ],
        )
        .expect("add shaped recipe")
        .z()
        .expect("added");
    assert!(added, "the recipe registers");

    let recipe = REGISTRY
        .recipes
        .iter_crafting()
        .find(|recipe| recipe.id().to_string() == "jvm_test:vigorous_radish")
        .expect("registered recipe");
    let result = recipe.result().to_item_stack();
    assert_eq!(result.item().key.to_string(), "minecraft:stick");
    assert_eq!(result.count, 3);
    assert!(
        result.get(CUSTOM_NAME).is_some(),
        "the plugin's name is part of the crafted result"
    );
}

fn strings<'local>(env: &mut JNIEnv<'local>, values: &[&str]) -> JObjectArray<'local> {
    let array = env
        .new_object_array(
            i32::try_from(values.len()).expect("length"),
            "java/lang/String",
            JObject::null(),
        )
        .expect("array");
    for (index, value) in values.iter().enumerate() {
        let value = env.new_string(value).expect("string");
        env.set_object_array_element(&array, i32::try_from(index).expect("index"), value)
            .expect("element");
    }
    array
}
