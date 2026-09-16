use std::env::var_os;
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Weak;

use jni::objects::JValue;

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
fn generated_java_apis_reach_live_registry_natives() -> Result<(), Box<dyn std::error::Error>> {
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
            library_directory: Some(root.join("plugin-api/lib")),
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
