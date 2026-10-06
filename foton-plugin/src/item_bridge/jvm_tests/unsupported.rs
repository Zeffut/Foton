use super::components::{java_item, native_item};
use foton_registry::{
    data_components::{
        components::{FireworkExplosion, Fireworks, ItemAttributeModifiers},
        vanilla_components::{ATTRIBUTE_MODIFIERS, FIREWORKS},
    },
    item_stack::ItemStack,
    vanilla_items,
};
use jni::{
    JNIEnv,
    objects::{JObject, JValue},
};

pub(super) fn check(env: &mut JNIEnv<'_>) {
    let mut rocket = ItemStack::new(&vanilla_items::FIREWORK_ROCKET);
    rocket.set(
        FIREWORKS,
        Fireworks::new(1, vec![FireworkExplosion::default()]).expect("fireworks"),
    );
    // An explicit empty list hides the type's defaults; Bukkit cannot see it, so
    // an unrelated edit has to carry it through untouched.
    let mut armor = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    armor.set(ATTRIBUTE_MODIFIERS, ItemAttributeModifiers::empty());
    for source in [&rocket, &armor] {
        let item = java_item(env, source);
        if source.item() == &*vanilla_items::FIREWORK_ROCKET {
            let donor = env
                .call_method(
                    &item,
                    "getItemMeta",
                    "()Lorg/bukkit/inventory/meta/ItemMeta;",
                    &[],
                )
                .expect("meta")
                .l()
                .expect("donor");
            env.call_method(&donor, "clearEffects", "()V", &[])
                .expect("clear hidden effects");
            assert!(
                env.call_method(
                    &item,
                    "setItemMeta",
                    "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
                    &[JValue::Object(&donor)]
                )
                .is_err(),
                "unsupported attempted removal must refuse"
            );
            let error = env.exception_occurred().expect("refusal");
            env.exception_clear().expect("clear refusal");
            assert!(
                env.is_instance_of(error, "java/lang/UnsupportedOperationException")
                    .expect("category")
            );
            assert_eq!(
                &native_item(env, &item).stack,
                source,
                "refused donor leaves recipient unchanged"
            );
        }
        let clean = env
            .call_method(
                &item,
                "getItemMeta",
                "()Lorg/bukkit/inventory/meta/ItemMeta;",
                &[],
            )
            .expect("clean donor")
            .l()
            .expect("meta");
        let name = JObject::from(env.new_string("unrelated").expect("name"));
        env.call_method(
            &clean,
            "setDisplayName",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&name)],
        )
        .expect("supported edit");
        env.call_method(
            &item,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(&clean)],
        )
        .expect("unrelated edit succeeds");
        let actual = native_item(env, &item).stack;
        assert_eq!(actual.get(FIREWORKS), source.get(FIREWORKS));
        assert_eq!(
            actual.get(ATTRIBUTE_MODIFIERS),
            source.get(ATTRIBUTE_MODIFIERS)
        );
    }
}
