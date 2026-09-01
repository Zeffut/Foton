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

use foton_core::event::{
    BlockBreakEvent, BlockPlaceEvent, PlayerChatEvent, PlayerJoinEvent, PlayerQuitEvent,
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

    let jvm = vm;
    events.on::<BlockPlaceEvent, _>(owner(), move |event| {
        if !block_call(&jvm, "fireBlockPlace", event.player(), event.position()) {
            event.set_cancelled(true);
        }
    });
}

/// Drops every subscription the plugin host made.
pub(crate) fn unsubscribe(server: &Arc<Server>) {
    server.events().forget(&owner());
}

/// A component as the plain text a Bukkit plugin expects a message to be.
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
