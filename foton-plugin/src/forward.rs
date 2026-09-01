//! Foton's events, carried to whatever plugins are listening.
//!
//! The direction that matters. Everything else in this crate answers a
//! question a plugin asked; this delivers something that happened whether a
//! plugin asked or not, and reads back what the plugins decided about it.
//!
//! Each crossing is one call deep: Foton hands over the facts, Java builds its
//! own event object around them, runs the handlers, and returns the outcome.
//! Nothing on either side holds a reference to the other's objects past the
//! call, which is the only reason the lifetimes work at all.

use std::sync::Arc;

use crate::natives;
use foton_core::event::{
    BlockBreakEvent, BlockFromToEvent, BlockPlaceEvent, CommandEvent, CreatureSpawnEvent,
    EntityDamageByEntityEvent, EntityPickupItemEvent, EntityRegainHealthEvent,
    EntityRemoveFromWorldEvent, InventoryClickEvent, PlayerChatEvent, PlayerCommandPreprocessEvent,
    PlayerCustomPayloadEvent, PlayerDeathEvent, PlayerInteractEvent, PlayerJoinEvent,
    PlayerLoginEvent, PlayerMoveEvent, PlayerQuitEvent, PlayerRespawnEvent, ServerTickEvent,
};
use foton_core::player::Player;
use foton_core::server::Server;
use foton_utils::text::DisplayResolutor;
use foton_utils::{BlockPos, Identifier};
use jni::JavaVM;
use jni::objects::{JString, JValue, JValueGen};
use text_components::TextComponent;

/// The Java class that owns the handler lists.
const BRIDGE: &str = "foton/EventBridge";

/// Where a plugin's scheduled tasks wait until a tick can run them.
const SCHEDULER: &str = "foton/FotonScheduler";

/// The Bukkit channel registry and its listeners.
const MESSENGER: &str = "foton/FotonMessenger";

/// Who these subscriptions belong to, so unloading takes them with it.
fn owner() -> Identifier {
    Identifier::from_foton("plugins")
}

/// Subscribes the plugin host to the events plugins can see.
///
/// Every subscription is a copy of the same shape: gather the facts, cross
/// once, apply what came back. The repetition is on purpose -- a generic
/// version would need each event to describe itself to the bridge, which is
/// more machinery than five events justify.
pub(crate) fn subscribe(server: &Arc<Server>, vm: Arc<JavaVM>) {
    let events = server.events();
    for world in server.worlds.values() {
        world_call(&vm, "fireWorldLoad", &world.key.to_string());
    }

    let jvm = Arc::clone(&vm);
    events.on::<PlayerInteractEvent, _>(owner(), move |event| {
        if !interact_call(&jvm, &event.player_id().to_string()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<InventoryClickEvent, _>(owner(), move |event| {
        let item = event.current_item().map_or(String::new(), |stack| {
            format!("{} {}", stack.item().key, stack.count())
        });
        if !inventory_click_call(&jvm, &event.player_id().to_string(), &item, event.click()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityRemoveFromWorldEvent, _>(owner(), move |event| {
        remove_call(&jvm, &event.entity().to_string());
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityPickupItemEvent, _>(owner(), move |event| {
        if !pickup_call(&jvm, &event.entity().to_string(), &event.item().to_string()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<CreatureSpawnEvent, _>(owner(), move |event| {
        let (x, y, z) = event.position();
        if !creature_spawn_call(
            &jvm,
            &event.entity().to_string(),
            event.world(),
            x,
            y,
            z,
            event.reason(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityRegainHealthEvent, _>(owner(), move |event| {
        if !regain_health_call(&jvm, &event.entity().to_string(), event.amount()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityDamageByEntityEvent, _>(owner(), move |event| {
        if !damage_call(
            &jvm,
            &event.damager().to_string(),
            &event.entity().to_string(),
            event.cause(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerCommandPreprocessEvent, _>(owner(), move |event| {
        let message = format!("/{}", event.message());
        match string_call(
            &jvm,
            "fireCommandPreprocess",
            &event.player_id().to_string(),
            Some(&message),
        ) {
            Answer::Nothing => event.set_cancelled(true),
            Answer::Message(rewritten) => {
                event.set_message(rewritten.strip_prefix('/').unwrap_or(&rewritten).to_owned())
            }
            Answer::Unreachable => {}
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerLoginEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        if let Some(message) = login_call(&jvm, &uuid) {
            event.deny(message);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerMoveEvent, _>(owner(), move |event| {
        let player = event.player();
        let from = event.from();
        let to = event.to();
        match move_call(
            &jvm,
            &player.gameprofile.id.to_string(),
            &player.get_world().key.to_string(),
            from,
            to,
        ) {
            MoveAnswer::Cancelled => event.set_cancelled(true),
            MoveAnswer::Redirect(destination) => event.set_to(destination),
            MoveAnswer::Accepted | MoveAnswer::Unreachable => {}
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerRespawnEvent, _>(owner(), move |event| {
        respawn_call(&jvm, &event.player_id().to_string());
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerDeathEvent, _>(owner(), move |event| {
        death_call(&jvm, &event.player_id().to_string());
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerJoinEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        let message = event.message().map(plain);
        match string_call(&jvm, "fireJoin", &uuid, message.as_deref()) {
            Answer::Unreachable => {}
            Answer::Nothing => event.set_message(None),
            Answer::Message(text) => event.set_message(Some(TextComponent::from(text))),
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerQuitEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        let message = event.message().map(plain);
        match string_call(&jvm, "fireQuit", &uuid, message.as_deref()) {
            Answer::Unreachable => {}
            Answer::Nothing => event.set_message(None),
            Answer::Message(text) => event.set_message(Some(TextComponent::from(text))),
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerChatEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        let said = event.message().to_owned();
        match string_call(&jvm, "fireChat", &uuid, Some(&said)) {
            // Nothing came back: a plugin stopped it.
            Answer::Nothing => event.set_cancelled(true),
            Answer::Message(rewritten) if rewritten != said => event.set_message(rewritten),
            Answer::Unreachable | Answer::Message(_) => {}
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<BlockBreakEvent, _>(owner(), move |event| {
        if !block_call(&jvm, "fireBlockBreak", event.player(), event.position()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<BlockPlaceEvent, _>(owner(), move |event| {
        if !block_call(&jvm, "fireBlockPlace", event.player(), event.position()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<BlockFromToEvent, _>(owner(), move |event| {
        if !from_to_call(&jvm, event.world(), event.block(), event.to_block()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<CommandEvent, _>(owner(), move |event| {
        let uuid = event
            .player()
            .map(|player| player.gameprofile.id.to_string())
            .unwrap_or_default();
        if command_call(&jvm, &uuid, event.command()) {
            event.set_handled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerCustomPayloadEvent, _>(owner(), move |event| {
        plugin_message_call(
            &jvm,
            &event.player().gameprofile.id.to_string(),
            &event.channel().to_string(),
            event.payload(),
        );
    });

    // The tick. Not a gameplay event: it is what makes `runTask` mean what
    // Bukkit says it means. A plugin hands over a Runnable from whatever
    // thread it likes, and the body runs here -- inside the tick, on the tick
    // thread, where touching the world is safe.
    let jvm = vm;
    let ticking = Arc::clone(server);
    events.on::<ServerTickEvent, _>(owner(), move |_| {
        // Before the plugins run: this is what tells the natives which thread
        // may write to the world, and it runs the writes that could not.
        natives::begin_tick(&ticking);
        drain_scheduler(&jvm);
    });
}

/// Drops every subscription the plugin host made.
pub(crate) fn unsubscribe(server: &Arc<Server>) {
    // Plugins receive unload notifications while the JVM is still alive.
    // World instances are not removed from Steel here, so this is the lifecycle boundary.
    // The host calls this before tearing down the JVM.
    server.events().forget(&owner());
}

/// A component as the plain text a Bukkit plugin expects a message to be.
fn world_call(vm: &JavaVM, method: &str, world: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(world) = env.new_string(world) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        method,
        "(Ljava/lang/String;)V",
        &[JValue::Object(&world)],
    );
}

fn plain(message: &TextComponent) -> String {
    message.to_plain(&DisplayResolutor)
}

/// What the plugins decided about a message.
///
/// Three outcomes that must not be confused. Reaching nobody is not the same
/// as everybody agreeing there should be no message, and a server that
/// silenced its own chat because a JVM thread failed to attach would be a
/// miserable thing to debug.
enum Answer {
    /// The crossing failed. Change nothing.
    Unreachable,
    /// The plugins settled on no message at all.
    Nothing,
    /// The message the plugins settled on.
    Message(String),
}

/// Calls a bridge method that takes a player and a message and returns one.
fn string_call(vm: &JavaVM, method: &str, uuid: &str, message: Option<&str>) -> Answer {
    let reach = || -> Option<Option<String>> {
        let mut env = vm.attach_current_thread().ok()?;
        let uuid = env.new_string(uuid).ok()?;
        let message: JString<'_> = match message {
            Some(text) => env.new_string(text).ok()?,
            None => JString::default(),
        };
        let answer = env
            .call_static_method(
                BRIDGE,
                method,
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&uuid), JValue::Object(&message)],
            )
            .ok()?
            .l()
            .ok()?;
        if answer.is_null() {
            return Some(None);
        }
        let answer: JString<'_> = answer.into();
        Some(Some(env.get_string(&answer).ok()?.into()))
    };

    match reach() {
        None => Answer::Unreachable,
        Some(None) => Answer::Nothing,
        Some(Some(message)) => Answer::Message(message),
    }
}

fn login_call(vm: &JavaVM, uuid: &str) -> Option<String> {
    let mut env = vm.attach_current_thread().ok()?;
    let uuid = env.new_string(uuid).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireLogin",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&uuid)],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let answer: JString<'_> = answer.into();
    let message: String = env.get_string(&answer).ok()?.into();
    (!message.is_empty()).then_some(message)
}

fn interact_call(vm: &JavaVM, player_uuid: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player_uuid) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireInteract",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&uuid)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn inventory_click_call(vm: &JavaVM, player_uuid: &str, item: &str, click: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player_uuid) else {
        return true;
    };
    let Ok(item) = env.new_string(item) else {
        return true;
    };
    let Ok(click) = env.new_string(click) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireInventoryClick",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&uuid),
            JValue::Object(&item),
            JValue::Object(&click),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn remove_call(vm: &JavaVM, entity: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(entity) = env.new_string(entity) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "fireEntityRemove",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&entity)],
    );
}

fn pickup_call(vm: &JavaVM, entity: &str, item: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(item) = env.new_string(item) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityPickup",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&entity), JValue::Object(&item)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn creature_spawn_call(
    vm: &JavaVM,
    entity: &str,
    world: &str,
    x: f64,
    y: f64,
    z: f64,
    reason: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(reason) = env.new_string(reason) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireCreatureSpawn",
        "(Ljava/lang/String;Ljava/lang/String;DDDDLjava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&world),
            JValue::Double(x),
            JValue::Double(y),
            JValue::Double(z),
            JValue::Object(&reason),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn regain_health_call(vm: &JavaVM, entity: &str, amount: f32) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityRegainHealth",
        "(Ljava/lang/String;F)Z",
        &[JValue::Object(&entity), JValue::Float(amount)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn damage_call(vm: &JavaVM, damager: &str, entity: &str, cause: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(damager) = env.new_string(damager) else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(cause) = env.new_string(cause) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityDamage",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&damager),
            JValue::Object(&entity),
            JValue::Object(&cause),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

/// Calls a bridge method about a block. `true` means nothing objected.
///
/// A failed crossing answers `true`: a plugin host that cannot be reached must
/// not silently start cancelling the world's block changes.
fn block_call(vm: &JavaVM, method: &str, player: &Arc<Player>, position: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player.gameprofile.id.to_string()) else {
        return true;
    };
    let Ok(world) = env.new_string(player.get_world().key.to_string()) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        method,
        "(Ljava/lang/String;IIILjava/lang/String;)Z",
        &[
            JValue::Object(&uuid),
            JValue::Int(position.x()),
            JValue::Int(position.y()),
            JValue::Int(position.z()),
            JValue::Object(&world),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn from_to_call(vm: &JavaVM, world: &str, block: BlockPos, to_block: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireBlockFromTo",
        "(Ljava/lang/String;IIIIII)Z",
        &[
            JValue::Object(&world),
            JValue::Int(block.x()),
            JValue::Int(block.y()),
            JValue::Int(block.z()),
            JValue::Int(to_block.x()),
            JValue::Int(to_block.y()),
            JValue::Int(to_block.z()),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn death_call(vm: &JavaVM, uuid: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(uuid) = env.new_string(uuid) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "firePlayerDeath",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&uuid)],
    );
}

fn respawn_call(vm: &JavaVM, uuid: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(uuid) = env.new_string(uuid) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "firePlayerRespawn",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&uuid)],
    );
}

enum MoveAnswer {
    Cancelled,
    Accepted,
    Redirect(glam::DVec3),
    Unreachable,
}

fn move_call(
    vm: &JavaVM,
    uuid: &str,
    world: &str,
    from: glam::DVec3,
    to: glam::DVec3,
) -> MoveAnswer {
    let Ok(mut env) = vm.attach_current_thread() else {
        return MoveAnswer::Unreachable;
    };
    let Ok(uuid) = env.new_string(uuid) else {
        return MoveAnswer::Unreachable;
    };
    let Ok(world) = env.new_string(world) else {
        return MoveAnswer::Unreachable;
    };
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireMove",
            "(Ljava/lang/String;Ljava/lang/String;DDDDDD)Ljava/lang/String;",
            &[
                JValue::Object(&uuid),
                JValue::Object(&world),
                JValue::Double(from.x),
                JValue::Double(from.y),
                JValue::Double(from.z),
                JValue::Double(to.x),
                JValue::Double(to.y),
                JValue::Double(to.z),
            ],
        )
        .ok()
        .and_then(|value| value.l().ok());
    let Some(answer) = answer else {
        return MoveAnswer::Unreachable;
    };
    if answer.is_null() {
        return MoveAnswer::Cancelled;
    }
    let answer: JString<'_> = answer.into();
    let Ok(value) = env.get_string(&answer) else {
        return MoveAnswer::Unreachable;
    };
    let value: String = value.into();
    if value.is_empty() {
        return MoveAnswer::Accepted;
    }
    let mut parts = value.split(',');
    let (Some(x), Some(y), Some(z)) = (parts.next(), parts.next(), parts.next()) else {
        return MoveAnswer::Unreachable;
    };
    let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) else {
        return MoveAnswer::Unreachable;
    };
    MoveAnswer::Redirect(glam::DVec3::new(x, y, z))
}

/// Runs whatever the plugins queued for this tick.
///
/// A failed crossing is not worth a log line here -- the same message twenty
/// times a second helps nobody -- so it runs nothing this tick and tries again
/// on the next.
fn drain_scheduler(vm: &JavaVM) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    // The answer is how many task bodies ran. Nothing here needs it; the
    // fixture asserts on it through the same call.
    let _ = env.call_static_method(SCHEDULER, "tick", "()I", &[]);
}

/// Offers one typed command to the plugins, and reads back who owned it.
///
/// A failed crossing answers "nobody owned it", so the server carries on to
/// its own dispatcher. Answering the other way would swallow every command
/// typed on a server whose plugin host had stopped responding.
fn command_call(vm: &JavaVM, uuid: &str, line: &str) -> bool {
    let reach = || -> Option<bool> {
        let mut env = vm.attach_current_thread().ok()?;
        let uuid = env.new_string(uuid).ok()?;
        let line = env.new_string(line).ok()?;
        env.call_static_method(
            BRIDGE,
            "fireCommand",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            &[JValue::Object(&uuid), JValue::Object(&line)],
        )
        .ok()?
        .z()
        .ok()
    };
    reach().unwrap_or(false)
}

/// Delivers one opaque client payload to the Bukkit channel registry.
fn plugin_message_call(vm: &JavaVM, uuid: &str, channel: &str, payload: &[u8]) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(uuid) = env.new_string(uuid) else {
        return;
    };
    let Ok(channel) = env.new_string(channel) else {
        return;
    };
    let Ok(payload) = env.byte_array_from_slice(payload) else {
        return;
    };
    let _ = env.call_static_method(
        MESSENGER,
        "dispatchFromNetwork",
        "(Ljava/lang/String;Ljava/lang/String;[B)V",
        &[
            JValue::Object(&uuid),
            JValue::Object(&channel),
            JValue::Object(&payload),
        ],
    );
}
