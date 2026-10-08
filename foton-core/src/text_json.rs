//! Reading a text component from vanilla's JSON form.

use serde_json::Value;
use text_components::TextComponent;

/// Parses vanilla component JSON, or `None` when it is not one.
///
/// `text_components` reads an RGB color as `{"rgb": [r, g, b]}` rather than
/// vanilla's `"#rrggbb"`, so hex colors are rewritten on the way in.
#[must_use]
pub fn parse_component(json: &str) -> Option<TextComponent> {
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
    fn vanilla_json_keeps_hex_colors_decorations_and_children() {
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
