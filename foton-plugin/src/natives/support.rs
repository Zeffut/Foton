//! Argument plumbing shared by the entity, player and world natives.
//!
//! Every native here receives Java strings and answers "nothing" when what
//! they name no longer exists: Bukkit's contract for a stale handle is an
//! object that quietly does nothing, and plugins rely on it.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::Arc;

use foton_core::entity::SharedEntity;
use foton_core::player::Player;
use foton_core::world::World;
use foton_utils::Identifier;
use jni::JNIEnv;
use jni::objects::{JDoubleArray, JString};
use jni::sys::jdoubleArray;
use serde_json::Value;
use text_components::TextComponent;
use uuid::Uuid;

use super::{entity_by_uuid, server};

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

/// The online player a Java handle names.
pub(super) fn player(env: &mut JNIEnv<'_>, uuid: &JString<'_>) -> Option<Arc<Player>> {
    let id = Uuid::parse_str(&text(env, uuid)?).ok()?;
    server()?.online_players().get_by_uuid(&id)
}

/// A world by the key a plugin holds it under.
pub(super) fn world(env: &mut JNIEnv<'_>, name: &JString<'_>) -> Option<Arc<World>> {
    let key = key(env, name)?;
    server()?.worlds.get_owned(&key)
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

/// A component sent as vanilla JSON by `FotonComponents.toJson`.
pub(super) fn component(env: &mut JNIEnv<'_>, json: &JString<'_>) -> Option<TextComponent> {
    parse_component(&text(env, json)?)
}

/// `text_components` reads an RGB colour as `{"rgb": [r, g, b]}` rather than
/// vanilla's `"#rrggbb"`, so hex colours are rewritten on the way in.
pub(super) fn parse_component(json: &str) -> Option<TextComponent> {
    let mut value: Value = serde_json::from_str(json).ok()?;
    rewrite_hex_colors(&mut value);
    serde_json::from_value(value).ok()
}

fn rewrite_hex_colors(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let rgb = object
                .get("color")
                .and_then(Value::as_str)
                .and_then(|color| color.strip_prefix('#'))
                .filter(|hex| hex.len() == 6)
                .and_then(|hex| u32::from_str_radix(hex, 16).ok());
            if let Some(rgb) = rgb {
                let [_, red, green, blue] = rgb.to_be_bytes();
                object.insert(
                    "color".to_owned(),
                    serde_json::json!({ "rgb": [red, green, blue] }),
                );
            }
            object.values_mut().for_each(rewrite_hex_colors);
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite_hex_colors),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use text_components::content::Content;
    use text_components::format::Color;

    use super::parse_component;

    #[test]
    fn vanilla_json_keeps_hex_colours_decorations_and_children() {
        let parsed = parse_component(
            r##"{"text":"Link","color":"#12ab34","bold":true,"extra":[{"translate":"a.b","with":[{"text":"x"}],"color":"gold"}]}"##,
        );
        let Some(parsed) = parsed else {
            panic!("the JSON FotonComponents writes did not decode");
        };
        assert_eq!(parsed.format.color, Some(Color::Rgb(0x12, 0xab, 0x34)));
        assert_eq!(parsed.format.bold, Some(true));
        assert!(matches!(parsed.children[0].content, Content::Translate(_)));
        assert_eq!(parsed.children[0].format.color, Some(Color::Gold));
    }
}
