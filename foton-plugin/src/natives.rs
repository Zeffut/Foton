//! What the Java side asks Foton, answered.
//!
//! Registered with `RegisterNatives` rather than found by symbol lookup: Foton
//! is one binary and its symbols are its own business, and a JVM searching a
//! statically linked executable for `Java_foton_Native_serverName` is a way to
//! discover at runtime that the linker discarded it.
//!
//! Every function here is called from a JVM thread, not from the game tick.
//! They take the same locks the rest of the server does, which is why they read
//! rather than write: a plugin changing the world from its own thread is a
//! problem the scheduler solves, not one to solve by taking a lock harder.

use std::ptr::null_mut;
use std::sync::{Arc, OnceLock, Weak};

use foton_core::permission::{PermissionExpr, PermissionKey};
use foton_core::player::Player;
use foton_core::server::Server;
use jni::JNIEnv;
use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::{jboolean, jobjectArray, jstring};
use uuid::Uuid;

/// The server the natives answer about.
///
/// A `static` because a JNI native is a bare function pointer with nowhere to
/// put context. `Weak` because the plugin host must never be the reason a
/// server cannot shut down.
static SERVER: OnceLock<Weak<Server>> = OnceLock::new();

/// Points the natives at a server. The first call wins.
pub(crate) fn bind(server: Weak<Server>) {
    let _ = SERVER.set(server);
}

/// The server, if there still is one.
fn server() -> Option<Arc<Server>> {
    SERVER.get().and_then(Weak::upgrade)
}

/// Resolves a Java-side handle back to a player who is still online.
fn player(env: &mut JNIEnv<'_>, uuid: &JString<'_>) -> Option<Arc<Player>> {
    let text: String = env.get_string(uuid).ok()?.into();
    let uuid = Uuid::parse_str(&text).ok()?;
    server()?.online_players().get_by_uuid(&uuid)
}

/// Returns a Java string, or Java's null when there is nothing to say.
fn to_java(env: &mut JNIEnv<'_>, value: Option<String>) -> jstring {
    value
        .and_then(|text| env.new_string(text).ok())
        .map_or_else(null_mut, JString::into_raw)
}

/// `foton.Native.serverName`
extern "system" fn server_name(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    to_java(&mut env, Some("Foton".to_owned()))
}

/// `foton.Native.serverVersion`
extern "system" fn server_version(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    to_java(&mut env, Some(env!("CARGO_PKG_VERSION").to_owned()))
}

/// `foton.Native.onlinePlayerIds`
extern "system" fn online_player_ids(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jobjectArray {
    let mut ids = Vec::new();
    if let Some(server) = server() {
        server.online_players().iter_players(|_uuid, player| {
            ids.push(player.gameprofile.id.to_string());
            true
        });
    }

    let Ok(empty) = env.new_string("") else {
        return null_mut();
    };
    let Ok(array) = env.new_object_array(
        i32::try_from(ids.len()).unwrap_or(0),
        "java/lang/String",
        &empty,
    ) else {
        return null_mut();
    };
    for (index, id) in ids.iter().enumerate() {
        let Ok(value) = env.new_string(id) else {
            continue;
        };
        let _ = env.set_object_array_element(&array, i32::try_from(index).unwrap_or(0), value);
    }
    let array: JObjectArray<'_> = array;
    array.into_raw()
}

/// `foton.Native.playerName`
extern "system" fn player_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let name = player(&mut env, &uuid).map(|player| player.gameprofile.name.clone());
    to_java(&mut env, name)
}

/// `foton.Native.playerWorld`
extern "system" fn player_world(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let key = player(&mut env, &uuid).map(|player| player.get_world().key.to_string());
    to_java(&mut env, key)
}

/// `foton.Native.sendMessage`
extern "system" fn send_message(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(text) = env.get_string(&message) else {
        return;
    };
    let text: String = text.into();
    player.send_message(&text.into());
}

/// `foton.Native.hasPermission`
extern "system" fn has_permission(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    permission: JString<'_>,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return u8::from(false);
    };
    let Ok(name) = env.get_string(&permission) else {
        return u8::from(false);
    };
    let name: String = name.into();
    // A key that does not parse is not a permission anyone holds, which is the
    // same answer as not holding it and a great deal calmer than a panic.
    let Ok(key) = PermissionKey::parse(name) else {
        return u8::from(false);
    };
    u8::from(player.has_permission(&PermissionExpr::key(key)))
}

/// Every native, with the descriptor the JVM matches it by.
///
/// A descriptor that disagrees with the Java declaration is not a compile
/// error on either side -- it is a `NoSuchMethodError` the first time a plugin
/// calls it, which is why they sit next to each other here.
pub(crate) fn bindings() -> Vec<jni::NativeMethod> {
    use std::ffi::c_void;

    fn method(name: &str, signature: &str, pointer: *mut c_void) -> jni::NativeMethod {
        jni::NativeMethod {
            name: name.into(),
            sig: signature.into(),
            fn_ptr: pointer,
        }
    }

    vec![
        method(
            "serverName",
            "()Ljava/lang/String;",
            server_name as *mut c_void,
        ),
        method(
            "serverVersion",
            "()Ljava/lang/String;",
            server_version as *mut c_void,
        ),
        method(
            "onlinePlayerIds",
            "()[Ljava/lang/String;",
            online_player_ids as *mut c_void,
        ),
        method(
            "playerName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_name as *mut c_void,
        ),
        method(
            "playerWorld",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_world as *mut c_void,
        ),
        method(
            "sendMessage",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            send_message as *mut c_void,
        ),
        method(
            "hasPermission",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            has_permission as *mut c_void,
        ),
    ]
}
