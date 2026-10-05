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
