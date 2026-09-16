use super::*;
use std::env::{join_paths, var_os};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Weak;

#[test]
#[ignore = "requires the built plugin API; dev/ci.sh runs this after the Java build"]
fn spawn_bridge_dispatches_and_returns_java_cancellation() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let scratch = tempfile::tempdir()?;
    let java_home = if let Some(home) = var_os("JAVA_HOME") {
        PathBuf::from(home)
    } else {
        let properties = Command::new("java")
            .args(["-XshowSettings:properties", "-version"])
            .output()?;
        assert!(
            properties.status.success(),
            "Java must report its runtime home"
        );
        String::from_utf8(properties.stderr)?
            .lines()
            .find_map(|line| line.trim().strip_prefix("java.home = ").map(PathBuf::from))
            .ok_or("java.home was not reported")?
    };
    let api = scratch.path().join("foton-plugin-api.jar");
    fs::copy(root.join("plugin-api/build/foton-plugin-api.jar"), &api)?;
    let libraries = root.join("plugin-api/lib");
    let classpath = join_paths([api.clone(), libraries.join("*")])?;
    assert!(
        Command::new(java_home.join("bin/javac"))
            .args(["--release", "21", "-cp"])
            .arg(&classpath)
            .arg("-d")
            .arg(scratch.path())
            .arg(root.join("plugin-api/check/SpawnBridgeCheck.java"))
            .status()?
            .success()
    );
    let mut jar = Command::new(java_home.join("bin/jar"));
    jar.arg("uf").arg(&api).current_dir(scratch.path());
    for file in fs::read_dir(scratch.path())? {
        let file = file?;
        if file.path().extension().is_some_and(|ext| ext == "class") {
            jar.arg(file.file_name());
        }
    }
    assert!(jar.status()?.success());
    let host = crate::PluginHost::start(
        &crate::PluginHostConfig {
            java_home,
            api_jar: api,
            library_directory: Some(libraries),
            plugin_directory: scratch.path().join("plugins"),
        },
        &Weak::new(),
    )?;
    let mut env = host.vm.attach_current_thread()?;
    env.call_static_method("SpawnBridgeCheck", "install", "()V", &[])?;
    let pre = pre_creature_spawn_call(
        &host.vm,
        "minecraft:overworld",
        1.0,
        64.0,
        2.0,
        "minecraft:bee",
        PluginSpawnReason::TrialSpawner,
    );
    let spawn = creature_spawn_call(
        &host.vm,
        "00000000-0000-0000-0000-000000000007",
        "minecraft:overworld",
        1.0,
        64.0,
        2.0,
        PluginSpawnReason::Beehive,
    );
    assert_eq!(
        (pre, spawn),
        (false, false),
        "both Java cancellations must cross JNI"
    );
    assert_eq!(
        env.get_static_field("SpawnBridgeCheck", "preCalls", "I")?
            .i()?,
        1
    );
    assert_eq!(
        env.get_static_field("SpawnBridgeCheck", "spawnCalls", "I")?
            .i()?,
        1
    );
    assert!(
        env.call_static_method("SpawnBridgeCheck", "absentEntityIsDefault", "()Z", &[])?
            .z()?
    );
    Ok(())
}
