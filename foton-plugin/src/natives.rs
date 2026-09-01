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

use std::mem;
use std::ptr::null_mut;
use std::sync::{Arc, OnceLock, Weak};
use std::thread::{self, ThreadId};

use foton_core::entity::Entity;
use foton_core::inventory::container::Container;
use foton_core::permission::{PermissionExpr, PermissionKey};
use foton_core::player::Player;
use foton_core::server::Server;
use foton_core::world::LevelReader as _;
use foton_core::world::World;
use foton_protocol::packets::game::SoundSource;
use foton_registry::item_stack::ItemStack;
use foton_registry::{REGISTRY, RegistryExt as _};
use foton_utils::Identifier;
use foton_utils::locks::SyncMutex;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId};
use glam::DVec3;
use jni::JNIEnv;
use jni::objects::{JClass, JDoubleArray, JObjectArray, JString};
use jni::sys::{jboolean, jdouble, jdoubleArray, jfloat, jint, jlong, jobjectArray, jstring};
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

/// Returns a Java `String[]`, or null if the array could not be built.
fn string_array(env: &mut JNIEnv<'_>, values: &[String]) -> jobjectArray {
    let Ok(empty) = env.new_string("") else {
        return null_mut();
    };
    let Ok(array) = env.new_object_array(
        i32::try_from(values.len()).unwrap_or(0),
        "java/lang/String",
        &empty,
    ) else {
        return null_mut();
    };
    for (index, value) in values.iter().enumerate() {
        let Ok(text) = env.new_string(value) else {
            continue;
        };
        let _ = env.set_object_array_element(&array, i32::try_from(index).unwrap_or(0), text);
    }
    let array: JObjectArray<'_> = array;
    array.into_raw()
}

/// Returns a position as `{x, y, z, yaw, pitch}`, or Java's null.
///
/// One array rather than five calls. Five calls could each land on a different
/// tick, and a plugin that read x from one and z from the next would get a
/// point nothing was ever at.
fn to_position(env: &mut JNIEnv<'_>, at: Option<[f64; 5]>) -> jdoubleArray {
    let Some(at) = at else {
        return null_mut();
    };
    let Ok(array) = env.new_double_array(5) else {
        return null_mut();
    };
    if env.set_double_array_region(&array, 0, &at).is_err() {
        return null_mut();
    }
    let array: JDoubleArray<'_> = array;
    array.into_raw()
}

/// Resolves a world by the key a plugin holds it under.
fn world(env: &mut JNIEnv<'_>, name: &JString<'_>) -> Option<Arc<World>> {
    let text: String = env.get_string(name).ok()?.into();
    let key: Identifier = text.parse().ok()?;
    server()?.worlds.get(&key).map(Arc::clone)
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

    string_array(&mut env, &ids)
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

/// The thread the game tick runs on, learned from the tick itself.
///
/// A plugin may write a block only from that thread: `World::set_block` says
/// its callers must be inside Foton's serialized world-mutation phase, and a
/// JVM thread is not. Knowing which thread that is means a write from an event
/// handler or a scheduled task -- which is where nearly every write comes
/// from -- can happen at once and read back immediately, while a write from a
/// plugin's own thread waits for the next tick instead of racing the palette.
static TICK_THREAD: SyncMutex<Option<ThreadId>> = SyncMutex::new(None);

/// Block writes that arrived from somewhere other than the tick.
static DEFERRED: SyncMutex<Vec<(Identifier, BlockPos, BlockStateId)>> = SyncMutex::new(Vec::new());

/// Records that this is the tick thread, and runs what was waiting for it.
pub(crate) fn begin_tick(server: &Arc<Server>) {
    *TICK_THREAD.lock() = Some(thread::current().id());

    let pending = mem::take(&mut *DEFERRED.lock());
    for (world, pos, state) in pending {
        if let Some(world) = server.worlds.get(&world) {
            world.set_block(pos, state, UpdateFlags::UPDATE_ALL);
        }
    }
}

/// Whether the caller may write to the world right now.
fn on_tick() -> bool {
    *TICK_THREAD.lock() == Some(thread::current().id())
}

/// One inventory slot, written the way `foton.Native.inventorySlot` promises.
///
/// An empty slot is the empty string and an unreadable one is Java's null, so
/// a plugin can tell "there is nothing here" from "this cannot be answered"
/// rather than reading a missing armor slot as bare feet.
fn describe_slot(stack: &ItemStack) -> String {
    if stack.is_empty() {
        return String::new();
    }
    format!("{} {}", stack.item().key, stack.count())
}

/// Reads back what `describe_slot` wrote.
fn parse_slot(text: &str) -> Option<ItemStack> {
    let text = text.trim();
    if text.is_empty() {
        return Some(ItemStack::empty());
    }
    let (name, count) = text.rsplit_once(' ')?;
    let count: i32 = count.parse().ok()?;
    let key: Identifier = name.parse().ok()?;
    let item = REGISTRY.items.by_key(&key)?;
    (count > 0).then(|| ItemStack::with_count(item, count))
}

/// A block state as `minecraft:name[facing=north]`, the way `/setblock` writes it.
fn describe_state(state: BlockStateId) -> Option<String> {
    let block = REGISTRY.blocks.by_state_id(state)?;
    let properties = REGISTRY.blocks.get_properties(state);
    if properties.is_empty() {
        return Some(block.key.to_string());
    }
    let listed = properties
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("{}[{listed}]", block.key))
}

/// Reads back what `describe_state` wrote.
fn parse_state(text: &str) -> Option<BlockStateId> {
    let text = text.trim();
    let (name, rest) = match text.split_once('[') {
        Some((name, rest)) => (name, rest.strip_suffix(']')?),
        None => (text, ""),
    };
    let key: Identifier = name.parse().ok()?;
    let pairs: Vec<(&str, &str)> = if rest.is_empty() {
        Vec::new()
    } else {
        rest.split(',')
            .filter_map(|pair| pair.split_once('='))
            .map(|(name, value)| (name.trim(), value.trim()))
            .collect()
    };
    REGISTRY.blocks.state_id_from_properties(&key, &pairs)
}

/// `foton.Native.isOperator`
extern "system" fn is_operator(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    u8::from(player(&mut env, &uuid).is_some_and(|player| player.is_operator()))
}

/// `foton.Native.blockState`
extern "system" fn block_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let described = world(&mut env, &name)
        .and_then(|world| describe_state(world.get_block_state(BlockPos::new(x, y, z))));
    to_java(&mut env, described)
}

/// `foton.Native.setBlock`
extern "system" fn set_block(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    state: JString<'_>,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Ok(text) = env.get_string(&state) else {
        return;
    };
    let text: String = text.into();
    let Some(state) = parse_state(&text) else {
        return;
    };
    let pos = BlockPos::new(x, y, z);
    if on_tick() {
        world.set_block(pos, state, UpdateFlags::UPDATE_ALL);
    } else {
        // Off the tick. Writing here would race the palette, so it waits.
        DEFERRED.lock().push((world.key.clone(), pos, state));
    }
}

/// `foton.Native.playSound`
extern "system" fn play_sound(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    sound: JString<'_>,
    volume: jfloat,
    pitch: jfloat,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Ok(text) = env.get_string(&sound) else {
        return;
    };
    let text: String = text.into();
    let Ok(key) = text.parse::<Identifier>() else {
        return;
    };
    let Some(sound) = REGISTRY.sound_events.by_key(&key) else {
        return;
    };
    // Reading and broadcasting is safe from any thread: this sends packets and
    // touches no block state.
    world.play_sound_at(
        sound,
        SoundSource::Master,
        DVec3::new(x, y, z),
        volume,
        pitch,
        None,
    );
}

/// `foton.Native.gameMode`
extern "system" fn game_mode(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let mode = player(&mut env, &uuid).map(|player| format!("{:?}", player.game_mode()));
    to_java(&mut env, mode)
}

/// `foton.Native.inventorySlot`
extern "system" fn inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let described = player(&mut env, &uuid).and_then(|player| {
        let slot = usize::try_from(slot).ok()?;
        let inventory = player.inventory.lock();
        (slot < inventory.get_container_size()).then(|| describe_slot(inventory.get_item(slot)))
    });
    to_java(&mut env, described)
}

/// `foton.Native.setInventorySlot`
extern "system" fn set_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(text) = env.get_string(&item) else {
        return;
    };
    let text: String = text.into();
    let Some(stack) = parse_slot(&text) else {
        return;
    };
    let Ok(slot) = usize::try_from(slot) else {
        return;
    };
    let mut inventory = player.inventory.lock();
    if slot < inventory.get_container_size() {
        inventory.set_item(slot, stack);
    }
}

/// `foton.Native.heldSlot`
extern "system" fn held_slot(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    player(&mut env, &uuid).map_or(-1, |player| {
        jint::from(player.inventory.lock().get_selected_slot())
    })
}

/// `foton.Native.serverBrand`
extern "system" fn server_brand(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    let brand = format!(
        "Foton {} (MC: {})",
        env!("CARGO_PKG_VERSION"),
        foton_utils::MC_VERSION
    );
    to_java(&mut env, Some(brand))
}

/// `foton.Native.onlineMode`
extern "system" fn online_mode(_env: JNIEnv<'_>, _class: JClass<'_>) -> jboolean {
    u8::from(server().is_some_and(|server| server.config.online_mode))
}

/// `foton.Native.maxPlayers`
extern "system" fn max_players(_env: JNIEnv<'_>, _class: JClass<'_>) -> jint {
    server().map_or(0, |server| {
        i32::try_from(server.config.max_players).unwrap_or(i32::MAX)
    })
}

/// `foton.Native.playerIdByName`
extern "system" fn player_id_by_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jstring {
    let Ok(wanted) = env.get_string(&name) else {
        return null_mut();
    };
    let wanted: String = wanted.into();
    let found = server().and_then(|server| {
        let mut found = None;
        server.online_players().iter_players(|_uuid, player| {
            if player.gameprofile.name == wanted {
                found = Some(player.gameprofile.id.to_string());
                return false;
            }
            true
        });
        found
    });
    to_java(&mut env, found)
}

/// `foton.Native.broadcast`
extern "system" fn broadcast(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    message: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&message) else {
        return 0;
    };
    let text: String = text.into();
    let Some(server) = server() else {
        return 0;
    };
    let mut reached = 0;
    server.online_players().iter_players(|_uuid, player| {
        player.send_message(&text.clone().into());
        reached += 1;
        true
    });
    reached
}

/// `foton.Native.playerPosition`
extern "system" fn player_position(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let at = player(&mut env, &uuid).map(|player| {
        let position = player.position();
        let (yaw, pitch) = player.rotation();
        [
            position.x,
            position.y,
            position.z,
            f64::from(yaw),
            f64::from(pitch),
        ]
    });
    to_position(&mut env, at)
}

/// `foton.Native.worldNames`
extern "system" fn world_names(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jobjectArray {
    let names = server().map_or_else(Vec::new, |server| {
        server.worlds.keys().map(ToString::to_string).collect()
    });
    string_array(&mut env, &names)
}

/// `foton.Native.worldSpawn`
extern "system" fn world_spawn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jdoubleArray {
    let at = world(&mut env, &name).map(|world| {
        let spawn = world.level_data.read().data().spawn_pos();
        // The center of the block, which is where vanilla puts a player
        // standing on it rather than in its corner.
        [
            f64::from(spawn.0.x) + 0.5,
            f64::from(spawn.0.y),
            f64::from(spawn.0.z) + 0.5,
            0.0,
            0.0,
        ]
    });
    to_position(&mut env, at)
}

/// `foton.Native.worldTime`
extern "system" fn world_time(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>) -> jlong {
    world(&mut env, &name).map_or(-1, |world| world.game_time())
}

/// Every native, with the descriptor the JVM matches it by.
///
/// A descriptor that disagrees with the Java declaration is not a compile
/// error on either side -- it is a `NoSuchMethodError` the first time a plugin
/// calls it, which is why they sit next to each other here.
#[expect(
    clippy::too_many_lines,
    reason = "one flat list of every native and its descriptor; splitting it would               put a name and the signature it must match in different functions"
)]
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
            "serverBrand",
            "()Ljava/lang/String;",
            server_brand as *mut c_void,
        ),
        method("onlineMode", "()Z", online_mode as *mut c_void),
        method("maxPlayers", "()I", max_players as *mut c_void),
        method(
            "playerIdByName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_id_by_name as *mut c_void,
        ),
        method(
            "broadcast",
            "(Ljava/lang/String;)I",
            broadcast as *mut c_void,
        ),
        method(
            "playerPosition",
            "(Ljava/lang/String;)[D",
            player_position as *mut c_void,
        ),
        method(
            "worldNames",
            "()[Ljava/lang/String;",
            world_names as *mut c_void,
        ),
        method(
            "worldSpawn",
            "(Ljava/lang/String;)[D",
            world_spawn as *mut c_void,
        ),
        method(
            "worldTime",
            "(Ljava/lang/String;)J",
            world_time as *mut c_void,
        ),
        method(
            "gameMode",
            "(Ljava/lang/String;)Ljava/lang/String;",
            game_mode as *mut c_void,
        ),
        method(
            "inventorySlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            inventory_slot as *mut c_void,
        ),
        method(
            "setInventorySlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_inventory_slot as *mut c_void,
        ),
        method(
            "heldSlot",
            "(Ljava/lang/String;)I",
            held_slot as *mut c_void,
        ),
        method(
            "isOperator",
            "(Ljava/lang/String;)Z",
            is_operator as *mut c_void,
        ),
        method(
            "blockState",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            block_state as *mut c_void,
        ),
        method(
            "setBlock",
            "(Ljava/lang/String;IIILjava/lang/String;)V",
            set_block as *mut c_void,
        ),
        method(
            "playSound",
            "(Ljava/lang/String;DDDLjava/lang/String;FF)V",
            play_sound as *mut c_void,
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
