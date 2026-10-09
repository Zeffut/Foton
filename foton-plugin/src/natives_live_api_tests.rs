use std::env::var_os;
use std::error::Error;
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Weak;

use jni::JNIEnv;
use jni::objects::{JObject, JValue};

use crate::{PluginHost, PluginHostConfig};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn java_home() -> Option<PathBuf> {
    if let Some(home) = var_os("JAVA_HOME").or_else(|| var_os("FOTON_JAVA_HOME")) {
        return Some(PathBuf::from(home));
    }
    if cfg!(target_os = "macos") {
        let output = Command::new("/usr/libexec/java_home").output().ok()?;
        if output.status.success() {
            let home = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
            if home.join("lib/server/libjvm.dylib").is_file() {
                return Some(home);
            }
        }
    }
    let mut homes = read_dir("/usr/lib/jvm")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("lib/server/libjvm.so").is_file())
        .collect::<Vec<_>>();
    homes.sort();
    homes.pop()
}

#[test]
#[ignore = "requires a prebuilt plugin API JAR and JDK; dev/ci.sh runs it in isolation"]
fn generated_java_apis_are_safe_before_registry_publication() -> Result<(), Box<dyn Error>> {
    assert!(foton_registry::REGISTRY.get().is_none());
    let java_home = java_home().ok_or("Java installation required for pre-publication JNI test")?;
    let root = repo();
    let api_jar = root.join("plugin-api/build/foton-plugin-api.jar");
    if !api_jar.is_file() {
        return Err("run dev/build-plugin-api.sh before the pre-publication JNI test".into());
    }
    let host = PluginHost::start(
        &PluginHostConfig {
            java_home,
            api_jar,
            item_snapshot_limit: PluginHostConfig::DEFAULT_ITEM_SNAPSHOT_LIMIT,
            library_directories: vec![root.join("plugin-api/lib")],
            plugin_directory: root.join("plugin-api/build/no-plugins"),
        },
        &Weak::new(),
    )?;
    let mut env = host.vm.attach_current_thread()?;

    let tag = env
        .get_static_field(
            "org/bukkit/Tag",
            "ITEMS_TRIMMABLE_ARMOR",
            "Lorg/bukkit/Tag;",
        )?
        .l()?;
    let material = env
        .get_static_field(
            "org/bukkit/Material",
            "DIAMOND_CHESTPLATE",
            "Lorg/bukkit/Material;",
        )?
        .l()?;
    let tagged = env
        .call_method(
            &tag,
            "isTagged",
            "(Lorg/bukkit/Keyed;)Z",
            &[JValue::Object(&material)],
        )?
        .z()?;
    assert!(!tagged);
    let values = env
        .call_method(&tag, "getValues", "()Ljava/util/Set;", &[])?
        .l()?;
    assert_eq!(env.call_method(&values, "size", "()I", &[])?.i()?, 0);

    let enchantment_class = "org/bukkit/enchantments/Enchantment";
    let enchantment_descriptor = "Lorg/bukkit/enchantments/Enchantment;";
    let infinity = env
        .get_static_field(enchantment_class, "INFINITY", enchantment_descriptor)?
        .l()?;
    let mending = env
        .get_static_field(enchantment_class, "MENDING", enchantment_descriptor)?
        .l()?;
    assert!(
        !env.call_method(
            &infinity,
            "conflictsWith",
            "(Lorg/bukkit/enchantments/Enchantment;)Z",
            &[JValue::Object(&mending)],
        )?
        .z()?
    );

    let item = env.new_object(
        "org/bukkit/inventory/ItemStack",
        "(Lorg/bukkit/Material;)V",
        &[JValue::Object(&material)],
    )?;
    assert!(
        !env.call_method(
            &infinity,
            "canEnchantItem",
            "(Lorg/bukkit/inventory/ItemStack;)Z",
            &[JValue::Object(&item)],
        )?
        .z()?
    );

    let potion_effect_type = "org/bukkit/potion/PotionEffectType";
    let potion_effect_descriptor = "Lorg/bukkit/potion/PotionEffectType;";
    for (name, expected) in [
        ("INSTANT_HEALTH", true),
        ("INSTANT_DAMAGE", true),
        ("SATURATION", true),
        ("LUCK", false),
    ] {
        let effect = env
            .get_static_field(potion_effect_type, name, potion_effect_descriptor)?
            .l()?;
        let instantaneous = env.call_method(&effect, "isInstant", "()Z", &[])?.z()?;
        assert_eq!(
            instantaneous, expected,
            "pre-publication {name} instant state"
        );
    }
    assert!(foton_registry::REGISTRY.get().is_none());
    Ok(())
}

#[test]
#[ignore = "requires the built plugin API jar and a JDK"]
fn generated_java_apis_reach_live_registry_natives() -> Result<(), Box<dyn Error>> {
    let java_home = java_home().ok_or("Java installation required for live JNI API test")?;
    let root = repo();
    let api_jar = root.join("plugin-api/build/foton-plugin-api.jar");
    if !api_jar.is_file() {
        return Err("run dev/build-plugin-api.sh before the live JNI API test".into());
    }
    foton_registry::init_vanilla_registry();
    let host = PluginHost::start(
        &PluginHostConfig {
            java_home,
            api_jar,
            item_snapshot_limit: PluginHostConfig::DEFAULT_ITEM_SNAPSHOT_LIMIT,
            library_directories: vec![root.join("plugin-api/lib")],
            plugin_directory: root.join("plugin-api/build/no-plugins"),
        },
        &Weak::new(),
    )?;
    let mut env = host.vm.attach_current_thread()?;

    let tag = env
        .get_static_field(
            "org/bukkit/Tag",
            "ITEMS_TRIMMABLE_ARMOR",
            "Lorg/bukkit/Tag;",
        )?
        .l()?;
    for (material, expected) in [("DIAMOND_CHESTPLATE", true), ("ELYTRA", false)] {
        let material = env
            .get_static_field("org/bukkit/Material", material, "Lorg/bukkit/Material;")?
            .l()?;
        let tagged = env
            .call_method(
                &tag,
                "isTagged",
                "(Lorg/bukkit/Keyed;)Z",
                &[JValue::Object(&material)],
            )?
            .z()?;
        assert_eq!(tagged, expected);
    }

    let enchantment_class = "org/bukkit/enchantments/Enchantment";
    let descriptor = "Lorg/bukkit/enchantments/Enchantment;";
    let infinity = env
        .get_static_field(enchantment_class, "INFINITY", descriptor)?
        .l()?;
    let mending = env
        .get_static_field(enchantment_class, "MENDING", descriptor)?
        .l()?;
    let sharpness = env
        .get_static_field(enchantment_class, "SHARPNESS", descriptor)?
        .l()?;
    let unbreaking = env
        .get_static_field(enchantment_class, "UNBREAKING", descriptor)?
        .l()?;
    for (first, second, expected) in [
        (&infinity, &infinity, true),
        (&infinity, &mending, true),
        (&mending, &infinity, true),
        (&sharpness, &unbreaking, false),
    ] {
        let conflicts = env
            .call_method(
                first,
                "conflictsWith",
                "(Lorg/bukkit/enchantments/Enchantment;)Z",
                &[JValue::Object(second)],
            )?
            .z()?;
        assert_eq!(conflicts, expected);
    }

    let unknown = env.new_string("example:infinity")?;
    let known = env.new_string("minecraft:mending")?;
    let conflicts = env
        .call_static_method(
            "foton/Native",
            "enchantmentsConflict",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            &[JValue::Object(&unknown), JValue::Object(&known)],
        )?
        .z()?;
    assert!(!conflicts);
    Ok(())
}

#[test]
#[ignore = "requires the built plugin API jar and a JDK"]
fn block_data_follows_the_registry_properties() -> Result<(), Box<dyn Error>> {
    let java_home = java_home().ok_or("Java installation required for live JNI API test")?;
    let root = repo();
    let api_jar = root.join("plugin-api/build/foton-plugin-api.jar");
    if !api_jar.is_file() {
        return Err("run dev/build-plugin-api.sh before the live JNI API test".into());
    }
    foton_registry::init_vanilla_registry();
    let host = PluginHost::start(
        &PluginHostConfig {
            java_home,
            api_jar,
            item_snapshot_limit: PluginHostConfig::DEFAULT_ITEM_SNAPSHOT_LIMIT,
            library_directories: vec![root.join("plugin-api/lib")],
            plugin_directory: root.join("plugin-api/build/no-plugins"),
        },
        &Weak::new(),
    )?;
    let mut env = host.vm.attach_current_thread()?;
    // A property the text leaves out takes the registry default.
    let campfire = create_block_data(&mut env, "campfire")?;
    let text = env
        .call_method(&campfire, "getAsString", "()Ljava/lang/String;", &[])?
        .l()?;
    let text: String = env.get_string(&text.into())?.into();
    assert_eq!(
        text,
        "minecraft:campfire[facing=north,lit=true,signal_fire=false,waterlogged=false]"
    );
    for interface in [
        "org/bukkit/block/data/type/Campfire",
        "org/bukkit/block/data/Lightable",
        "org/bukkit/block/data/Directional",
        "org/bukkit/block/data/Waterlogged",
    ] {
        assert!(env.is_instance_of(&campfire, interface)?, "{interface}");
    }

    // Setters check the value against what the block allows for the property.
    let face = env
        .get_static_field(
            "org/bukkit/block/BlockFace",
            "UP",
            "Lorg/bukkit/block/BlockFace;",
        )?
        .l()?;
    let rejected = env.call_method(
        &campfire,
        "setFacing",
        "(Lorg/bukkit/block/BlockFace;)V",
        &[JValue::Object(&face)],
    );
    assert!(rejected.is_err(), "a campfire cannot face up");
    let thrown = env.exception_occurred()?;
    env.exception_clear()?;
    assert!(env.is_instance_of(&thrown, "java/lang/IllegalArgumentException")?);

    let wheat = create_block_data(&mut env, "minecraft:wheat[age=2]")?;
    let maximum = env.call_method(&wheat, "getMaximumAge", "()I", &[])?.i()?;
    assert_eq!(maximum, 7);
    assert!(
        env.call_method(&wheat, "setAge", "(I)V", &[JValue::Int(8)])
            .is_err()
    );
    env.exception_clear()?;
    env.call_method(&wheat, "setAge", "(I)V", &[JValue::Int(5)])?;
    let age = env.call_method(&wheat, "getAge", "()I", &[])?.i()?;
    assert_eq!(age, 5);

    let log = create_block_data(&mut env, "minecraft:oak_log")?;
    let axes = env
        .call_method(&log, "getAxes", "()Ljava/util/Set;", &[])?
        .l()?;
    assert_eq!(env.call_method(&axes, "size", "()I", &[])?.i()?, 3);
    Ok(())
}

fn create_block_data<'local>(
    env: &mut JNIEnv<'local>,
    text: &str,
) -> Result<JObject<'local>, Box<dyn Error>> {
    let text = env.new_string(text)?;
    Ok(env
        .call_static_method(
            "org/bukkit/Bukkit",
            "createBlockData",
            "(Ljava/lang/String;)Lorg/bukkit/block/data/BlockData;",
            &[JValue::Object(&text)],
        )?
        .l()?)
}
