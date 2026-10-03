use foton_registry::{
    data_components::{
        components::CustomModelData,
        vanilla_components::{CUSTOM_MODEL_DATA, DAMAGE, GLIDER},
    },
    item_stack::ItemStack,
    vanilla_items,
};
use jni::{
    JNIEnv,
    objects::{JObject, JValue},
};

use crate::item_bridge::{mutation, snapshot::Candidate, transfer};

pub(super) fn java_item<'local>(env: &mut JNIEnv<'local>, item: &ItemStack) -> JObject<'local> {
    let transfer = transfer::capture(env, item).expect("component fixture capture");
    env.call_static_method(
        "foton/FotonInventory",
        "decodeTransfer",
        "(Lfoton/item/ItemTransfer;)Lorg/bukkit/inventory/ItemStack;",
        &[JValue::Object(&transfer)],
    )
    .expect("component fixture decode")
    .l()
    .expect("item")
}

pub(super) fn native_item(env: &mut JNIEnv<'_>, item: &JObject<'_>) -> Candidate {
    let value = env
        .call_method(item, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("owning component mutation")
        .l()
        .expect("mutation");
    mutation::materialize(env, &value).expect("canonical component fixture")
}

fn has(env: &mut JNIEnv<'_>, item: &JObject<'_>, key: &JObject<'_>, method: &str) -> bool {
    env.call_method(
        item,
        method,
        "(Lio/papermc/paper/datacomponent/DataComponentType;)Z",
        &[JValue::Object(key)],
    )
    .expect("component presence query")
    .z()
    .expect("boolean")
}

pub(super) fn check(env: &mut JNIEnv<'_>) {
    check_meta_opaque_provenance(env);
    let damage = env
        .get_static_field(
            "io/papermc/paper/datacomponent/DataComponentTypes",
            "DAMAGE",
            "Lio/papermc/paper/datacomponent/DataComponentType$Valued;",
        )
        .expect("DAMAGE")
        .l()
        .expect("key");
    let stone = java_item(env, &ItemStack::new(&vanilla_items::STONE));
    assert!(
        !has(env, &stone, &damage, "hasData"),
        "stone has no prototype DAMAGE"
    );
    let mut source = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    source.set(GLIDER, ());
    source.set(
        CUSTOM_MODEL_DATA,
        CustomModelData::new(
            vec![1.25],
            vec![true],
            vec!["typed ✓".to_owned()],
            vec![0x123456],
        ),
    );
    let armor = java_item(env, &source);
    env.call_method(
        &armor,
        "setOpaqueNbt",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&JObject::null())],
    )
    .expect("unchanged item-only opaque state");
    let opaque = env.new_string("changed opaque state").expect("opaque edit");
    assert!(
        env.call_method(
            &armor,
            "setOpaqueNbt",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&opaque)]
        )
        .is_err()
    );
    let error = env.exception_occurred().expect("opaque edit refusal");
    env.exception_clear().expect("clear opaque refusal");
    assert!(
        env.is_instance_of(error, "java/lang/UnsupportedOperationException")
            .expect("opaque capability category")
    );
    assert_eq!(
        native_item(env, &armor).stack,
        source,
        "opaque edit refuses before changing the source"
    );
    assert!(has(env, &armor, &damage, "hasData"));
    assert!(!has(env, &armor, &damage, "isDataOverridden"));
    let model_key = env
        .get_static_field(
            "io/papermc/paper/datacomponent/DataComponentTypes",
            "CUSTOM_MODEL_DATA",
            "Lio/papermc/paper/datacomponent/DataComponentType$Valued;",
        )
        .expect("model key")
        .l()
        .expect("key");
    let model = env
        .call_method(
            &armor,
            "getData",
            "(Lio/papermc/paper/datacomponent/DataComponentType$Valued;)Ljava/lang/Object;",
            &[JValue::Object(&model_key)],
        )
        .expect("native model getter")
        .l()
        .expect("model");
    let strings = env
        .call_method(&model, "strings", "()Ljava/util/List;", &[])
        .expect("model strings")
        .l()
        .expect("list");
    let string = env
        .call_method(strings, "get", "(I)Ljava/lang/Object;", &[JValue::Int(0)])
        .expect("first string")
        .l()
        .expect("string");
    assert_eq!(
        env.get_string(&string.into())
            .expect("model UTF8")
            .to_str()
            .expect("valid UTF8"),
        "typed ✓"
    );

    let full_damage = env
        .new_object("java/lang/Integer", "(I)V", &[JValue::Int(70000)])
        .expect("full damage");
    env.call_method(
        &armor,
        "setData",
        "(Lio/papermc/paper/datacomponent/DataComponentType$Valued;Ljava/lang/Object;)V",
        &[JValue::Object(&damage), JValue::Object(&full_damage)],
    )
    .expect("full-width set");
    assert_eq!(native_item(env, &armor).stack.get(DAMAGE), Some(&70000));
    assert!(has(env, &armor, &damage, "isDataOverridden"));
    let short_damage = env
        .call_method(&armor, "getDurability", "()S", &[])
        .expect("legacy damage view")
        .s()
        .expect("short");
    assert_eq!(
        i32::from(short_damage),
        source.get_max_damage(),
        "legacy adapter follows native clamp; component stays full-width"
    );
    let invalid = env
        .new_object("java/lang/Integer", "(I)V", &[JValue::Int(-1)])
        .expect("invalid damage");
    assert!(
        env.call_method(
            &armor,
            "setData",
            "(Lio/papermc/paper/datacomponent/DataComponentType$Valued;Ljava/lang/Object;)V",
            &[JValue::Object(&damage), JValue::Object(&invalid)]
        )
        .is_err()
    );
    let error = env.exception_occurred().expect("invalid edit exception");
    env.exception_clear().expect("clear invalid edit");
    assert!(
        env.is_instance_of(error, "java/lang/IllegalArgumentException")
            .expect("invalid edit category")
    );
    assert_eq!(native_item(env, &armor).stack.get(DAMAGE), Some(&70000));

    for (operation, present, overridden) in [("unsetData", false, true), ("resetData", true, false)]
    {
        env.call_method(
            &armor,
            operation,
            "(Lio/papermc/paper/datacomponent/DataComponentType;)V",
            &[JValue::Object(&damage)],
        )
        .expect("component edit");
        assert_eq!(has(env, &armor, &damage, "hasData"), present);
        assert_eq!(has(env, &armor, &damage, "isDataOverridden"), overridden);
        let candidate = native_item(env, &armor);
        assert!(candidate.stack.has(GLIDER));
        assert_eq!(
            candidate.stack.get(CUSTOM_MODEL_DATA),
            source.get(CUSTOM_MODEL_DATA)
        );
    }
    let state = env
        .call_method(&armor, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("unknown key source")
        .l()
        .expect("mutation");
    let unknown = env.new_string("not_a_supported_key").expect("unknown key");
    assert!(
        env.call_static_method(
            "foton/Native",
            "editItemComponent",
            "(Lfoton/item/ItemMutation;Ljava/lang/String;Z)Lfoton/item/ItemTransfer;",
            &[
                JValue::Object(&state),
                JValue::Object(&unknown),
                JValue::Bool(0)
            ]
        )
        .is_err()
    );
    let error = env.exception_occurred().expect("unknown key refusal");
    env.exception_clear().expect("clear unknown key");
    assert!(
        env.is_instance_of(error, "java/lang/UnsupportedOperationException")
            .expect("unknown key category")
    );
    assert_eq!(native_item(env, &armor).stack, source);

    env.call_method(&armor, "setAmount", "(I)V", &[JValue::Int(0)])
        .expect("zero amount");
    assert!(!has(env, &armor, &damage, "hasData"));
    let clone = env
        .call_method(&armor, "clone", "()Lorg/bukkit/inventory/ItemStack;", &[])
        .expect("latent clone")
        .l()
        .expect("clone");
    env.call_method(&clone, "setAmount", "(I)V", &[JValue::Int(1)])
        .expect("restore latent amount");
    assert_eq!(native_item(env, &clone).stack, source);
    assert!(
        native_item(env, &armor).stack.is_empty(),
        "cloned amount change does not affect source"
    );
    let air = env
        .get_static_field("org/bukkit/Material", "AIR", "Lorg/bukkit/Material;")
        .expect("AIR")
        .l()
        .expect("material");
    env.call_method(
        &clone,
        "setType",
        "(Lorg/bukkit/Material;)V",
        &[JValue::Object(&air)],
    )
    .expect("destructive AIR transition");
    let iron = env
        .get_static_field(
            "org/bukkit/Material",
            "IRON_CHESTPLATE",
            "Lorg/bukkit/Material;",
        )
        .expect("iron")
        .l()
        .expect("material");
    env.call_method(
        &clone,
        "setType",
        "(Lorg/bukkit/Material;)V",
        &[JValue::Object(&iron)],
    )
    .expect("restore from AIR");
    assert_eq!(
        native_item(env, &clone).stack,
        ItemStack::new(&vanilla_items::IRON_CHESTPLATE)
    );
}

fn check_meta_opaque_provenance(env: &mut JNIEnv<'_>) {
    let mut recipient = ItemStack::new(&vanilla_items::STONE);
    recipient.set_opaque_nbt(Some("recipient-only extension".to_owned()));
    recipient.set(
        foton_registry::data_components::vanilla_components::CUSTOM_NAME,
        text_components::TextComponent::plain("recipient name"),
    );
    let mut donor = ItemStack::new(&vanilla_items::STONE);
    donor.set(GLIDER, ());
    donor.set_opaque_nbt(Some("donor-only extension".to_owned()));
    let donor_item = java_item(env, &donor);
    let donor_meta = env
        .call_method(
            &donor_item,
            "getItemMeta",
            "()Lorg/bukkit/inventory/meta/ItemMeta;",
            &[],
        )
        .expect("opaque donor metadata")
        .l()
        .expect("meta");
    let empty = env
        .new_object("org/bukkit/inventory/meta/SimpleItemMeta", "()V", &[])
        .expect("empty metadata");
    for (meta, has_glider) in [
        (&JObject::null(), false),
        (&empty, false),
        (&donor_meta, true),
    ] {
        let target = java_item(env, &recipient);
        assert!(
            env.call_method(
                &target,
                "setItemMeta",
                "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
                &[JValue::Object(meta)]
            )
            .expect("meta-only adoption")
            .z()
            .expect("accepted")
        );
        let changed = native_item(env, &target);
        assert_eq!(
            changed.stack.opaque_nbt(),
            recipient.opaque_nbt(),
            "meta must not adopt donor stack-only extension"
        );
        assert_eq!(changed.stack.has(GLIDER), has_glider);
        if !has_glider {
            assert!(changed.stack.patch().is_empty());
        }
    }
    assert_eq!(
        native_item(env, &donor_item).stack,
        donor,
        "donor remains independent"
    );
}
