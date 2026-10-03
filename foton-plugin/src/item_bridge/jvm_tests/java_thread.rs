use foton_registry::{
    ItemStackTemplate,
    data_components::{
        DataComponentPatch,
        components::{
            BundleContents, ChargedProjectiles, ItemContainerContents, SulfurCubeContent,
            UseRemainder,
        },
        vanilla_components::{
            BUNDLE_CONTENTS, CHARGED_PROJECTILES, CONTAINER, CUSTOM_NAME, SULFUR_CUBE_CONTENT,
            USE_REMAINDER,
        },
    },
    item_stack::ItemStack,
    vanilla_items,
};
use jni::{JNIEnv, objects::JValue};
use text_components::TextComponent;

use crate::item_bridge::{ItemBridgeError, preflight, store, transfer};

fn nested(children: usize, templates: usize, mixed: bool) -> ItemStack {
    let mut text = TextComponent::plain("leaf");
    for _ in 0..children {
        let mut parent = TextComponent::plain("level");
        parent.children.push(text);
        text = parent;
    }
    let mut patch = DataComponentPatch::new();
    patch.set(CUSTOM_NAME, text);
    for level in 0..templates {
        let item = ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
            .expect("reachable persistent template construction");
        patch = DataComponentPatch::new();
        match if mixed { level % 5 } else { 0 } {
            0 => patch.set(
                BUNDLE_CONTENTS,
                BundleContents::with_selected_item(vec![item], 0),
            ),
            1 => patch.set(USE_REMAINDER, UseRemainder::new(item)),
            2 => patch.set(
                CONTAINER,
                ItemContainerContents::new(vec![Some(item)]).expect("one slot"),
            ),
            3 => patch.set(
                CHARGED_PROJECTILES,
                ChargedProjectiles::new(vec![item]).expect("one projectile"),
            ),
            _ => patch.set(SULFUR_CUBE_CONTENT, SulfurCubeContent::new(item)),
        }
    }
    ItemStack::with_count_and_patch(&vanilla_items::BUNDLE, 1, patch)
}

#[test]
fn template_counter_follows_mixed_wrappers_but_not_siblings() {
    foton_registry::init_vanilla_registry();
    let accepted = nested(0, 4, true);
    preflight::check(&accepted).expect("four mixed templates");
    let fifth = nested(0, 5, true);
    assert!(matches!(
        preflight::check(&fifth),
        Err(ItemBridgeError::TemplateDepth(4))
    ));
    // A SET journal starts at a component, but must enforce the same path cap.
    for (_, entry) in fifth.patch().iter() {
        if let foton_registry::data_components::ComponentPatchEntry::Set(value) = entry {
            assert!(matches!(
                preflight::check_component(value),
                Err(ItemBridgeError::TemplateDepth(4))
            ));
        }
    }
    let inner = nested(0, 3, true);
    let item = ItemStackTemplate::try_with_count_and_patch(
        &vanilla_items::STONE,
        1,
        inner.components_patch().clone(),
    )
    .expect("real four-template sibling");
    let mut siblings = ItemStack::new(&vanilla_items::BUNDLE);
    siblings.set(
        BUNDLE_CONTENTS,
        BundleContents::with_selected_item(vec![item.clone(), item], 0),
    );
    preflight::check(&siblings).expect("siblings do not accumulate depth");
}

pub(super) fn check(env: &mut JNIEnv<'_>) {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository");
    let output = tempfile::tempdir().expect("Java thread fixture output under configured TMPDIR");
    let property = env
        .new_string("java.class.path")
        .expect("classpath property");
    let classpath = env
        .call_static_method(
            "java/lang/System",
            "getProperty",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&property)],
        )
        .expect("host classpath")
        .l()
        .expect("classpath");
    let classpath = env
        .get_string(&classpath.into())
        .expect("classpath text")
        .to_string_lossy()
        .into_owned();
    let java_home = std::env::var_os("JAVA_HOME").expect("JNI test JAVA_HOME");
    let compile = std::process::Command::new(std::path::Path::new(&java_home).join("bin/javac"))
        .arg("-cp")
        .arg(classpath)
        .arg("-d")
        .arg(output.path())
        .arg(repository.join("plugin-api/check/ItemThreadProbe.java"))
        .output()
        .expect("compile Java thread fixture");
    assert!(
        compile.status.success(),
        "Java thread fixture compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let bytes = std::fs::read(output.path().join("ItemThreadProbe.class"))
        .expect("compiled Java thread fixture");
    let loader = env
        .call_static_method(
            "java/lang/ClassLoader",
            "getSystemClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .expect("system class loader")
        .l()
        .expect("loader");
    let class = env
        .define_class("ItemThreadProbe", &loader, &bytes)
        .expect("define actual thread fixture");

    // Patch/component/text contributes 2 + children; every contained template
    // adds component/template/patch (3). 2 + 114 + 4*3 = aggregate depth 128.
    let boundary = nested(114, 4, false);
    preflight::check(&boundary).expect("aggregate depth 128 accepted");
    let refused = nested(115, 4, false);
    assert!(matches!(
        store().expect("store").capture(&refused),
        Err(ItemBridgeError::Depth(128))
    ));
    for mixed in [false, true] {
        let fifth = nested(0, 5, mixed);
        assert!(matches!(
            store().expect("store").capture(&fifth),
            Err(ItemBridgeError::TemplateDepth(4))
        ));
        let accepted = nested(0, 4, mixed);
        let transfer =
            transfer::capture(env, &accepted).expect("four-template recovery after fifth refusal");
        let item = env
            .call_static_method(
                "foton/FotonInventory",
                "decodeTransfer",
                "(Lfoton/item/ItemTransfer;)Lorg/bukkit/inventory/ItemStack;",
                &[JValue::Object(&transfer)],
            )
            .expect("four-template item")
            .l()
            .expect("item");
        let result = env.call_static_method(
            &class,
            "check",
            "(Lorg/bukkit/inventory/ItemStack;)V",
            &[JValue::Object(&item)],
        );
        if result.is_err() {
            env.exception_describe().expect("four-template failure");
        }
        result.expect("four-template clone on actual Java-created thread");
    }
    let transfer = transfer::capture(env, &boundary).expect("valid sibling after depth refusal");
    let item = env
        .call_static_method(
            "foton/FotonInventory",
            "decodeTransfer",
            "(Lfoton/item/ItemTransfer;)Lorg/bukkit/inventory/ItemStack;",
            &[JValue::Object(&transfer)],
        )
        .expect("combined-depth item")
        .l()
        .expect("item");
    let result = env.call_static_method(
        class,
        "check",
        "(Lorg/bukkit/inventory/ItemStack;)V",
        &[JValue::Object(&item)],
    );
    if result.is_err() {
        env.exception_describe().expect("Java thread failure");
    }
    result.expect("combined item/template/text semantic clone on actual Java-created thread");
    env.delete_local_ref(loader)
        .expect("release loader reference");
}
