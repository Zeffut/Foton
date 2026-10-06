//! Bukkit persistent data containers, stored on the entity, player or chunk
//! that owns them so they are saved and loaded with it.
//!
//! The container crosses the bridge as a binary NBT root compound -- what
//! `Snbt.toBinary` writes -- and an empty `BukkitValues` is an empty compound.

use std::ffi::c_void;
use std::io::Cursor;
use std::ptr::null_mut;

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jboolean, jbyteArray, jint};
use simdnbt::owned::{Nbt, NbtCompound, read as read_nbt};

use super::support::{entity, method, world};

fn encode(compound: NbtCompound) -> Vec<u8> {
    let mut bytes = Vec::new();
    Nbt::new("".into(), compound).write(&mut bytes);
    bytes
}

fn decode(env: &JNIEnv<'_>, data: &JByteArray<'_>) -> Option<NbtCompound> {
    let bytes = env.convert_byte_array(data).ok()?;
    match read_nbt(&mut Cursor::new(bytes.as_slice())).ok()? {
        Nbt::Some(root) => Some(root.as_compound()),
        Nbt::None => None,
    }
}

fn to_java(env: &JNIEnv<'_>, bytes: &[u8]) -> jbyteArray {
    env.byte_array_from_slice(bytes)
        .map_or_else(|_| null_mut(), JByteArray::into_raw)
}

/// `foton.Native.entityPersistentData`: null when the entity no longer exists.
extern "system" fn entity_persistent_data(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jbyteArray {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return null_mut();
    };
    to_java(&env, &encode(entity.base().bukkit_values()))
}

/// `foton.Native.setEntityPersistentData`
extern "system" fn set_entity_persistent_data(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    data: JByteArray<'_>,
) -> jboolean {
    let Some(values) = decode(&env, &data) else {
        return 0;
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return 0;
    };
    entity.base().set_bukkit_values(values);
    1
}

/// `foton.Native.chunkPersistentData`: null when the chunk is not loaded.
extern "system" fn chunk_persistent_data(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    z: jint,
) -> jbyteArray {
    let values = world(&mut env, &name).and_then(|world| world.chunk_bukkit_values(x, z));
    values.map_or_else(null_mut, |values| to_java(&env, &encode(values)))
}

/// `foton.Native.setChunkPersistentData`: false when the chunk is not loaded.
extern "system" fn set_chunk_persistent_data(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    z: jint,
    data: JByteArray<'_>,
) -> jboolean {
    let Some(values) = decode(&env, &data) else {
        return 0;
    };
    jboolean::from(
        world(&mut env, &name).is_some_and(|world| world.set_chunk_bukkit_values(x, z, values)),
    )
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "entityPersistentData",
            "(Ljava/lang/String;)[B",
            entity_persistent_data as *mut c_void,
        ),
        method(
            "setEntityPersistentData",
            "(Ljava/lang/String;[B)Z",
            set_entity_persistent_data as *mut c_void,
        ),
        method(
            "chunkPersistentData",
            "(Ljava/lang/String;II)[B",
            chunk_persistent_data as *mut c_void,
        ),
        method(
            "setChunkPersistentData",
            "(Ljava/lang/String;II[B)Z",
            set_chunk_persistent_data as *mut c_void,
        ),
    ]
}
