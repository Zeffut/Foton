use super::*;
use foton_registry::vanilla_entities;
use jni::JavaVM;
use jni::objects::{JValue, JValueGen};
use std::cell::RefCell;
use std::error::Error;
use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use super::entity_bridge_tests::rain_test_world;
use crate::forward::spawn_bridge_tests::{live_test_server, spawn_check_host};
use foton_core::entity::next_entity_id;
use foton_core::event::EntityRemoveFromWorldEvent;

thread_local! {
    static SOURCE_RESOLVED: RefCell<Option<Arc<Barrier>>> = const { RefCell::new(None) };
}

pub(super) fn after_source_resolution() {
    SOURCE_RESOLVED.with(|hook| {
        if let Some(barrier) = hook.borrow_mut().take() {
            barrier.wait();
            barrier.wait();
        }
    });
}

#[test]
#[ignore = "requires the built plugin API and an isolated JVM"]
fn removed_projectile_rejects_resolved_custom_source() -> Result<(), Box<dyn Error>> {
    resolved_setter_cannot_outlive_removal(true)
}

#[test]
#[ignore = "requires the built plugin API and an isolated JVM"]
fn removed_projectile_rejects_resolved_entity_owner() -> Result<(), Box<dyn Error>> {
    resolved_setter_cannot_outlive_removal(false)
}

fn resolved_setter_cannot_outlive_removal(custom: bool) -> Result<(), Box<dyn Error>> {
    let (_storage, server) = live_test_server()?;
    let world = rain_test_world();
    server
        .worlds
        .insert(world.key.clone(), Arc::clone(&world))?;
    server.attach_worlds();
    let (_scratch, host) = spawn_check_host()?;
    host.bind_server(&Arc::downgrade(&server));
    let arrow: SharedEntity = Arc::new(ArrowEntity::new(
        &vanilla_entities::ARROW,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world.try_add_entity(Arc::clone(&arrow))?;
    let id = arrow.uuid();
    let owner = create_entity_at(
        &world,
        &vanilla_entities::COW.key,
        DVec3::new(9.5, 64.0, 8.5),
    )
    .ok_or("owner must construct")?;
    let owner_id = owner.uuid();
    world.try_add_entity(owner)?;
    let callbacks = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&callbacks);
    server
        .events
        .on::<EntityRemoveFromWorldEvent, _>("foton:race_test".parse()?, move |event| {
            if event.entity() == id {
                assert!(
                    projectile_sources().try_write().is_some(),
                    "callback ran under source lock"
                );
                seen.fetch_add(1, Ordering::SeqCst);
            }
        });
    let mut env = host.vm.attach_current_thread()?;
    let source = env
        .call_static_method(
            "SpawnBridgeCheck",
            "customSource",
            "()Lorg/bukkit/projectiles/ProjectileSource;",
            &[],
        )?
        .l()?;
    let source = env.new_global_ref(source)?;
    projectile_sources().write().insert(id, source.clone());
    let barrier = Arc::new(Barrier::new(2));
    let result = thread::scope(|scope| {
        let setter = scope.spawn(|| {
            set_resolved_source(
                &host.vm,
                id,
                (!custom).then_some(owner_id),
                &source,
                &barrier,
            )
        });
        barrier.wait();
        let removal = (|| -> Result<(), Box<dyn Error>> {
            let uuid = env.new_string(id.to_string())?;
            for _ in 0..2 {
                env.call_static_method(
                    "foton/Native",
                    "removeEntity",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&uuid)],
                )?;
            }
            Ok(())
        })();
        let remaining = projectile_source_count();
        barrier.wait();
        let result = setter.join().map_err(|_| "setter thread panicked")?;
        removal?;
        assert_eq!(
            remaining, 0,
            "removal must clear the source before the setter resumes"
        );
        Ok::<_, Box<dyn Error>>(result?)
    })?;
    assert!(
        !result,
        "setter resolved before removal must reject stale entity"
    );
    assert!(arrow.is_removed());
    assert!(arrow.projectile_owner_uuid().is_none());
    assert!(world.get_entity_by_uuid(&id).is_none());
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(projectile_source_count(), 0);
    assert_stale_projectile_getters(&mut env, &world, &arrow, source)?;
    server.cancel_token.cancel();
    drop(env);
    host.disable_all()?;
    Ok(())
}

fn set_resolved_source(
    vm: &JavaVM,
    id: Uuid,
    owner: Option<Uuid>,
    source: &GlobalRef,
    barrier: &Arc<Barrier>,
) -> Result<bool, String> {
    SOURCE_RESOLVED.with(|hook| *hook.borrow_mut() = Some(Arc::clone(barrier)));
    let result = (|| {
        let mut env = vm
            .attach_current_thread()
            .map_err(|error| error.to_string())?;
        let uuid = env
            .new_string(id.to_string())
            .map_err(|error| error.to_string())?;
        let owner = env
            .new_string(owner.map_or_else(String::new, |id| id.to_string()))
            .map_err(|error| error.to_string())?;
        env.call_static_method(
            "foton/Native",
            "setEntityProjectileSource",
            "(Ljava/lang/String;Ljava/lang/String;Lorg/bukkit/projectiles/ProjectileSource;Z)Z",
            &[
                JValue::Object(&uuid),
                JValue::Object(&owner),
                JValue::Object(source.as_obj()),
                JValue::Bool(0),
            ],
        )
        .and_then(JValueGen::z)
        .map_err(|error| error.to_string())
    })();
    if SOURCE_RESOLVED.with(|hook| hook.borrow().is_some()) {
        // Release the remover even if JNI failed before reaching the resolution hook.
        after_source_resolution();
        return Err(format!(
            "setter did not reach its resolution hook: {result:?}"
        ));
    }
    result
}

fn assert_stale_projectile_getters(
    env: &mut JNIEnv<'_>,
    world: &Arc<World>,
    entity: &SharedEntity,
    source: GlobalRef,
) -> Result<(), Box<dyn Error>> {
    let id = entity.uuid();
    // Check both orphaned state and a removed entity still addressable during a pending spawn.
    entity
        .as_projectile()
        .ok_or("test entity must be a projectile")?
        .set_owner_uuid(Some(id));
    let uuid = env.new_string(id.to_string())?;
    for pending in [false, true] {
        if pending {
            world.begin_pending_spawn(Arc::clone(entity));
        }
        for custom in [true, false] {
            if custom {
                projectile_sources().write().insert(id, source.clone());
            } else {
                remove_projectile_source(&id);
            }
            for method in [
                "entityProjectileSource",
                "entityProjectileShooter",
                "entityProjectileOwner",
            ] {
                let signature = if method == "entityProjectileOwner" {
                    "(Ljava/lang/String;)Ljava/lang/String;"
                } else if method == "entityProjectileSource" {
                    "(Ljava/lang/String;)Lorg/bukkit/projectiles/ProjectileSource;"
                } else {
                    "(Ljava/lang/String;)Ljava/lang/Object;"
                };
                assert!(
                    env.call_static_method(
                        "foton/Native",
                        method,
                        signature,
                        &[JValue::Object(&uuid)]
                    )?
                    .l()?
                    .is_null()
                );
            }
        }
    }
    world.take_pending_spawn(&id);
    remove_projectile_source(&id);
    assert_eq!(projectile_source_count(), 0);
    Ok(())
}

#[test]
#[ignore = "requires the built plugin API and an isolated JVM"]
fn native_non_finite_spawns_return_null_and_server_survives() -> Result<(), Box<dyn Error>> {
    let (_storage, server) = live_test_server()?;
    let world = rain_test_world();
    server
        .worlds
        .insert(world.key.clone(), Arc::clone(&world))?;
    server.attach_worlds();
    let (_scratch, host) = spawn_check_host()?;
    host.bind_server(&Arc::downgrade(&server));
    let mut env = host.vm.attach_current_thread()?;
    let name = env.new_string(world.key.to_string())?;
    let kind = env.new_string("arrow")?;
    let before = next_entity_id();
    for method in ["spawnEntity", "spawnEntityPending"] {
        for initialization in ["", "minecraft:water"] {
            let initialization = env.new_string(initialization)?;
            for axis in 0..3 {
                for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                    let mut position = [8.5, 64.0, 8.5];
                    position[axis] = value;
                    let result = env.call_static_method(
                        "foton/Native", method,
                        "(Ljava/lang/String;DDDLjava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                        &[JValue::Object(&name), JValue::Double(position[0]), JValue::Double(position[1]),
                          JValue::Double(position[2]), JValue::Object(&kind), JValue::Object(&initialization)],
                    )?.l()?;
                    assert!(result.is_null(), "{method} accepted axis {axis}: {value}");
                    assert!(!env.exception_check()?);
                    assert!(world.accessible_entities().is_empty());
                }
            }
        }
    }
    assert_eq!(
        next_entity_id(),
        before + 1,
        "invalid native calls must allocate no entity IDs or pending entities"
    );
    let initialization = env.new_string("")?;
    let uuid = env
        .call_static_method(
            "foton/Native",
            "spawnEntityPending",
            "(Ljava/lang/String;DDDLjava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&name),
                JValue::Double(8.5),
                JValue::Double(64.0),
                JValue::Double(8.5),
                JValue::Object(&kind),
                JValue::Object(&initialization),
            ],
        )?
        .l()?;
    assert!(
        !uuid.is_null(),
        "server must accept a valid spawn after all invalid requests"
    );
    assert!(
        env.call_static_method(
            "foton/Native",
            "finishPendingSpawn",
            "(Ljava/lang/String;Ljava/lang/String;Z)Z",
            &[
                JValue::Object(&name),
                JValue::Object(&uuid),
                JValue::Bool(1)
            ]
        )?
        .z()?
    );
    assert_eq!(world.accessible_entities().len(), 1);
    server.cancel_token.cancel();
    drop(env);
    host.disable_all()?;
    Ok(())
}
