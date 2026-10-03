//! Strict admission for the existing no-carrier slot format.
//! This format remains a limited projection, never a lossless snapshot codec.

use std::collections::HashSet;

use foton_registry::{REGISTRY, RegistryExt, data_components::components::ItemLore};
use foton_utils::Identifier;

fn utf8_hex(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(value.get(index..index + 2)?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

/// Reject everything the historical decoder would otherwise skip or truncate.
/// Typed book and PDC validation stays with their existing owning decoders.
pub(crate) fn validate(text: &str) -> Option<()> {
    if text.is_empty() {
        return Some(());
    }
    if text.trim() != text {
        return None;
    }
    let mut fields = text.split('\u{1d}');
    let header = fields.next()?;
    if header.is_empty() {
        return None;
    }
    let mut singles = HashSet::new();
    let mut enchantments = HashSet::new();
    let mut lore = 0;
    let mut saw_effects = false;
    for field in fields {
        let (key, value) = field.split_once('=').unwrap_or((field, ""));
        let repeat = matches!(
            key,
            "lorehex"
                | "modelfloat"
                | "modelflag"
                | "modelstrhex"
                | "modelcolor"
                | "enchhex"
                | "storedenchhex"
                | "bookpagehex"
                | "pdcstrhex"
                | "pdcbyte"
                | "pdcint"
                | "pdcremove"
        );
        if !repeat && !singles.insert(key) {
            return None;
        }
        match key {
            "unbreakable" | "hidetooltip" if field == key => {}
            "damage" => {
                if value.parse::<i32>().ok()? < 0 {
                    return None;
                }
            }
            "model" | "modelfloat" => {
                if !value.parse::<f32>().ok()?.is_finite() {
                    return None;
                }
            }
            "modelflag" | "bookresolved" => {
                value.parse::<bool>().ok()?;
            }
            "modelcolor" | "bookgen" => {
                value.parse::<i32>().ok()?;
            }
            "namehex" | "namejsonhex" | "modelstrhex" | "nbthex" | "booktitlehex"
            | "bookauthorhex" | "bookpagehex" => {
                utf8_hex(value)?;
            }
            "itemmodelhex" | "tooltipstylehex" => {
                utf8_hex(value)?.parse::<Identifier>().ok()?;
            }
            "lorehex" => {
                utf8_hex(value)?;
                lore += 1;
                if lore > ItemLore::MAX_LINES {
                    return None;
                }
            }
            "enchhex" | "storedenchhex" => {
                let (name, level) = value.rsplit_once(':')?;
                let name = utf8_hex(name)?.parse::<Identifier>().ok()?;
                level.parse::<u32>().ok()?;
                if !enchantments.insert((key, name)) {
                    return None;
                }
            }
            "bookrawhex" | "pdcrawhex" | "pdcstrhex" | "pdcbyte" | "pdcint" | "pdcremove" => {}
            "pdcidentity" => {
                if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
            }
            _ => {
                if saw_effects || field.contains('=') || field.is_empty() {
                    return None;
                }
                parse_effects(field)?;
                saw_effects = true;
            }
        }
    }
    Some(())
}

pub(crate) fn parse_effects(
    value: &str,
) -> Option<Vec<foton_registry::mob_effect::instance::MobEffectInstance>> {
    use foton_registry::mob_effect::instance::MobEffectInstance;
    value
        .strip_suffix(';')
        .unwrap_or(value)
        .split(';')
        .map(|effect| {
            let mut parts = effect.split(',');
            let name: Identifier = format!("minecraft:{}", parts.next()?).parse().ok()?;
            let effect = REGISTRY.mob_effects.by_key(&name)?;
            let duration = parts.next()?.parse::<i32>().ok()?;
            let amplifier = parts.next()?.parse::<i32>().ok()?;
            if parts.next().is_some() {
                return None;
            }
            Some(MobEffectInstance::simple(effect, duration, amplifier))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::natives::parse_slot;

    #[test]
    fn malformed_nonempty_metadata_is_not_partially_accepted() {
        foton_registry::init_vanilla_registry();
        for metadata in [
            "namehex=gg",
            "lorehex=f",
            "damage=abc",
            "model=NaN",
            "modelflag=1",
            "pdcrawhex=00",
            "pdcint=6b:bad",
            "unknown=123",
            "unbreakable=garbage",
            "enchhex=6d696e6563726166743a73686172706e657373:bad",
        ] {
            assert!(
                parse_slot(&format!("minecraft:stone 1\u{1d}{metadata}")).is_none(),
                "{metadata}"
            );
        }
        assert!(parse_slot("minecraft:stone 1\u{1d}damage=1\u{1d}damage=2").is_none());
        assert!(parse_slot(" ").is_none());
        assert!(parse_slot("").expect("legacy empty").is_empty());
        assert!(parse_slot("minecraft:stone 1\u{1d}namehex=6f6b").is_some());
    }
}
