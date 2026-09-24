//! Argument plumbing shared by the entity, player and world natives.
//!
//! Every native here receives Java strings and answers "nothing" when what
//! they name no longer exists: Bukkit's contract for a stale handle is an
//! object that quietly does nothing, and plugins rely on it.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::Arc;

use foton_core::entity::SharedEntity;
use foton_core::world::World;
use foton_utils::Identifier;
use jni::JNIEnv;
use jni::objects::{JDoubleArray, JString};
use jni::sys::jdoubleArray;
use uuid::Uuid;

use super::entity_by_uuid;

/// A registered native, in the shape `RegisterNatives` wants.
pub(super) fn method(name: &str, signature: &str, pointer: *mut c_void) -> jni::NativeMethod {
    jni::NativeMethod {
        name: name.into(),
        sig: signature.into(),
        fn_ptr: pointer,
    }
}

/// A Java string, or `None` for null or an unreadable one.
pub(super) fn text(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Option<String> {
    if value.is_null() {
        return None;
    }
    env.get_string(value).ok().map(Into::into)
}

/// A Java string parsed as a registry key; a bare path is in `minecraft:`.
pub(super) fn key(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Option<Identifier> {
    text(env, value)?.parse().ok()
}

/// The entity a Java handle names, with the world it is in, if it still exists.
pub(super) fn entity(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
) -> Option<(Arc<World>, SharedEntity)> {
    let id = Uuid::parse_str(&text(env, uuid)?).ok()?;
    entity_by_uuid(&id)
}

/// A Java `double[]`, or null when there is nothing to answer.
pub(super) fn doubles(env: &mut JNIEnv<'_>, values: Option<&[f64]>) -> jdoubleArray {
    let Some(values) = values else {
        return null_mut();
    };
    let Ok(length) = i32::try_from(values.len()) else {
        return null_mut();
    };
    let Ok(array) = env.new_double_array(length) else {
        return null_mut();
    };
    if env.set_double_array_region(&array, 0, values).is_err() {
        return null_mut();
    }
    let array: JDoubleArray<'_> = array;
    array.into_raw()
}
