//! Resource packs a plugin pushes to a player, takes back, and asks about,
//! through Paper's `Player.sendResourcePacks` and friends.

use std::ffi::c_void;

use foton_protocol::packets::common::{CResourcePackPop, CResourcePackPush};
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint};
use uuid::Uuid;

use super::support::{component, method, player, text};

/// The longest URL a push packet carries.
///
/// Vanilla parity: `ByteBufCodecs.STRING_UTF8`, which is `stringUtf8(32767)`.
const MAX_PACK_URL_LENGTH: usize = 32767;

/// Asks the player's client to load a resource pack.
///
/// Paper parity: `CraftPlayer.sendResourcePacks`. False when the packet could
/// not be built: a packet that will not encode would drop the player instead.
extern "system" fn send_resource_pack(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    pack: JString<'_>,
    url: JString<'_>,
    hash: JString<'_>,
    required: jboolean,
    prompt: JString<'_>,
) -> jboolean {
    let (Some(id), Some(url), Some(hash)) = (
        text(&mut env, &pack).and_then(|pack| Uuid::parse_str(&pack).ok()),
        text(&mut env, &url),
        text(&mut env, &hash),
    ) else {
        return 0;
    };
    if url.len() > MAX_PACK_URL_LENGTH || hash.len() > CResourcePackPush::MAX_HASH_LENGTH {
        return 0;
    }
    let prompt = if prompt.is_null() {
        None
    } else if let Some(prompt) = component(&mut env, &prompt) {
        Some(prompt)
    } else {
        return 0;
    };
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    player.send_packet(CResourcePackPush {
        id,
        url,
        hash,
        required: required != 0,
        prompt,
    });
    1
}

/// Takes one resource pack, or every one the server sent, back from the client.
extern "system" fn remove_resource_packs(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    pack: JString<'_>,
) {
    let id = if pack.is_null() {
        None
    } else if let Some(id) = text(&mut env, &pack).and_then(|pack| Uuid::parse_str(&pack).ok()) {
        Some(id)
    } else {
        return;
    };
    if let Some(player) = player(&mut env, &uuid) {
        player.send_packet(CResourcePackPop { id });
    }
}

/// The ordinal of the client's last reported pack status, or -1 before it said any.
extern "system" fn resource_pack_status(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    player(&mut env, &uuid)
        .and_then(|player| player.resource_pack_status())
        .map_or(-1, |status| status as jint)
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "sendResourcePack",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;ZLjava/lang/String;)Z",
            send_resource_pack as *mut c_void,
        ),
        method(
            "removeResourcePacks",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            remove_resource_packs as *mut c_void,
        ),
        method(
            "resourcePackStatus",
            "(Ljava/lang/String;)I",
            resource_pack_status as *mut c_void,
        ),
    ]
}
