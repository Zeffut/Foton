//! Entities that exist outside the world for a while: serialized by a plugin,
//! deserialized, then spawned where the plugin chooses.
//!
//! Paper's `deserializeEntity` hands back an entity that is not in any world
//! yet, and the plugin may adjust it before `spawnAt`. Foton names entities to
//! Java by UUID, so the unspawned one waits here, where every entity native
//! finds it, until it is spawned or abandoned.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use foton_core::entity::SharedEntity;
use foton_core::entity::serialization::{deserialize_entity, serialize_entity};
use foton_core::event::entity::CreatureSpawnEvent;
use foton_core::world::World;
use foton_utils::locks::SyncMutex;
use glam::DVec3;
use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jboolean, jbyteArray, jdouble, jfloat, jstring};
use rustc_hash::FxHashMap;
use uuid::Uuid;

use super::support::{entity, method, text, world};
use super::{server, to_java};

/// How long a deserialized entity waits to be spawned.
///
/// Paper's unspawned entity is garbage the moment the plugin drops it; here it
/// is held by UUID, so one the plugin never spawns would be kept forever. A
/// plugin spawns what it deserializes in the same call chain, and an entity
/// left for ten minutes is one nobody is coming back for.
const ABANDONED_ENTITY_TIMEOUT: Duration = Duration::from_secs(600);

type Pending = FxHashMap<Uuid, (Instant, Arc<World>, SharedEntity)>;

static PENDING: OnceLock<SyncMutex<Pending>> = OnceLock::new();

fn pending() -> &'static SyncMutex<Pending> {
    PENDING.get_or_init(|| SyncMutex::new(FxHashMap::default()))
}

/// A deserialized entity still waiting for `spawnAt`.
pub(super) fn pending_entity(uuid: &Uuid) -> Option<(Arc<World>, SharedEntity)> {
    let pending = pending().lock();
    let (_, world, entity) = pending.get(uuid)?;
    Some((Arc::clone(world), Arc::clone(entity)))
}

/// Drops an unspawned entity: `remove()` on one that never spawned.
pub(super) fn discard_pending(uuid: &Uuid) -> bool {
    pending().lock().remove(uuid).is_some()
}

/// The entity's save data, gzipped as Paper's `serializeEntity` writes it.
extern "system" fn serialize_entity_native(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jbyteArray {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return null_mut();
    };
    let Some(bytes) = serialize_entity(entity.as_ref()) else {
        return null_mut();
    };
    env.byte_array_from_slice(&bytes)
        .map_or(null_mut(), JByteArray::into_raw)
}

/// Builds the entity a blob describes and holds it unspawned; the UUID it is
/// held under, or null when the blob does not describe an entity.
extern "system" fn deserialize_entity_native(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    data: JByteArray<'_>,
    world_name: JString<'_>,
    preserve_uuid: jboolean,
) -> jstring {
    let Some(world) = world(&mut env, &world_name) else {
        return null_mut();
    };
    let Ok(bytes) = env.convert_byte_array(&data) else {
        return null_mut();
    };
    let Some(entity) = deserialize_entity(&bytes, &world, preserve_uuid != 0) else {
        return null_mut();
    };
    let id = entity.uuid();
    let now = Instant::now();
    let mut pending = pending().lock();
    pending.retain(|_, (since, _, _)| now.duration_since(*since) < ABANDONED_ENTITY_TIMEOUT);
    pending.insert(id, (now, world, entity));
    drop(pending);
    to_java(&mut env, Some(id.to_string()))
}

/// Places a held entity and adds it to `world`, firing `CreatureSpawnEvent`
/// with `reason` for a living one as Paper's `spawnAt` does. `false` when it
/// was already spawned, the event was cancelled, or the chunk is not loaded.
extern "system" fn spawn_pending_entity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    world_name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    yaw: jfloat,
    pitch: jfloat,
    reason: JString<'_>,
) -> jboolean {
    let Some(id) = text(&mut env, &uuid).and_then(|value| Uuid::parse_str(&value).ok()) else {
        return 0;
    };
    let Some(world) = world(&mut env, &world_name) else {
        return 0;
    };
    let reason = text(&mut env, &reason).unwrap_or_else(|| "DEFAULT".to_owned());
    let Some((_, entity)) = pending_entity(&id) else {
        return 0;
    };
    // Not yet in a world: nothing indexes it by position, so moving it is only
    // a matter of its own fields.
    if entity.try_set_position(DVec3::new(x, y, z)).is_err() {
        return 0;
    }
    entity.set_rotation((yaw, pitch));
    // Still held while listeners run, so a handler can read the entity it is
    // told about. A cancelled spawn leaves it unspawned, as on Paper.
    if entity.as_living_entity().is_some()
        && let Some(server) = server()
    {
        let mut event = CreatureSpawnEvent::new(id, world.key.to_string(), x, y, z, reason);
        server.events.fire(&mut event);
        if event.is_cancelled() {
            return 0;
        }
    }
    if pending().lock().remove(&id).is_none() {
        return 0;
    }
    jboolean::from(world.try_add_entity(entity).is_ok())
}

/// Whether the entity was deserialized and has not been spawned yet.
extern "system" fn entity_is_pending(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    text(&mut env, &uuid)
        .and_then(|value| Uuid::parse_str(&value).ok())
        .map_or(0, |id| jboolean::from(pending().lock().contains_key(&id)))
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "serializeEntity",
            "(Ljava/lang/String;)[B",
            serialize_entity_native as *mut c_void,
        ),
        method(
            "deserializeEntity",
            "([BLjava/lang/String;Z)Ljava/lang/String;",
            deserialize_entity_native as *mut c_void,
        ),
        method(
            "entityIsPending",
            "(Ljava/lang/String;)Z",
            entity_is_pending as *mut c_void,
        ),
        method(
            "spawnPendingEntity",
            "(Ljava/lang/String;Ljava/lang/String;DDDFFLjava/lang/String;)Z",
            spawn_pending_entity as *mut c_void,
        ),
    ]
}
