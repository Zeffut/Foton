//! Paper's events that Foton learned to carry after the first set.
//!
//! Every crossing here has the same shape, so it is written once: the facts go
//! over as strings to a static method of `foton.EventRelay`, Java builds its
//! event, runs the handlers, and answers with one string that the subscription
//! below reads back. Fields in an answer are separated by [`FIELD`]; a boolean
//! is `1` or `0`. A crossing that fails -- no JVM thread, a throwing handler --
//! answers `None`, and every subscription treats that as "nobody objected",
//! because a broken plugin must not be able to freeze the game.

use std::sync::Arc;

use foton_core::event::{
    PlayerArmorChangeEvent, PlayerFailMoveEvent, PlayerItemConsumeEvent,
    PlayerToggleFlightEvent, PlayerVelocityEvent,
};
use foton_registry::equipment::EquipmentSlot;
use foton_utils::types::InteractionHand;
use foton_core::server::Server;
use glam::DVec3;
use jni::JavaVM;
use jni::objects::{JObject, JString, JValue};

use crate::forward::{BridgeEnv, owner};
use crate::natives::{describe_slot, parse_slot};

/// The Java class these events are built in.
const RELAY: &str = "foton/EventRelay";

/// Separates the fields of one answer.
const FIELD: char = '\u{1f}';

/// Calls `EventRelay.<method>(String...)` and returns its `String` answer.
///
/// The arguments are created inside a local frame, so a long-lived attached
/// thread -- the tick thread is one -- does not keep a reference per call.
fn text_call(vm: &JavaVM, method: &str, args: &[&str]) -> Option<String> {
    let mut env = BridgeEnv::attach(vm)?;
    let signature = format!("({})Ljava/lang/String;", "Ljava/lang/String;".repeat(args.len()));
    env.with_local_frame(i32::try_from(args.len()).unwrap_or(i32::MAX).saturating_add(4), |env| {
        let mut strings: Vec<JObject<'_>> = Vec::with_capacity(args.len());
        for arg in args {
            strings.push(env.new_string(arg)?.into());
        }
        let values: Vec<JValue<'_, '_>> = strings.iter().map(JValue::Object).collect();
        let answer = env.call_static_method(RELAY, method, &signature, &values)?.l()?;
        if answer.is_null() {
            return Ok(None);
        }
        let answer = JString::from(answer);
        let text: String = env.get_string(&answer)?.into();
        Ok::<_, jni::errors::Error>(Some(text))
    })
    .ok()
    .flatten()
}

/// Splits an answer into its fields.
fn fields(answer: &str) -> Vec<&str> {
    answer.split(FIELD).collect()
}

/// Reads a `1`/`0` field.
fn flag(field: Option<&&str>) -> Option<bool> {
    match field.copied() {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

/// Writes a boolean the way the Java side reads it.
const fn bit(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

/// Reads `x y z` back into a vector.
fn vector(field: Option<&&str>) -> Option<DVec3> {
    let mut parts = field?.split(' ').map(str::parse::<f64>);
    let vector = DVec3::new(
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    vector.is_finite().then_some(vector)
}

/// Writes a position and rotation as `x y z yaw pitch`.
fn location(position: DVec3, rotation: (f32, f32)) -> String {
    format!(
        "{} {} {} {} {}",
        position.x, position.y, position.z, rotation.0, rotation.1
    )
}

/// Subscribes the relay to the events it carries.
pub(crate) fn subscribe(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<PlayerFailMoveEvent, _>(owner(), move |event| {
        let (from, from_rotation) = event.from();
        let (to, to_rotation) = event.to();
        let Some(answer) = text_call(
            &jvm,
            "fireFailMove",
            &[
                &event.player().to_string(),
                event.world(),
                event.reason().name(),
                &location(from, from_rotation),
                &location(to, to_rotation),
                bit(event.log_warning()),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if let Some(allowed) = flag(answer.first()) {
            event.set_allowed(allowed);
        }
        if let Some(log_warning) = flag(answer.get(1)) {
            event.set_log_warning(log_warning);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerToggleFlightEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireToggleFlight",
            &[&event.player().to_string(), bit(event.flying())],
        ) else {
            return;
        };
        if flag(fields(&answer).first()) == Some(true) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerVelocityEvent, _>(owner(), move |event| {
        let velocity = event.velocity();
        let Some(answer) = text_call(
            &jvm,
            "fireVelocity",
            &[
                &event.player().to_string(),
                &format!("{} {} {}", velocity.x, velocity.y, velocity.z),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(velocity) = vector(answer.get(1)) {
            event.set_velocity(velocity);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerArmorChangeEvent, _>(owner(), move |event| {
        let Some(slot) = armor_slot_name(event.slot()) else {
            return;
        };
        let _ = text_call(
            &jvm,
            "fireArmorChange",
            &[
                &event.player().to_string(),
                slot,
                &describe_slot(event.old_item()),
                &describe_slot(event.new_item()),
            ],
        );
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerItemConsumeEvent, _>(owner(), move |event| {
        let hand = match event.hand() {
            InteractionHand::MainHand => "HAND",
            InteractionHand::OffHand => "OFF_HAND",
        };
        let Some(answer) = text_call(
            &jvm,
            "fireItemConsume",
            &[&event.player().to_string(), hand, &describe_slot(event.item())],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(item) = answer.get(1).and_then(|text| parse_slot(text)) {
            event.set_item(item);
        }
        if flag(answer.get(2)) == Some(true) {
            event.set_replacement(answer.get(3).and_then(|text| parse_slot(text)));
        }
    });
}

/// The Bukkit `SlotType` name of an armor slot.
const fn armor_slot_name(slot: EquipmentSlot) -> Option<&'static str> {
    match slot {
        EquipmentSlot::Head => Some("HEAD"),
        EquipmentSlot::Chest => Some("CHEST"),
        EquipmentSlot::Legs => Some("LEGS"),
        EquipmentSlot::Feet => Some("FEET"),
        _ => None,
    }
}
