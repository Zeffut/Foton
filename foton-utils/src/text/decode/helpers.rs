//! Scalar validators retained from the pinned parser; see the parent module.
use super::{Color, ComponentDecodeError, NbtCompound, NbtList, NbtTag, Uuid, nbt_list_len};

pub(super) fn parse_uuid(tag: &NbtTag) -> Result<Uuid, ComponentDecodeError> {
    match tag {
        NbtTag::String(value) => {
            Uuid::parse_str(&value.to_string()).map_err(|_| invalid("uuid", "a UUID"))
        }
        NbtTag::IntArray(values) | NbtTag::List(NbtList::Int(values)) if values.len() == 4 => {
            Ok(Uuid::from_u64_pair(
                (u64::from(values[0] as u32) << 32) | u64::from(values[1] as u32),
                (u64::from(values[2] as u32) << 32) | u64::from(values[3] as u32),
            ))
        }
        _ => Err(invalid("uuid", "a UUID")),
    }
}

pub(super) fn parse_color(value: &str) -> Result<Color, ComponentDecodeError> {
    let color = match value {
        "aqua" => Color::Aqua,
        "black" => Color::Black,
        "blue" => Color::Blue,
        "dark_aqua" => Color::DarkAqua,
        "dark_blue" => Color::DarkBlue,
        "dark_gray" => Color::DarkGray,
        "dark_green" => Color::DarkGreen,
        "dark_purple" => Color::DarkPurple,
        "dark_red" => Color::DarkRed,
        "gold" => Color::Gold,
        "gray" => Color::Gray,
        "green" => Color::Green,
        "light_purple" => Color::LightPurple,
        "red" => Color::Red,
        "white" => Color::White,
        "yellow" => Color::Yellow,
        value => {
            let Some(hex) = value.strip_prefix('#') else {
                return Err(invalid("color", "a text color"));
            };
            let rgb = u32::from_str_radix(hex, 16)
                .ok()
                .filter(|_| !hex.is_empty())
                .filter(|value| *value <= 0x00ff_ffff)
                .ok_or_else(|| invalid("color", "a text color"))?;
            Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
        }
    };
    Ok(color)
}

pub(super) fn parse_shadow_color(tag: &NbtTag) -> Result<i32, ComponentDecodeError> {
    if let Some(value) = as_i32(tag) {
        return Ok(value);
    }
    let NbtTag::List(list) = tag else {
        return Err(invalid(
            "shadow_color",
            "an ARGB integer or four-number list",
        ));
    };
    if nbt_list_len(list) != 4
        || !matches!(
            list,
            NbtList::Byte(_)
                | NbtList::Short(_)
                | NbtList::Int(_)
                | NbtList::Long(_)
                | NbtList::Float(_)
                | NbtList::Double(_)
        )
    {
        return Err(invalid(
            "shadow_color",
            "an ARGB integer or four-number list",
        ));
    }
    let values = list.as_nbt_tags();
    let mut channels = [0_u32; 4];
    for (channel, value) in channels.iter_mut().zip(values.iter()) {
        let value = as_f32(value)
            .ok_or_else(|| invalid("shadow_color", "an ARGB integer or four-number list"))?;
        *channel = ((value * 255.0).floor() as i32 as u32) & 0xff;
    }
    Ok(((channels[3] << 24) | (channels[0] << 16) | (channels[1] << 8) | channels[2]) as i32)
}

pub(super) fn required_compound<'a>(
    compound: &'a NbtCompound,
    field: &'static str,
) -> Result<&'a NbtCompound, ComponentDecodeError> {
    match compound.get(field) {
        Some(NbtTag::Compound(value)) => Ok(value),
        Some(_) => Err(invalid(field, "a compound")),
        None => Err(ComponentDecodeError::MissingField(field)),
    }
}

pub(super) fn required_string(
    compound: &NbtCompound,
    field: &'static str,
) -> Result<String, ComponentDecodeError> {
    match compound.get(field) {
        Some(NbtTag::String(value)) => Ok(value.to_string()),
        Some(_) => Err(invalid(field, "a string")),
        None => Err(ComponentDecodeError::MissingField(field)),
    }
}

pub(super) fn optional_string(
    compound: &NbtCompound,
    field: &'static str,
) -> Result<Option<String>, ComponentDecodeError> {
    match compound.get(field) {
        Some(NbtTag::String(value)) => Ok(Some(value.to_string())),
        Some(_) => Err(invalid(field, "a string")),
        None => Ok(None),
    }
}

pub(super) fn required_identifier(
    compound: &NbtCompound,
    field: &'static str,
) -> Result<String, ComponentDecodeError> {
    let value = required_string(compound, field)?;
    if !is_identifier(&value) {
        return Err(invalid(field, "an identifier"));
    }
    Ok(value)
}

pub(super) fn optional_identifier(
    compound: &NbtCompound,
    field: &'static str,
) -> Result<Option<String>, ComponentDecodeError> {
    let Some(value) = optional_string(compound, field)? else {
        return Ok(None);
    };
    if !is_identifier(&value) {
        return Err(invalid(field, "an identifier"));
    }
    Ok(Some(value))
}

pub(super) fn required_chat_string(
    compound: &NbtCompound,
    field: &'static str,
) -> Result<String, ComponentDecodeError> {
    let value = required_string(compound, field)?;
    if value
        .chars()
        .any(|character| character == '\u{a7}' || character < ' ' || character == '\u{7f}')
    {
        return Err(invalid(field, "a chat string"));
    }
    Ok(value)
}

pub(super) fn optional_bool(
    compound: &NbtCompound,
    field: &'static str,
) -> Result<Option<bool>, ComponentDecodeError> {
    match compound.get(field) {
        Some(value) => as_bool(value)
            .map(Some)
            .ok_or_else(|| invalid(field, "a boolean")),
        None => Ok(None),
    }
}

pub(super) fn required_i32(
    compound: &NbtCompound,
    field: &'static str,
) -> Result<i32, ComponentDecodeError> {
    compound
        .get(field)
        .and_then(as_i32)
        .ok_or_else(|| match compound.get(field) {
            Some(_) => invalid(field, "an integer"),
            None => ComponentDecodeError::MissingField(field),
        })
}

pub(super) fn as_string(tag: &NbtTag) -> Option<String> {
    match tag {
        NbtTag::String(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn as_bool(tag: &NbtTag) -> Option<bool> {
    as_f64(tag).map(|value| value != 0.0)
}

pub(super) fn as_i32(tag: &NbtTag) -> Option<i32> {
    match tag {
        NbtTag::Byte(value) => Some(i32::from(*value)),
        NbtTag::Short(value) => Some(i32::from(*value)),
        NbtTag::Int(value) => Some(*value),
        NbtTag::Long(value) => Some(*value as i32),
        NbtTag::Float(value) => Some(*value as i32),
        NbtTag::Double(value) => Some(*value as i32),
        _ => None,
    }
}

pub(super) fn as_f32(tag: &NbtTag) -> Option<f32> {
    as_f64(tag).map(|value| value as f32)
}

pub(super) fn as_f64(tag: &NbtTag) -> Option<f64> {
    match tag {
        NbtTag::Byte(value) => Some(f64::from(*value)),
        NbtTag::Short(value) => Some(f64::from(*value)),
        NbtTag::Int(value) => Some(f64::from(*value)),
        NbtTag::Long(value) => Some(*value as f64),
        NbtTag::Float(value) => Some(f64::from(*value)),
        NbtTag::Double(value) => Some(*value),
        _ => None,
    }
}

pub(super) fn validate_player_name(value: &str) -> Result<(), ComponentDecodeError> {
    if value.encode_utf16().count() > 16
        || value
            .chars()
            .any(|character| character <= ' ' || character >= '\u{7f}')
    {
        return Err(invalid("name", "a valid player name"));
    }
    Ok(())
}

pub(super) fn is_identifier(value: &str) -> bool {
    let (namespace, path) = value
        .split_once(':')
        .map_or(("minecraft", value), |(namespace, path)| (namespace, path));
    namespace != ".."
        && namespace.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
                || character == '.'
        })
        && path.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-' | '.' | '/')
        })
}

pub(super) fn is_allowed_url(value: &str) -> bool {
    let Some((scheme, remainder)) = value.split_once(':') else {
        return false;
    };
    matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
        && !remainder.is_empty()
        && !value.chars().any(char::is_whitespace)
}

pub(super) const fn invalid(field: &'static str, expected: &'static str) -> ComponentDecodeError {
    ComponentDecodeError::InvalidField { field, expected }
}
