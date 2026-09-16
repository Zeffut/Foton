use super::*;
use std::env::{join_paths, var_os};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Weak;

use foton_core::block_entity::BlockEntity as _;
use foton_core::block_entity::entities::{BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR, BeehiveBlockEntity};
use foton_core::entity::entities::{BatEntity, BeeEntity};
use foton_core::entity::{Entity as _, SharedEntity, next_entity_id};
use foton_core::permission::{PermissionGroupManager, PermissionGroupsConfig};
use foton_registry::{vanilla_blocks, vanilla_entities};
use foton_utils::{BlockPos, Downcast as _, WorldAabb, types::UpdateFlags};
use glam::DVec3;

#[test]
#[ignore = "requires the built plugin API; dev/ci.sh runs this after the Java build"]
fn spawn_bridge_dispatches_cancellation_and_queries_released_bee() -> Result<(), Box<dyn Error>> {
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
    drop(env);
    live_beehive_release_returns_java_provenance(&host)?;
    Ok(())
}

fn live_test_server() -> Result<(tempfile::TempDir, Arc<Server>), Box<dyn Error>> {
    // Server configuration requires a relative save path; TempDir removes only
    // this test's directory, including when the Java assertion fails.
    let storage = tempfile::Builder::new()
        .prefix(".spawn-bridge-")
        .tempdir_in(".")?;
    let mut worlds: foton_core::config::WorldsConfig = toml::from_str(
        r#"
        seed = "0"
        [storage]
        type = "foton:ram"
        [domains.minecraft]
        default = true
        [[domains.minecraft.worlds]]
        name = "spawn_bridge"
        generator = "foton:empty"
        default = true
        [domains.minecraft.worlds.config]
        dimension_type = "minecraft:overworld"
        "#,
    )?;
    worlds.save_path = storage
        .path()
        .file_name()
        .ok_or("test storage directory has no name")?
        .to_string_lossy()
        .into_owned();
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?,
    );
    let mut config = (*crate::natives::entity_bridge_tests::equipment_test_config()).clone();
    // This offline test needs no external Minecraft services.
    config.services_server = Some("http://127.0.0.1:0".to_owned());
    let server = Arc::new(runtime.block_on(foton_core::server::Server::new(
        Arc::clone(&runtime),
        Default::default(),
        config,
        worlds,
        PermissionGroupManager::transient(PermissionGroupsConfig::default())?,
    ))?);
    Ok((storage, server))
}

fn live_beehive_release_returns_java_provenance(
    host: &crate::PluginHost,
) -> Result<(), Box<dyn Error>> {
    let (_storage, server) = live_test_server()?;
    let world = crate::natives::entity_bridge_tests::rain_test_world();
    server
        .worlds
        .insert(world.key.clone(), Arc::clone(&world))?;
    server.attach_worlds();
    host.bind_server(&Arc::downgrade(&server));
    let mut env = host.vm.attach_current_thread()?;
    env.call_static_method("SpawnBridgeCheck", "allowBeehiveRelease", "()V", &[])?;

    let pos = BlockPos::new(8, 64, 8);
    assert!(world.set_block(
        pos,
        vanilla_blocks::BEEHIVE.default_state(),
        UpdateFlags::UPDATE_ALL
    ));
    let block_entity = world
        .get_block_entity(pos)
        .ok_or("placed hive is missing")?;
    let hive = block_entity
        .downcast_ref::<BeehiveBlockEntity>()
        .ok_or("placed block entity is not a beehive")?;
    let bee = BeeEntity::new(
        &vanilla_entities::BEE,
        next_entity_id(),
        DVec3::new(8.5, 64.5, 8.5),
        Arc::downgrade(&world),
    );
    bee.set_has_nectar(true);
    hive.add_occupant(&bee);
    for _ in 0..=BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR + 1 {
        hive.tick(&world);
    }
    assert_eq!(hive.occupant_count(), 0, "the bee must leave the hive");
    let released = world.get_entities_in_aabb_matching(
        &WorldAabb::new(4.0, 60.0, 4.0, 13.0, 69.0, 13.0),
        |entity| entity.entity_type() == &vanilla_entities::BEE,
    );
    assert_eq!(released.len(), 1, "exactly one bee must be inserted");
    assert!(world.get_entity_by_id(released[0].id()).is_some());
    let uuid = env.new_string(released[0].uuid().to_string())?;
    let result = env.call_static_method(
        "SpawnBridgeCheck",
        "assertReleasedBee",
        "(Ljava/lang/String;)V",
        &[(&uuid).into()],
    );
    if result.is_err() && env.exception_check()? {
        env.exception_describe()?;
        env.exception_clear()?;
    }
    let bat = Arc::new(BatEntity::new(
        &vanilla_entities::BAT,
        next_entity_id(),
        DVec3::new(9.5, 64.5, 9.5),
        Arc::downgrade(&world),
    ));
    world.try_add_entity(Arc::clone(&bat) as SharedEntity)?;
    let living = java_entity_handle(&mut env, bat.uuid())?;
    assert!(
        env.is_instance_of(&living, "foton/FotonLivingEntity")?,
        "an unhandled living type must use the living fallback"
    );
    let stale = java_entity_handle(
        &mut env,
        Uuid::from_u128(0x00000000_0000_0000_0000_000000000099),
    )?;
    assert!(env.is_instance_of(&stale, "foton/FotonEntity")?);
    assert!(
        !env.is_instance_of(&stale, "foton/FotonLivingEntity")?,
        "a stale UUID must remain the generic non-living wrapper"
    );
    let world_name = env.new_string(world.key.to_string())?;
    let contracts = env.call_static_method(
        "SpawnBridgeCheck",
        "assertClassSpawnContracts",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&world_name)],
    );
    if contracts.is_err() && env.exception_check()? {
        env.exception_describe()?;
        env.exception_clear()?;
    }
    server.cancel_token.cancel();
    host.disable_all()?;
    contracts?;
    result?;
    Ok(())
}

fn java_entity_handle<'local>(
    env: &mut jni::JNIEnv<'local>,
    uuid: Uuid,
) -> Result<jni::objects::JObject<'local>, Box<dyn Error>> {
    let uuid_text = env.new_string(uuid.to_string())?;
    let uuid = env
        .call_static_method(
            "java/util/UUID",
            "fromString",
            "(Ljava/lang/String;)Ljava/util/UUID;",
            &[JValue::Object(&uuid_text)],
        )?
        .l()?;
    Ok(env
        .call_static_method(
            "foton/FotonEntity",
            "handle",
            "(Ljava/util/UUID;)Lfoton/FotonEntity;",
            &[JValue::Object(&uuid)],
        )?
        .l()?)
}
