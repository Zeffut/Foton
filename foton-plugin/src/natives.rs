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

use foton_core::boss_event::ServerBossEvent;
use foton_core::chunk::{chunk_request::{ChunkRequestHandle, ChunkTicketKind}, status::ChunkStatus};
use foton_core::entity::{Entity, LivingEntity as _};
use foton_core::inventory::container::Container;
use foton_core::permission::{PermissionExpr, PermissionKey};
use foton_core::player::Player;
use foton_core::server::Server;
use foton_core::world::LevelReader as _;
use foton_core::world::World;
use foton_protocol::packets::common::CCustomPayload;
use foton_protocol::packets::game::{
    BossBarColor, BossBarOverlay, CClearTitles, COpenBook, CSetSubtitleText, CSetTitleText,
    CSetTitlesAnimation, CStopSound, CSystemChat, CTabList, SoundSource,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::{REGISTRY, RegistryExt as _, vanilla_items};
use foton_utils::Identifier;
use foton_utils::locks::{SyncMutex, SyncRwLock};
use foton_utils::types::InteractionHand;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId};
use glam::DVec3;
use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JDoubleArray, JObjectArray, JString};
use jni::sys::{jboolean, jdouble, jdoubleArray, jfloat, jint, jlong, jobjectArray, jstring};
use rustc_hash::FxHashMap;
use text_components::TextComponent;
use uuid::Uuid;

/// The server the natives answer about.
///
/// A `static` because a JNI native is a bare function pointer with nowhere to
/// put context. `Weak` because the plugin host must never be the reason a
/// server cannot shut down.
static SERVER: OnceLock<Weak<Server>> = OnceLock::new();

/// Outstanding asynchronous Bukkit chunk requests, retained until Full status.
static CHUNK_REQUESTS: OnceLock<SyncMutex<FxHashMap<Uuid, ChunkRequestHandle>>> = OnceLock::new();

/// Non-persistent bars created through Bukkit, keyed by the opaque handle Java owns.
static BOSS_BARS: OnceLock<SyncRwLock<FxHashMap<Uuid, Arc<ServerBossEvent>>>> = OnceLock::new();

/// Bukkit updates header and footer independently, while the protocol packet carries both.
static PLAYER_TAB_LISTS: OnceLock<SyncRwLock<FxHashMap<Uuid, (String, String)>>> = OnceLock::new();

/// Points the natives at a server. The first call wins.
pub(crate) fn bind(server: Weak<Server>) {
    let _ = SERVER.set(server);
}

/// The server, if there still is one.
fn server() -> Option<Arc<Server>> {
    SERVER.get().and_then(Weak::upgrade)
}

fn chunk_requests() -> &'static SyncMutex<FxHashMap<Uuid, ChunkRequestHandle>> {
    CHUNK_REQUESTS.get_or_init(|| SyncMutex::new(FxHashMap::default()))
}

fn boss_bars() -> &'static SyncRwLock<FxHashMap<Uuid, Arc<ServerBossEvent>>> {
    BOSS_BARS.get_or_init(|| SyncRwLock::new(FxHashMap::default()))
}

fn player_tab_lists() -> &'static SyncRwLock<FxHashMap<Uuid, (String, String)>> {
    PLAYER_TAB_LISTS.get_or_init(|| SyncRwLock::new(FxHashMap::default()))
}

fn boss_bar(env: &mut JNIEnv<'_>, id: &JString<'_>) -> Option<Arc<ServerBossEvent>> {
    let text: String = env.get_string(id).ok()?.into();
    let id = Uuid::parse_str(&text).ok()?;
    boss_bars().read().get(&id).map(Arc::clone)
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

/// `foton.Native.playerLocale`
extern "system" fn player_locale(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jstring {
    let locale = player(&mut env, &uuid).map(|player| player.client_information().language);
    to_java(&mut env, locale)
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

fn entity_by_uuid(uuid: &Uuid) -> Option<(Arc<World>, foton_core::entity::SharedEntity)> {
    let server = server()?;
    for world in server.worlds.values() {
        if let Some(entity) = world.get_entity_by_uuid(uuid) {
            return Some((Arc::clone(world), entity));
        }
    }
    None
}

extern "system" fn remove_entity(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    let Ok(text) = env.get_string(&uuid) else { return; };
    let Ok(id) = text.to_str().ok().and_then(|value| value.parse::<Uuid>().ok()).ok_or(()) else { return; };
    let Some((world, entity)) = entity_by_uuid(&id) else { return; };
    let _ = world.remove_entity(entity.id());
}

extern "system" fn entity_world(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jstring {
    let text: String = match env.get_string(&uuid) { Ok(v) => v.into(), Err(_) => return to_java(&mut env, None) };
    let Some(id) = Uuid::parse_str(&text).ok() else { return to_java(&mut env, None); };
    to_java(&mut env, entity_by_uuid(&id).map(|(world, _)| world.key.to_string()))
}

extern "system" fn entity_type(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jstring {
    let text: String = match env.get_string(&uuid) { Ok(v) => v.into(), Err(_) => return to_java(&mut env, None) };
    let Some(id) = Uuid::parse_str(&text).ok() else { return to_java(&mut env, None); };
    to_java(&mut env, entity_by_uuid(&id).map(|(_, entity)| entity.entity_type().key.path.to_string()))
}

extern "system" fn entity_spawn_category(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jstring {
    let Ok(text) = env.get_string(&uuid) else { return null_mut(); };
    let Ok(id) = Uuid::parse_str(match text.to_str() { Ok(value) => value, Err(_) => return null_mut() }) else { return null_mut(); };
    let Some((_world, entity)) = entity_by_uuid(&id) else { return null_mut(); };
    let category = format!("{:?}", entity.entity_type().mob_category);
    to_java(&mut env, Some(category))
}

extern "system" fn entity_position(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jdoubleArray {
    let text: String = match env.get_string(&uuid) { Ok(v) => v.into(), Err(_) => return to_position(&mut env, None) };
    let Some(id) = Uuid::parse_str(&text).ok() else { return to_position(&mut env, None); };
    to_position(&mut env, entity_by_uuid(&id).map(|(_, entity)| { let p = entity.position(); [p.x, p.y, p.z, 0.0, 0.0] }))
}

extern "system" fn entity_id(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let text: String = match env.get_string(&uuid) { Ok(v) => v.into(), Err(_) => return -1 };
    let Some(id) = Uuid::parse_str(&text).ok() else { return -1; };
    entity_by_uuid(&id).map_or(-1, |(_, entity)| entity.id())
}

extern "system" fn entity_custom_name(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jstring {
    let text: String = match env.get_string(&uuid) { Ok(v) => v.into(), Err(_) => return to_java(&mut env, None) };
    let Some(id) = Uuid::parse_str(&text).ok() else { return to_java(&mut env, None); };
    to_java(&mut env, entity_by_uuid(&id).and_then(|(_, entity)| entity.custom_name().map(|name| name.to_string())))
}

extern "system" fn set_entity_custom_name(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>, name: JString<'_>) {
    let Ok(uuid_text) = env.get_string(&uuid) else { return; };
    let Ok(name_text) = env.get_string(&name) else { return; };
    let Some(id) = Uuid::parse_str(&String::from(uuid_text)).ok() else { return; };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_custom_name(Some(text_components::TextComponent::plain(String::from(name_text))));
    }
}

extern "system" fn entity_send_message(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>, message: JString<'_>) {
    let Ok(uuid_text) = env.get_string(&uuid) else { return; };
    let Ok(message_text) = env.get_string(&message) else { return; };
    let Some(id) = Uuid::parse_str(&String::from(uuid_text)).ok() else { return; };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(player) = entity.as_player() {
            player.send_message(&text_components::TextComponent::plain(String::from(message_text)));
        }
    }
}

extern "system" fn has_played_before(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let text: String = text.into();
    let Ok(uuid) = Uuid::parse_str(&text) else {
        return 0;
    };
    u8::from(server().is_some_and(|server| server.known_players().by_uuid(uuid).is_some()))
}

/// `foton.Native.customName`
extern "system" fn custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let name = player(&mut env, &uuid)
        .and_then(|player| player.custom_name().map(|name| name.to_string()));
    to_java(&mut env, name)
}

/// `foton.Native.setCustomName`
extern "system" fn set_custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(name) = env.get_string(&name) else {
        return;
    };
    let name = String::from(name);
    player.set_custom_name((!name.is_empty()).then(|| TextComponent::from(name)));
}

/// `foton.Native.health`
extern "system" fn player_food_level(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    player(&mut env, &uuid).map_or(20, |player| player.food_data.lock().food_level)
}

extern "system" fn health(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jdouble {
    player(&mut env, &uuid).map_or(0.0, |player| f64::from(player.get_health()))
}

/// `foton.Native.setHealth`
extern "system" fn set_health(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    health: jdouble,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.set_health(health as f32);
    }
}

/// `foton.Native.maxHealth`
extern "system" fn max_health(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdouble {
    player(&mut env, &uuid).map_or(20.0, |player| f64::from(player.get_max_health()))
}

/// `foton.Native.playerRespawnWorld`
extern "system" fn player_respawn_world(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jstring {
    let world = player(&mut env, &uuid).and_then(|player| player.respawn_config()).map(|config| config.respawn_data.dimension().to_string());
    to_java(&mut env, world)
}

/// `foton.Native.playerRespawnPosition`
extern "system" fn player_respawn_position(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jdoubleArray {
    let position = player(&mut env, &uuid).and_then(|player| player.respawn_config()).map(|config| {
        let pos = config.respawn_data.pos();
        [f64::from(pos.x()) + 0.5, f64::from(pos.y()), f64::from(pos.z()) + 0.5, f64::from(config.respawn_data.yaw), f64::from(config.respawn_data.pitch)]
    });
    to_position(&mut env, position)
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

/// `foton.Native.kickPlayer`
extern "system" fn kick_player(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(message) = env.get_string(&message) else {
        return;
    };
    player.disconnect(String::from(message));
}

fn set_player_tab_list(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    header: Option<JString<'_>>,
    footer: Option<JString<'_>>,
) {
    let Ok(id_text) = env.get_string(uuid) else {
        return;
    };
    let Ok(id) = String::from(id_text).parse::<Uuid>() else {
        return;
    };
    let Some(player) = player(env, uuid) else {
        return;
    };
    let mut lists = player_tab_lists().write();
    let entry = lists
        .entry(id)
        .or_insert_with(|| (String::new(), String::new()));
    if let Some(header) = header {
        let Ok(value) = env.get_string(&header) else {
            return;
        };
        entry.0 = String::from(value);
    }
    if let Some(footer) = footer {
        let Ok(value) = env.get_string(&footer) else {
            return;
        };
        entry.1 = String::from(value);
    }
    let header: TextComponent = entry.0.clone().into();
    let footer: TextComponent = entry.1.clone().into();
    player.send_packet(CTabList::new(&header, &footer, player.as_ref()));
}

/// `foton.Native.setPlayerListHeader`
extern "system" fn set_player_list_header(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    header: JString<'_>,
) {
    set_player_tab_list(&mut env, &uuid, Some(header), None);
}

/// `foton.Native.setPlayerListFooter`
extern "system" fn set_player_list_footer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    footer: JString<'_>,
) {
    set_player_tab_list(&mut env, &uuid, None, Some(footer));
}

/// `foton.Native.setPlayerListHeaderFooter`
extern "system" fn set_player_list_header_footer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    header: JString<'_>,
    footer: JString<'_>,
) {
    set_player_tab_list(&mut env, &uuid, Some(header), Some(footer));
}

/// `foton.Native.sendActionBar`
extern "system" fn send_action_bar(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(message) = env.get_string(&message) else {
        return;
    };
    let message: TextComponent = String::from(message).into();
    player.send_packet(CSystemChat::new(&message, true, player.as_ref()));
}

/// `foton.Native.sendTitle`
extern "system" fn send_title(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    title: JString<'_>,
    subtitle: JString<'_>,
    fade_in: jint,
    stay: jint,
    fade_out: jint,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(title) = env.get_string(&title) else {
        return;
    };
    let Ok(subtitle) = env.get_string(&subtitle) else {
        return;
    };
    let title: TextComponent = String::from(title).into();
    let subtitle: TextComponent = String::from(subtitle).into();
    player.send_packet(CSetTitlesAnimation {
        fade_in,
        stay,
        fade_out,
    });
    player.send_packet(CSetTitleText::new(&title, player.as_ref()));
    player.send_packet(CSetSubtitleText::new(&subtitle, player.as_ref()));
}

/// `foton.Native.clearTitle`
extern "system" fn clear_title(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    player.send_packet(CClearTitles { reset_times: true });
}

/// `foton.Native.sendPluginMessage`
extern "system" fn send_plugin_message(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    channel: JString<'_>,
    message: JByteArray<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(channel) = env.get_string(&channel).map(String::from) else {
        return;
    };
    let Ok(channel) = channel.parse::<Identifier>() else {
        return;
    };
    let Ok(message) = env.convert_byte_array(&message) else {
        return;
    };
    player.send_packet(CCustomPayload::new(channel, message.into_boxed_slice()));
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

/// `foton.Native.isPrimaryThread`
extern "system" fn is_primary_thread(_env: JNIEnv<'_>, _class: JClass<'_>) -> jboolean {
    u8::from(on_tick())
}

/// `foton.Native.experienceLevel`
extern "system" fn experience_level(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let Some(player) = player(&mut env, &uuid) else { return 0 };
    player.experience.lock().level()
}

/// `foton.Native.savePlayers`
extern "system" fn save_players(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(server) = server() { server.request_save_players(); }
}

/// `foton.Native.shutdown`
extern "system" fn shutdown(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(server) = server() {
        server.cancel_token.cancel();
    }
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

extern "system" fn is_permission_set(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    permission: JString<'_>,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let Ok(permission) = env.get_string(&permission) else {
        return 0;
    };
    let permission: String = permission.into();
    let Ok(key) = PermissionKey::parse(permission) else {
        return 0;
    };
    u8::from(player.permission_state(&PermissionExpr::key(key)).is_some())
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

/// `foton.Native.playSoundCategory`
extern "system" fn play_sound_category(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    sound: JString<'_>,
    category: JString<'_>,
    volume: jfloat,
    pitch: jfloat,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Ok(sound) = env.get_string(&sound) else {
        return;
    };
    let Ok(category) = env.get_string(&category) else {
        return;
    };
    let Ok(key) = String::from(sound).parse::<Identifier>() else {
        return;
    };
    let Some(sound) = REGISTRY.sound_events.by_key(&key) else {
        return;
    };
    let Ok(category) = category.to_str() else {
        return;
    };
    let source = match category {
        "MASTER" => SoundSource::Master,
        "MUSIC" => SoundSource::Music,
        "RECORDS" => SoundSource::Records,
        "WEATHER" => SoundSource::Weather,
        "BLOCKS" => SoundSource::Blocks,
        "HOSTILE" => SoundSource::Hostile,
        "NEUTRAL" => SoundSource::Neutral,
        "PLAYERS" => SoundSource::Players,
        "AMBIENT" => SoundSource::Ambient,
        "VOICE" => SoundSource::Voice,
        "UI" => SoundSource::Ui,
        _ => return,
    };
    world.play_sound_at(sound, source, DVec3::new(x, y, z), volume, pitch, None);
}

/// `foton.Native.stopSound`
extern "system" fn stop_sound(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    sound: JString<'_>,
    category: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(sound) = env.get_string(&sound) else {
        return;
    };
    let Ok(category) = env.get_string(&category) else {
        return;
    };
    let sound: String = sound.into();
    let sound = if sound.is_empty() {
        None
    } else {
        sound.parse::<Identifier>().ok()
    };
    let source = match category.to_str().ok() {
        Some("MASTER") => Some(SoundSource::Master),
        Some("MUSIC") => Some(SoundSource::Music),
        Some("RECORDS") => Some(SoundSource::Records),
        Some("WEATHER") => Some(SoundSource::Weather),
        Some("BLOCKS") => Some(SoundSource::Blocks),
        Some("HOSTILE") => Some(SoundSource::Hostile),
        Some("NEUTRAL") => Some(SoundSource::Neutral),
        Some("PLAYERS") => Some(SoundSource::Players),
        Some("AMBIENT") => Some(SoundSource::Ambient),
        Some("VOICE") => Some(SoundSource::Voice),
        Some("UI") => Some(SoundSource::Ui),
        Some("") => None,
        _ => return,
    };
    player.send_packet(CStopSound { sound, source });
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

/// `foton.Native.createBossBar`
extern "system" fn create_boss_bar(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    title: JString<'_>,
    color: jint,
    style: jint,
    flags: jint,
) -> jstring {
    let Ok(title) = env.get_string(&title).map(String::from) else {
        return null_mut();
    };
    let Some(color) = usize::try_from(color)
        .ok()
        .and_then(|index| BossBarColor::VALUES.get(index).copied())
    else {
        return null_mut();
    };
    let Some(style) = usize::try_from(style)
        .ok()
        .and_then(|index| BossBarOverlay::VALUES.get(index).copied())
    else {
        return null_mut();
    };
    let bar = Arc::new(ServerBossEvent::with_random_id(
        TextComponent::from(title),
        color,
        style,
    ));
    bar.set_darken_screen(flags & 1 != 0);
    bar.set_play_boss_music(flags & 2 != 0);
    bar.set_create_world_fog(flags & 4 != 0);
    let id = bar.id();
    boss_bars().write().insert(id, bar);
    to_java(&mut env, Some(id.to_string()))
}

extern "system" fn release_boss_bar(mut env: JNIEnv<'_>, _class: JClass<'_>, id: JString<'_>) {
    let Ok(text) = env.get_string(&id).map(String::from) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(&text) else {
        return;
    };
    if let Some(bar) = boss_bars().write().remove(&id) {
        bar.remove_all_players();
    }
}

extern "system" fn boss_bar_set_title(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    title: JString<'_>,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    let Ok(title) = env.get_string(&title).map(String::from) else {
        return;
    };
    bar.set_name(TextComponent::from(title));
}

extern "system" fn boss_bar_set_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    color: jint,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    let Some(color) = usize::try_from(color)
        .ok()
        .and_then(|index| BossBarColor::VALUES.get(index).copied())
    else {
        return;
    };
    bar.set_color(color);
}

extern "system" fn boss_bar_set_style(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    style: jint,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    let Some(style) = usize::try_from(style)
        .ok()
        .and_then(|index| BossBarOverlay::VALUES.get(index).copied())
    else {
        return;
    };
    bar.set_overlay(style);
}

extern "system" fn boss_bar_set_flags(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    flags: jint,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    bar.set_darken_screen(flags & 1 != 0);
    bar.set_play_boss_music(flags & 2 != 0);
    bar.set_create_world_fog(flags & 4 != 0);
}

extern "system" fn boss_bar_set_progress(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    progress: jdouble,
) {
    if let Some(bar) = boss_bar(&mut env, &id) {
        bar.set_progress(progress as f32);
    }
}

extern "system" fn boss_bar_add_player(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    player_id: JString<'_>,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    if let Some(player) = player(&mut env, &player_id) {
        bar.add_player(&player);
    }
}

extern "system" fn boss_bar_remove_player(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    player_id: JString<'_>,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    if let Some(player) = player(&mut env, &player_id) {
        bar.remove_player(&player);
    }
}

extern "system" fn boss_bar_remove_all(mut env: JNIEnv<'_>, _class: JClass<'_>, id: JString<'_>) {
    if let Some(bar) = boss_bar(&mut env, &id) {
        bar.remove_all_players();
    }
}

extern "system" fn boss_bar_player_ids(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
) -> jobjectArray {
    let ids = boss_bar(&mut env, &id).map_or_else(Vec::new, |bar| {
        bar.players()
            .into_iter()
            .map(|player| player.uuid().to_string())
            .collect()
    });
    string_array(&mut env, &ids)
}

extern "system" fn boss_bar_set_visible(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    visible: jboolean,
) {
    if let Some(bar) = boss_bar(&mut env, &id) {
        bar.set_visible(visible != 0);
    }
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

/// `foton.Native.worldPlayerIds`
extern "system" fn world_player_ids(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jobjectArray {
    let ids = world(&mut env, &name).map_or_else(Vec::new, |world| {
        let mut ids = Vec::new();
        world.players.iter_players(|_uuid, player| {
            ids.push(player.gameprofile.id.to_string());
            true
        });
        ids
    });
    string_array(&mut env, &ids)
}

/// `foton.Native.worldEntityIds`
extern "system" fn world_entity_ids(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jobjectArray {
    let ids = world(&mut env, &name).map_or_else(Vec::new, |world| {
        world
            .accessible_entities()
            .into_iter()
            .map(|entity| entity.uuid().to_string())
            .collect()
    });
    string_array(&mut env, &ids)
}

/// `foton.Native.requestChunk`
extern "system" fn request_chunk(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>, x: jint, z: jint) -> jstring {
    let Some(world) = world(&mut env, &name) else { return null_mut(); };
    let pos = foton_utils::ChunkPos::new(x, z);
    let handle = world.chunk_map.request_chunk(pos, ChunkStatus::Full, ChunkTicketKind::Command);
    let id = Uuid::new_v4();
    chunk_requests().lock().insert(id, handle);
    to_java(&mut env, Some(id.to_string()))
}

/// `foton.Native.chunkRequestReady`
extern "system" fn chunk_request_ready(mut env: JNIEnv<'_>, _class: JClass<'_>, id: JString<'_>) -> jboolean {
    let Ok(text) = env.get_string(&id) else { return 0; };
    let Ok(text) = text.to_str() else { return 0; };
    let Ok(id) = Uuid::parse_str(text) else { return 0; };
    let mut requests = chunk_requests().lock();
    let Some(handle) = requests.get(&id) else { return 0; };
    match handle.poll() {
        foton_core::chunk::chunk_request::ChunkRequestState::Ready => { requests.remove(&id); 1 }
        foton_core::chunk::chunk_request::ChunkRequestState::Cancelled => { requests.remove(&id); 0 }
        foton_core::chunk::chunk_request::ChunkRequestState::Pending { .. } => 0,
    }
}

/// `foton.Native.worldChunkLoaded`
extern "system" fn world_chunk_loaded(
    mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>, x: jint, z: jint,
) -> jboolean {
    world(&mut env, &name).is_some_and(|world| world.is_chunk_loaded(x, z)) as jboolean
}

/// `foton.Native.worldLoadedChunkCoords`
extern "system" fn world_loaded_chunk_coords(
    mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>,
) -> jobjectArray {
    let coords = world(&mut env, &name).map_or_else(Vec::new, |world| {
        world.loaded_chunk_positions().into_iter()
            .map(|pos| format!("{},{}", pos.0.x, pos.0.y)).collect()
    });
    string_array(&mut env, &coords)
}

/// `foton.Native.worldDropItem`
extern "system" fn world_drop_item(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>, x: jdouble, y: jdouble, z: jdouble, item: JString<'_>) -> jstring {
    let Some(world) = world(&mut env, &name) else { return null_mut(); };
    let Ok(item) = env.get_string(&item) else { return null_mut(); };
    let Ok(item) = item.to_str() else { return null_mut(); };
    let Some(stack) = parse_slot(item) else { return null_mut(); };
    let Some(entity) = world.spawn_item(glam::DVec3::new(x, y, z), stack) else { return null_mut(); };
    to_java(&mut env, Some(entity.uuid().to_string()))
}

/// `foton.Native.worldAutoSave`
extern "system" fn world_auto_save(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>) -> jboolean {
    world(&mut env, &name).is_some_and(|world| world.is_auto_save()) as jboolean
}

/// `foton.Native.setWorldAutoSave`
extern "system" fn set_world_auto_save(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>, value: jboolean) {
    if let Some(world) = world(&mut env, &name) { world.set_auto_save(value != 0); }
}

/// `foton.Native.saveWorld`
extern "system" fn save_world(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>) {
    if let Some(world) = world(&mut env, &name) { world.request_save(); }
}

/// `foton.Native.worldFolder`
extern "system" fn world_folder(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>) -> jstring {
    let Some(path) = world(&mut env, &name).and_then(|world| world.world_folder()) else { return null_mut(); };
    let value = path.to_string_lossy();
    let Ok(value) = env.new_string(value.as_ref()) else { return null_mut(); };
    value.into_raw()
}

/// `foton.Native.scoreboardTeamEntries`
extern "system" fn scoreboard_team_entries(
    mut env: JNIEnv<'_>, _class: JClass<'_>, world_name: JString<'_>, team_name: JString<'_>,
) -> jobjectArray {
    let Ok(world_name): Result<String, _> = env.get_string(&world_name).map(Into::into) else { return string_array(&mut env, &[]) };
    let Ok(team_name): Result<String, _> = env.get_string(&team_name).map(Into::into) else { return string_array(&mut env, &[]) };
    let entries = server().and_then(|server| {
        let key: Identifier = world_name.parse().ok()?;
        let world = server.worlds.get(&key).map(Arc::clone)?;
        server.scoreboards.get(world.domain()).map(|scoreboard| {
            scoreboard.team(&team_name).map(|team| scoreboard.team_entries(&team)).unwrap_or_default()
        })
    }).unwrap_or_default();
    string_array(&mut env, &entries)
}

/// `foton.Native.scoreboardEntryTeam`
extern "system" fn scoreboard_entry_team(
    mut env: JNIEnv<'_>, _class: JClass<'_>, world_name: JString<'_>, entry: JString<'_>,
) -> jstring {
    let Ok(world_name): Result<String, _> = env.get_string(&world_name).map(Into::into) else { return null_mut() };
    let Ok(entry): Result<String, _> = env.get_string(&entry).map(Into::into) else { return null_mut() };
    let team = server().and_then(|server| {
        let key: Identifier = world_name.parse().ok()?;
        let world = server.worlds.get(&key).map(Arc::clone)?;
        server.scoreboards.get(world.domain()).and_then(|scoreboard| {
            scoreboard.holder_team_name(&foton_core::scoreboard::ScoreHolder::new(entry))
        })
    });
    to_java(&mut env, team)
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

extern "system" fn world_min_height(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jint {
    world(&mut env, &name).map_or(0, |world| world.get_min_y())
}

extern "system" fn world_max_height(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jint {
    world(&mut env, &name).map_or(0, |world| world.max_build_height())
}

extern "system" fn is_sneaking(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    jboolean::from(player(&mut env, &uuid).is_some_and(|player| player.is_crouching()))
}

extern "system" fn open_book(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let inventory = player.inventory.lock();
    let hand = if inventory
        .get_item_in_hand(InteractionHand::MainHand)
        .is(&vanilla_items::WRITTEN_BOOK)
        || inventory
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::WRITABLE_BOOK)
    {
        InteractionHand::MainHand
    } else if inventory
        .get_item_in_hand(InteractionHand::OffHand)
        .is(&vanilla_items::WRITTEN_BOOK)
        || inventory
            .get_item_in_hand(InteractionHand::OffHand)
            .is(&vanilla_items::WRITABLE_BOOK)
    {
        InteractionHand::OffHand
    } else {
        return;
    };
    drop(inventory);
    player.send_packet(COpenBook { hand });
}

extern "system" fn teleport_entity(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>, world_name: JString<'_>, x: jdouble, y: jdouble, z: jdouble, yaw: jfloat, pitch: jfloat) -> jboolean {
    let Ok(world_name) = env.get_string(&world_name) else { return 0; };
    let Ok(text) = env.get_string(&uuid) else { return 0; };
    let Ok(text) = text.to_str() else { return 0; };
    let Ok(id) = Uuid::parse_str(text) else { return 0; };
    let Some((world, entity)) = entity_by_uuid(&id) else { return 0; };
    if world.key.to_string() != String::from(world_name) { return 0; }
    if entity.try_set_position(DVec3::new(x, y, z)).is_err() { return 0; }
    entity.set_rotation((yaw, pitch));
    1
}

extern "system" fn teleport(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    world_name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    yaw: jfloat,
    pitch: jfloat,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let Ok(world_name) = env.get_string(&world_name) else {
        return 0;
    };
    if player.get_world().key.to_string() != String::from(world_name) {
        return 0;
    }
    u8::from(player.teleport(DVec3::new(x, y, z), yaw, pitch).is_ok())
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
        method("isPrimaryThread", "()Z", is_primary_thread as *mut c_void),
        method("shutdown", "()V", shutdown as *mut c_void),
        method("savePlayers", "()V", save_players as *mut c_void),
        method("experienceLevel", "(Ljava/lang/String;)I", experience_level as *mut c_void),
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
            "worldPlayerIds",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            world_player_ids as *mut c_void,
        ),
        method(
            "worldEntityIds",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            world_entity_ids as *mut c_void,
        ),
        method("requestChunk", "(Ljava/lang/String;II)Ljava/lang/String;", request_chunk as *mut c_void),
        method("chunkRequestReady", "(Ljava/lang/String;)Z", chunk_request_ready as *mut c_void),
        method("worldChunkLoaded", "(Ljava/lang/String;II)Z", world_chunk_loaded as *mut c_void),
        method("worldLoadedChunkCoords", "(Ljava/lang/String;)[Ljava/lang/String;", world_loaded_chunk_coords as *mut c_void),
        method("worldFolder", "(Ljava/lang/String;)Ljava/lang/String;", world_folder as *mut c_void),
        method("worldAutoSave", "(Ljava/lang/String;)Z", world_auto_save as *mut c_void),
        method("setWorldAutoSave", "(Ljava/lang/String;Z)V", set_world_auto_save as *mut c_void),
        method("saveWorld", "(Ljava/lang/String;)V", save_world as *mut c_void),
        method("worldDropItem", "(Ljava/lang/String;DDDLjava/lang/String;)Ljava/lang/String;", world_drop_item as *mut c_void),
        method("scoreboardTeamEntries", "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;", scoreboard_team_entries as *mut c_void),
        method("scoreboardEntryTeam", "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;", scoreboard_entry_team as *mut c_void),
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
            "isSneaking",
            "(Ljava/lang/String;)Z",
            is_sneaking as *mut c_void,
        ),
        method(
            "openBook",
            "(Ljava/lang/String;)V",
            open_book as *mut c_void,
        ),
        method(
            "teleport",
            "(Ljava/lang/String;Ljava/lang/String;DDDFF)Z",
            teleport as *mut c_void,
        ),
        method("teleportEntity", "(Ljava/lang/String;Ljava/lang/String;DDDFF)Z", teleport_entity as *mut c_void),
        method(
            "worldMinHeight",
            "(Ljava/lang/String;)I",
            world_min_height as *mut c_void,
        ),
        method(
            "worldMaxHeight",
            "(Ljava/lang/String;)I",
            world_max_height as *mut c_void,
        ),
        method("entityWorld", "(Ljava/lang/String;)Ljava/lang/String;", entity_world as *mut c_void),
        method("removeEntity", "(Ljava/lang/String;)V", remove_entity as *mut c_void),
        method("entityType", "(Ljava/lang/String;)Ljava/lang/String;", entity_type as *mut c_void),
        method("entitySpawnCategory", "(Ljava/lang/String;)Ljava/lang/String;", entity_spawn_category as *mut c_void),
        method("entityPosition", "(Ljava/lang/String;)[D", entity_position as *mut c_void),
        method("entityId", "(Ljava/lang/String;)I", entity_id as *mut c_void),
        method("entityCustomName", "(Ljava/lang/String;)Ljava/lang/String;", entity_custom_name as *mut c_void),
        method("setEntityCustomName", "(Ljava/lang/String;Ljava/lang/String;)V", set_entity_custom_name as *mut c_void),
        method("entitySendMessage", "(Ljava/lang/String;Ljava/lang/String;)V", entity_send_message as *mut c_void),
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
            "isPermissionSet",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            is_permission_set as *mut c_void,
        ),
        method(
            "createBossBar",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            create_boss_bar as *mut c_void,
        ),
        method(
            "releaseBossBar",
            "(Ljava/lang/String;)V",
            release_boss_bar as *mut c_void,
        ),
        method(
            "bossBarSetTitle",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            boss_bar_set_title as *mut c_void,
        ),
        method(
            "bossBarSetColor",
            "(Ljava/lang/String;I)V",
            boss_bar_set_color as *mut c_void,
        ),
        method(
            "bossBarSetStyle",
            "(Ljava/lang/String;I)V",
            boss_bar_set_style as *mut c_void,
        ),
        method(
            "bossBarSetFlags",
            "(Ljava/lang/String;I)V",
            boss_bar_set_flags as *mut c_void,
        ),
        method(
            "bossBarSetProgress",
            "(Ljava/lang/String;D)V",
            boss_bar_set_progress as *mut c_void,
        ),
        method(
            "bossBarAddPlayer",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            boss_bar_add_player as *mut c_void,
        ),
        method(
            "bossBarRemovePlayer",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            boss_bar_remove_player as *mut c_void,
        ),
        method(
            "bossBarRemoveAll",
            "(Ljava/lang/String;)V",
            boss_bar_remove_all as *mut c_void,
        ),
        method(
            "bossBarPlayerIds",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            boss_bar_player_ids as *mut c_void,
        ),
        method(
            "bossBarSetVisible",
            "(Ljava/lang/String;Z)V",
            boss_bar_set_visible as *mut c_void,
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
            "playSoundCategory",
            "(Ljava/lang/String;DDDLjava/lang/String;Ljava/lang/String;FF)V",
            play_sound_category as *mut c_void,
        ),
        method(
            "stopSound",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            stop_sound as *mut c_void,
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
        method("playerLocale", "(Ljava/lang/String;)Ljava/lang/String;", player_locale as *mut c_void),
        method(
            "hasPlayedBefore",
            "(Ljava/lang/String;)Z",
            has_played_before as *mut c_void,
        ),
        method(
            "customName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            custom_name as *mut c_void,
        ),
        method(
            "setCustomName",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_custom_name as *mut c_void,
        ),
        method("playerFoodLevel", "(Ljava/lang/String;)I", player_food_level as *mut c_void),
        method("health", "(Ljava/lang/String;)D", health as *mut c_void),
        method(
            "setHealth",
            "(Ljava/lang/String;D)V",
            set_health as *mut c_void,
        ),
        method(
            "maxHealth",
            "(Ljava/lang/String;)D",
            max_health as *mut c_void,
        ),
        method(
            "playerWorld",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_world as *mut c_void,
        ),
        method("playerRespawnWorld", "(Ljava/lang/String;)Ljava/lang/String;", player_respawn_world as *mut c_void),
        method("playerRespawnPosition", "(Ljava/lang/String;)[D", player_respawn_position as *mut c_void),
        method(
            "sendMessage",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            send_message as *mut c_void,
        ),
        method(
            "kickPlayer",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            kick_player as *mut c_void,
        ),
        method(
            "setPlayerListHeader",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_header as *mut c_void,
        ),
        method(
            "setPlayerListFooter",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_footer as *mut c_void,
        ),
        method(
            "setPlayerListHeaderFooter",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_header_footer as *mut c_void,
        ),
        method(
            "sendActionBar",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            send_action_bar as *mut c_void,
        ),
        method(
            "sendTitle",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;III)V",
            send_title as *mut c_void,
        ),
        method(
            "clearTitle",
            "(Ljava/lang/String;)V",
            clear_title as *mut c_void,
        ),
        method(
            "sendPluginMessage",
            "(Ljava/lang/String;Ljava/lang/String;[B)V",
            send_plugin_message as *mut c_void,
        ),
        method(
            "hasPermission",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            has_permission as *mut c_void,
        ),
    ]
}
