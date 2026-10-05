//! Parent-union groups must survive actual hydration and donor conversion.
use super::components::{java_item, native_item};
use crate::natives as bridge_native_bridge;
use foton_registry::item_stack as bridge_item_stack;
use foton_registry::{
    data_components::vanilla_components::{DAMAGE, GLIDER, TOOLTIP_DISPLAY},
    vanilla_items,
};
use jni::{
    JNIEnv,
    objects::{JObject, JObjectArray, JValue},
};

pub(super) fn check(env: &mut JNIEnv<'_>) {
    check_rich_lore(env);
    check_empty_banner(env);
    for (item, fields) in [
        ("iron_chestplate", "trim=minecraft:gold,minecraft:wild"),
        (
            "white_banner",
            "pattern=minecraft:cross,red\u{1d}basecolor=blue",
        ),
        ("goat_horn", "instrument=minecraft:ponder_goat_horn"),
        (
            "potion",
            "basepotionhex=6d696e6563726166743a7761746572\u{1d}potioneffects=speed,20,1,true,false,false;",
        ),
        (
            "stone",
            "glint=true\u{1d}maxstack=16\u{1d}cooldown=2.5\u{1d}cooldowngroup=fixture:cooldown\u{1d}lorejsonhex=7b2274657874223a2272696368222c22626f6c64223a747275657d\u{1d}customhex=7b6f70617175653a317d",
        ),
    ] {
        check_family(env, item, fields);
    }
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

fn apply_meta(env: &mut JNIEnv<'_>, item: &JObject<'_>, meta: &JObject<'_>) {
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

fn check_rich_lore(env: &mut JNIEnv<'_>) {
    use foton_registry::data_components::{components::ItemLore, vanilla_components::LORE};
    let rich = bridge_native_bridge::adventure_component_from_json(
        r#"{"text":"rich","bold":true,"color":"gold","extra":[{"text":"child","italic":true}],"hover_event":{"action":"show_text","value":{"text":"hover"}}}"#
    ).expect("rich source");
    let mut source = bridge_item_stack::ItemStack::new(&vanilla_items::STONE);
    source.set(
        LORE,
        ItemLore::new(vec![rich.clone()]).expect("bounded lore"),
    );
    source.set(GLIDER, ());
    source.remove(DAMAGE);
    source.set_opaque_nbt(Some("lore opaque".into()));
    let item = java_item(env, &source);
    assert_eq!(
        native_item(env, &item).stack,
        source,
        "hydration without edit"
    );
    let meta = meta(env, &item);
    let lore = env
        .call_method(&meta, "lore", "()Ljava/util/List;", &[])
        .expect("rich lore getter")
        .l()
        .expect("lore");
    let first = env
        .call_method(&lore, "get", "(I)Ljava/lang/Object;", &[JValue::Int(0)])
        .expect("first line")
        .l()
        .expect("component");
    let json = env
        .call_static_method(
            "foton/ComponentJson",
            "json",
            "(Lnet/kyori/adventure/text/Component;)Ljava/lang/String;",
            &[JValue::Object(&first)],
        )
        .expect("serialize getter")
        .l()
        .expect("json");
    let json: String = env
        .get_string(&jni::objects::JString::from(json))
        .expect("JSON string")
        .into();
    assert_eq!(
        bridge_native_bridge::adventure_component_from_json(&json),
        Some(rich.clone()),
        "getter retains rich style/children/hover"
    );
    let edited = env
        .new_object(
            "java/util/ArrayList",
            "(Ljava/util/Collection;)V",
            &[JValue::Object(&lore)],
        )
        .expect("editable lore");
    env.call_method(
        &edited,
        "add",
        "(Ljava/lang/Object;)Z",
        &[JValue::Object(&first)],
    )
    .expect("append line");
    env.call_method(
        &meta,
        "lore",
        "(Ljava/util/List;)V",
        &[JValue::Object(&edited)],
    )
    .expect("lore setter");
    apply_meta(env, &item, &meta);
    source.set(
        LORE,
        ItemLore::new(vec![rich.clone(), rich]).expect("bounded edited lore"),
    );
    assert_eq!(
        native_item(env, &item).stack,
        source,
        "lore edit retains styles and unrelated state"
    );
}

fn check_empty_banner(env: &mut JNIEnv<'_>) {
    use foton_registry::data_components::{
        components::BannerPatternLayers, vanilla_components::BANNER_PATTERNS,
    };
    for kind in ["iron_chestplate", "shield", "white_banner"] {
        let mut source = bridge_native_bridge::parse_slot(&format!(
            "minecraft:{kind} 1\u{1d}pattern=minecraft:cross,red\u{1d}basecolor=blue"
        ))
        .expect("banner source");
        source.set(GLIDER, ());
        source.remove(DAMAGE);
        source.set_opaque_nbt(Some("banner opaque".into()));
        let item = java_item(env, &source);
        let meta = meta(env, &item);
        let empty = env
            .call_static_method("java/util/List", "of", "()Ljava/util/List;", &[])
            .expect("empty list")
            .l()
            .expect("list");
        env.call_method(
            &meta,
            "setBannerPatternsComponent",
            "(Ljava/util/List;)V",
            &[JValue::Object(&empty)],
        )
        .expect("empty typed SET");
        apply_meta(env, &item, &meta);
        source.set(BANNER_PATTERNS, BannerPatternLayers::empty());
        assert_eq!(
            native_item(env, &item).stack,
            source,
            "{kind} explicit empty differs from remove/reset and retains base color"
        );
    }
}

fn check_family(env: &mut JNIEnv<'_>, item: &str, fields: &str) {
    let mut source = bridge_native_bridge::parse_slot(&format!(
        "minecraft:{item} 1\u{1d}{fields}\u{1d}hide=fixture:opaque\u{1d}hidetooltip"
    ))
    .expect("parent-supported source");
    source.set(GLIDER, ());
    source.remove(DAMAGE);
    source.set_opaque_nbt(Some("source-only opaque state".to_owned()));
    let java = java_item(env, &source);
    let mutation = env
        .call_method(&java, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("hydrated mutation")
        .l()
        .expect("mutation");
    let keys = JObjectArray::from(
        env.get_field(&mutation, "keys", "[Ljava/lang/String;")
            .expect("keys")
            .l()
            .expect("array"),
    );
    assert_eq!(
        env.get_array_length(&keys).expect("journal length"),
        0,
        "{item} hydration is not mutation"
    );
    assert_eq!(
        native_item(env, &java).stack,
        source,
        "{item} hydration preserves full snapshot"
    );
    let meta = env
        .call_method(
            &java,
            "getItemMeta",
            "()Lorg/bukkit/inventory/meta/ItemMeta;",
            &[],
        )
        .expect("donor meta")
        .l()
        .expect("meta");
    env.call_method(&meta, "setHideTooltip", "(Z)V", &[JValue::Bool(0)])
        .expect("visibility mutation");
    let mut target = source.clone();
    target.count = 3;
    target.set_opaque_nbt(Some("recipient-only opaque state".to_owned()));
    let recipient = java_item(env, &target);
    assert!(
        env.call_method(
            &recipient,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(&meta)]
        )
        .expect("same-family donor")
        .z()
        .expect("accepted")
    );
    let result = native_item(env, &recipient);
    let mut expected = target;
    let mut display = source.get(TOOLTIP_DISPLAY).expect("tooltip").clone();
    display.hide_tooltip = false;
    expected.set(TOOLTIP_DISPLAY, display);
    assert_eq!(
        result.stack, expected,
        "{item} edits preserve all other groups and recipient state"
    );
    assert_eq!(
        native_item(env, &java).stack,
        source,
        "{item} donor independent"
    );
    if item != "stone" {
        let incompatible = java_item(
            env,
            &bridge_item_stack::ItemStack::new(&vanilla_items::STONE),
        );
        assert!(
            !env.call_method(
                &incompatible,
                "setItemMeta",
                "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
                &[JValue::Object(&meta)]
            )
            .expect("incompatible family is false")
            .z()
            .expect("rejected")
        );
        assert_eq!(
            native_item(env, &incompatible).stack,
            bridge_item_stack::ItemStack::new(&vanilla_items::STONE)
        );
    }
    env.delete_local_ref(JObject::from(keys))
        .expect("release journal array");
}
