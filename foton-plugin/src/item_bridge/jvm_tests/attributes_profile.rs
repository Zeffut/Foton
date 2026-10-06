//! Attribute modifiers and skull profiles must cross the bridge in both
//! directions: Java edits become native components, and native components
//! hydrate back into the meta.
use super::components::{java_item, native_item};
use foton_registry::{
    PlayerSkinPatch, ProfileProperty, ResolvableProfile, StoredGameProfile,
    attribute::AttributeModifierOperation,
    data_components::vanilla_components::{ATTRIBUTE_MODIFIERS, CUSTOM_NAME, PROFILE},
    equipment::EquipmentSlotGroup,
    item_stack::ItemStack,
    vanilla_items,
};
use jni::{
    JNIEnv,
    objects::{JObject, JValue},
};
use uuid::Uuid;

fn string<'local>(env: &mut JNIEnv<'local>, value: &str) -> JObject<'local> {
    JObject::from(env.new_string(value).expect("string"))
}

fn constant<'local>(env: &mut JNIEnv<'local>, class: &str, name: &str) -> JObject<'local> {
    env.get_static_field(class, name, format!("L{class};"))
        .expect("constant")
        .l()
        .expect("object")
}

fn meta<'local>(env: &mut JNIEnv<'local>, item: &JObject<'local>) -> JObject<'local> {
    env.call_method(
        item,
        "getItemMeta",
        "()Lorg/bukkit/inventory/meta/ItemMeta;",
        &[],
    )
    .expect("meta getter")
    .l()
    .expect("meta")
}

fn apply(env: &mut JNIEnv<'_>, item: &JObject<'_>, meta: &JObject<'_>) {
    let applied = env.call_method(
        item,
        "setItemMeta",
        "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
        &[JValue::Object(meta)],
    );
    if applied.is_err() {
        env.exception_describe().expect("mutation diagnostic");
    }
    assert!(applied.expect("set meta").z().expect("accepted"));
}

fn add_modifier(
    env: &mut JNIEnv<'_>,
    meta: &JObject<'_>,
    (attribute, key, amount, operation, slot): (&str, &str, f64, &str, &str),
) -> bool {
    let attribute = constant(env, "org/bukkit/attribute/Attribute", attribute);
    let operation = constant(
        env,
        "org/bukkit/attribute/AttributeModifier$Operation",
        operation,
    );
    let slot = constant(env, "org/bukkit/inventory/EquipmentSlotGroup", slot);
    let key = string(env, key);
    let key = env
        .call_static_method(
            "org/bukkit/NamespacedKey",
            "fromString",
            "(Ljava/lang/String;)Lorg/bukkit/NamespacedKey;",
            &[JValue::Object(&key)],
        )
        .expect("key")
        .l()
        .expect("key");
    let modifier = env
        .new_object(
            "org/bukkit/attribute/AttributeModifier",
            "(Lorg/bukkit/NamespacedKey;DLorg/bukkit/attribute/AttributeModifier$Operation;Lorg/bukkit/inventory/EquipmentSlotGroup;)V",
            &[
                JValue::Object(&key),
                JValue::Double(amount),
                JValue::Object(&operation),
                JValue::Object(&slot),
            ],
        )
        .expect("modifier");
    let added = env.call_method(
        meta,
        "addAttributeModifier",
        "(Lorg/bukkit/attribute/Attribute;Lorg/bukkit/attribute/AttributeModifier;)Z",
        &[JValue::Object(&attribute), JValue::Object(&modifier)],
    );
    if added.is_err() {
        let error = env.exception_occurred().expect("refusal");
        env.exception_clear().expect("clear refusal");
        assert!(
            env.is_instance_of(error, "java/lang/IllegalArgumentException")
                .expect("category")
        );
        return false;
    }
    added.expect("added").z().expect("accepted")
}

pub(super) fn check(env: &mut JNIEnv<'_>) {
    check_attributes(env);
    check_profile(env);
}

#[expect(
    clippy::too_many_lines,
    reason = "one scenario read top to bottom: build, write, hydrate, clear"
)]
fn check_attributes(env: &mut JNIEnv<'_>) {
    let item = java_item(env, &ItemStack::new(&vanilla_items::ELYTRA));
    let meta = meta(env, &item);
    assert!(add_modifier(
        env,
        &meta,
        ("ARMOR", "fixture:armor", 6.0, "ADD_NUMBER", "CHEST")
    ));
    assert!(add_modifier(
        env,
        &meta,
        (
            "ARMOR_TOUGHNESS",
            "fixture:tough",
            0.25,
            "ADD_SCALAR",
            "HEAD"
        )
    ));
    assert!(add_modifier(
        env,
        &meta,
        (
            "KNOCKBACK_RESISTANCE",
            "fixture:kb",
            -0.5,
            "MULTIPLY_SCALAR_1",
            "ANY"
        )
    ));
    assert!(
        !add_modifier(
            env,
            &meta,
            ("LUCK", "fixture:armor", 1.0, "ADD_NUMBER", "ANY")
        ),
        "a modifier key is unique across the meta"
    );
    apply(env, &item, &meta);

    let stack = native_item(env, &item).stack;
    let modifiers = stack.get(ATTRIBUTE_MODIFIERS).expect("component set");
    let seen: Vec<_> = modifiers
        .modifiers
        .iter()
        .map(|entry| {
            (
                entry.attribute.key.to_string(),
                entry.id.to_string(),
                entry.amount,
                entry.operation,
                entry.slot,
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (
                "minecraft:armor".to_owned(),
                "fixture:armor".to_owned(),
                6.0,
                AttributeModifierOperation::AddValue,
                EquipmentSlotGroup::Chest
            ),
            (
                "minecraft:armor_toughness".to_owned(),
                "fixture:tough".to_owned(),
                0.25,
                AttributeModifierOperation::AddMultipliedBase,
                EquipmentSlotGroup::Head
            ),
            (
                "minecraft:knockback_resistance".to_owned(),
                "fixture:kb".to_owned(),
                -0.5,
                AttributeModifierOperation::AddMultipliedTotal,
                EquipmentSlotGroup::Any
            ),
        ]
    );

    // Native state reads back into the meta, and clearing it restores the default.
    let again = java_item(env, &stack);
    let meta = self::meta(env, &again);
    assert!(
        env.call_method(&meta, "hasAttributeModifiers", "()Z", &[])
            .expect("has")
            .z()
            .expect("bool")
    );
    let all = env
        .call_method(
            &meta,
            "getAttributeModifiers",
            "()Lcom/google/common/collect/Multimap;",
            &[],
        )
        .expect("modifiers")
        .l()
        .expect("multimap");
    assert_eq!(
        env.call_method(&all, "size", "()I", &[])
            .expect("size")
            .i()
            .expect("int"),
        3
    );
    for name in ["ARMOR", "ARMOR_TOUGHNESS", "KNOCKBACK_RESISTANCE"] {
        let attribute = constant(env, "org/bukkit/attribute/Attribute", name);
        assert!(
            env.call_method(
                &meta,
                "removeAttributeModifier",
                "(Lorg/bukkit/attribute/Attribute;)Z",
                &[JValue::Object(&attribute)],
            )
            .expect("remove")
            .z()
            .expect("bool")
        );
    }
    apply(env, &again, &meta);
    assert_eq!(
        native_item(env, &again).stack.get(ATTRIBUTE_MODIFIERS),
        ItemStack::new(&vanilla_items::ELYTRA).get(ATTRIBUTE_MODIFIERS),
        "an empty list is the item's default, as Paper"
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "one scenario read top to bottom: hydrate, edit, replace, clear"
)]
fn check_profile(env: &mut JNIEnv<'_>) {
    let id = Uuid::from_u128(0x1234);
    let textures = || {
        ProfileProperty::new(
            "textures".to_owned(),
            "dGV4dHVyZXM=".to_owned(),
            Some("c2ln".to_owned()),
        )
        .expect("property")
    };
    let native_profile = ResolvableProfile::static_full(
        StoredGameProfile::new(id, "Link".to_owned(), vec![textures()]).expect("profile"),
        PlayerSkinPatch::default(),
    );
    let mut head = ItemStack::new(&vanilla_items::PLAYER_HEAD);
    head.set(PROFILE, native_profile.clone());
    let item = java_item(env, &head);
    let meta = meta(env, &item);
    let owner = env
        .call_method(&meta, "getOwner", "()Ljava/lang/String;", &[])
        .expect("owner")
        .l()
        .expect("name");
    let owner: String = env
        .get_string(&jni::objects::JString::from(owner))
        .expect("name")
        .into();
    assert_eq!(owner, "Link", "native profile hydrates into the meta");

    let name = string(env, "unrelated");
    env.call_method(
        &meta,
        "setDisplayName",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&name)],
    )
    .expect("name edit");
    apply(env, &item, &meta);
    let stack = native_item(env, &item).stack;
    assert!(stack.has(CUSTOM_NAME));
    assert_eq!(
        stack.get(PROFILE),
        Some(&native_profile),
        "untouched profile kept"
    );

    let replacement = Uuid::from_u128(0x5678);
    let java_id = string(env, &replacement.to_string());
    let java_id = env
        .call_static_method(
            "java/util/UUID",
            "fromString",
            "(Ljava/lang/String;)Ljava/util/UUID;",
            &[JValue::Object(&java_id)],
        )
        .expect("uuid")
        .l()
        .expect("uuid");
    let name = string(env, "Zelda");
    let profile = env
        .new_object(
            "foton/FotonPlayerProfile",
            "(Ljava/util/UUID;Ljava/lang/String;)V",
            &[JValue::Object(&java_id), JValue::Object(&name)],
        )
        .expect("profile");
    let (property_name, value, signature) = (
        string(env, "textures"),
        string(env, "dGV4dHVyZXM="),
        string(env, "c2ln"),
    );
    let property = env
        .new_object(
            "com/destroystokyo/paper/profile/ProfileProperty",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            &[
                JValue::Object(&property_name),
                JValue::Object(&value),
                JValue::Object(&signature),
            ],
        )
        .expect("property");
    env.call_method(
        &profile,
        "setProperty",
        "(Lcom/destroystokyo/paper/profile/ProfileProperty;)V",
        &[JValue::Object(&property)],
    )
    .expect("texture property");
    let meta = self::meta(env, &item);
    env.call_method(
        &meta,
        "setPlayerProfile",
        "(Lcom/destroystokyo/paper/profile/PlayerProfile;)V",
        &[JValue::Object(&profile)],
    )
    .expect("profile setter");
    apply(env, &item, &meta);
    let expected = ResolvableProfile::static_full(
        StoredGameProfile::new(replacement, "Zelda".to_owned(), vec![textures()]).expect("profile"),
        PlayerSkinPatch::default(),
    );
    assert_eq!(
        native_item(env, &item).stack.get(PROFILE),
        Some(&expected),
        "profile set from Java is the native component"
    );

    let meta = self::meta(env, &item);
    env.call_method(
        &meta,
        "setOwnerProfile",
        "(Lorg/bukkit/profile/PlayerProfile;)V",
        &[JValue::Object(&JObject::null())],
    )
    .expect("clear owner");
    apply(env, &item, &meta);
    assert!(native_item(env, &item).stack.get(PROFILE).is_none());
}
