use super::components::{java_item, native_item};
use foton_registry::{
    data_components::{
        components::ItemAttributeModifiers,
        vanilla_components::{ATTRIBUTE_MODIFIERS, GLIDER},
    },
    item_stack::ItemStack,
    vanilla_items,
};
use jni::{
    JNIEnv,
    objects::{JByteArray, JValue},
};

/// A stack the legacy byte layout cannot hold (`ZeldaCiv`'s wings carry native
/// components) serializes as the server's item NBT and comes back identical.
pub(super) fn check(env: &mut JNIEnv<'_>) {
    let mut wings = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    wings.set(GLIDER, ());
    wings.set(ATTRIBUTE_MODIFIERS, ItemAttributeModifiers::empty());
    let item = java_item(env, &wings);

    let bytes = JByteArray::from(
        env.call_method(&item, "serializeAsBytes", "()[B", &[])
            .expect("a native stack serializes instead of throwing")
            .l()
            .expect("bytes"),
    );
    let encoded = env.convert_byte_array(&bytes).expect("encoded bytes");
    assert_eq!(encoded.first(), Some(&0x0A), "a root compound tag");

    let restored = env
        .call_static_method(
            "org/bukkit/inventory/ItemStack",
            "deserializeBytes",
            "([B)Lorg/bukkit/inventory/ItemStack;",
            &[JValue::Object(&bytes)],
        )
        .expect("deserialize native stack")
        .l()
        .expect("stack");
    assert_eq!(native_item(env, &restored).stack, wings);
}
