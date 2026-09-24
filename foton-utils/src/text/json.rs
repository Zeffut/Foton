//! Minecraft's JSON text form of a component.
//!
//! The component model derives its serde form field by field, and that form
//! is Minecraft's but for two details: an RGB colour is `{"rgb": [r, g, b]}`
//! where Minecraft writes `"#rrggbb"`, and a child or argument must be an
//! object where Minecraft also accepts a bare string. These two functions
//! translate at the border, so the server list and Adventure's Gson
//! serializer on the plugin side each read what they expect.

use serde_json::{Value, json};
use text_components::TextComponent;
use text_components::format::Color;

use super::DisplayResolutor;

/// The JSON text of a component.
#[must_use]
pub fn to_json(component: &TextComponent) -> Value {
    let Ok(mut value) = serde_json::to_value(component) else {
        return json!({ "text": component.to_plain(&DisplayResolutor) });
    };
    rgb_to_hex(&mut value);
    value
}

/// The component a JSON text object describes, or `None` when `text` is not
/// one.
#[must_use]
pub fn from_json(text: &str) -> Option<TextComponent> {
    let mut value = serde_json::from_str::<Value>(text).ok().filter(Value::is_object)?;
    to_model(&mut value);
    serde_json::from_value(value).ok()
}

/// Rewrites every `{"rgb": [r, g, b]}` colour as `"#rrggbb"`.
fn rgb_to_hex(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(color) = map.get_mut("color")
                && let Some(rgb) = color.get("rgb").and_then(Value::as_array)
                && let [r, g, b] = rgb.as_slice()
                && let (Some(r), Some(g), Some(b)) = (r.as_u64(), g.as_u64(), b.as_u64())
            {
                *color = Value::String(format!("#{r:02x}{g:02x}{b:02x}"));
            }
            map.values_mut().for_each(rgb_to_hex);
        }
        Value::Array(items) => items.iter_mut().for_each(rgb_to_hex),
        _ => {}
    }
}

/// Rewrites `"#rrggbb"` colours into the model's form and expands bare-string
/// children and arguments into text components.
fn to_model(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(color) = map.get_mut("color")
                && let Some(Color::Rgb(r, g, b)) = color.as_str().and_then(Color::from_hex)
            {
                *color = json!({ "rgb": [r, g, b] });
            }
            for key in ["extra", "with"] {
                if let Some(Value::Array(items)) = map.get_mut(key) {
                    for item in items.iter_mut() {
                        if let Value::String(text) = item {
                            *item = json!({ "text": text });
                        }
                    }
                }
            }
            map.values_mut().for_each(to_model);
        }
        Value::Array(items) => items.iter_mut().for_each(to_model),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use text_components::TextComponent;
    use text_components::format::Color;

    use super::{from_json, to_json};

    /// What Adventure's Gson serializer writes, as the plugin API configures
    /// it, for a MOTD like Zelda Civ's: a hex colour, a named one, bold, a
    /// newline child and click and hover events. Taken from its own output.
    const ADVENTURE_MOTD: &str = r##"{"extra":[{"bold":true,"color":"#FDC14B","text":"Zelda "},{"color":"dark_gray","text":"| "},{"text":"\n"},{"click_event":{"action":"run_command","command":"/help"},"hover_event":{"action":"show_text","value":{"text":"hi"}},"text":"x"}],"text":""}"##;

    /// A component a plugin built arrives with its structure. Reading it as
    /// plain text instead would put the raw JSON in the server list.
    #[test]
    fn adventure_json_reads_as_a_component() {
        let parsed = from_json(ADVENTURE_MOTD).expect("Adventure's JSON is a component");
        assert_eq!(parsed.children.len(), 4, "every child survives");
        assert_eq!(
            parsed.children[0].format.color,
            Some(Color::Rgb(0xfd, 0xc1, 0x4b)),
            "a hex colour is read as the colour it names"
        );
    }

    /// Minecraft's shorthand for a text child is a bare string.
    #[test]
    fn a_bare_string_child_is_a_text_component() {
        let parsed = from_json(r#"{"text":"","extra":["a",{"text":"b"}]}"#).expect("a component");
        assert_eq!(parsed.children[0], TextComponent::plain("a"));
    }

    /// Adventure and the client read a hex colour, and nothing else, for an
    /// RGB one.
    #[test]
    fn an_rgb_colour_is_written_as_hex() {
        let parsed = from_json(r##"{"text":"a","color":"#FDC14B"}"##).expect("a component");
        assert_eq!(to_json(&parsed)["color"], "#fdc14b");
    }

    /// The vanilla join message goes to the plugins and comes back unchanged
    /// when none touched it.
    #[test]
    fn a_translatable_message_survives_the_round_trip() {
        let json = r#"{"translate":"multiplayer.player.joined","color":"yellow","with":[{"text":"Steve"}]}"#;
        let parsed = from_json(json).expect("a component");
        assert_eq!(from_json(&to_json(&parsed).to_string()), Some(parsed));
    }
}
