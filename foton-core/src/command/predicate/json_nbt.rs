//! JSON to NBT, the way vanilla's `Dynamic.convert(NbtOps.INSTANCE)` does it.

use serde_json::Value;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

/// Converts datapack JSON to NBT.
///
/// Vanilla parity: `JsonOps.convertTo`. A JSON integer becomes the *smallest*
/// of byte, short, int and long that holds it, and a number with a fractional
/// part becomes a float if that is exact and a double otherwise. Component
/// values that are compared whole -- `custom_data` above all -- are compared
/// with those types, so `{"nuke": 1}` in a predicate means `nuke: 1b`, and only
/// matches an item that stores a byte.
pub(super) fn json_to_nbt(path: &str, value: &Value) -> Result<NbtTag, String> {
    match value {
        Value::Null => Err(format!("{path}: null has no NBT form")),
        Value::Bool(flag) => Ok(NbtTag::Byte(i8::from(*flag))),
        Value::Number(number) => Ok(number_tag(number)),
        Value::String(text) => Ok(NbtTag::String(text.as_str().into())),
        Value::Array(items) => list_tag(path, items).map(NbtTag::List),
        Value::Object(entries) => {
            let mut compound = NbtCompound::new();
            for (key, entry) in entries {
                compound.insert(key.as_str(), json_to_nbt(&format!("{path}.{key}"), entry)?);
            }
            Ok(NbtTag::Compound(compound))
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    reason = "each narrowing is guarded by an exact round-trip, as in JsonOps.convertTo"
)]
fn number_tag(number: &serde_json::Number) -> NbtTag {
    if let Some(integer) = number
        .as_i64()
        .or_else(|| number.as_u64().and_then(|big| i64::try_from(big).ok()))
    {
        return if i64::from(integer as i8) == integer {
            NbtTag::Byte(integer as i8)
        } else if i64::from(integer as i16) == integer {
            NbtTag::Short(integer as i16)
        } else if i64::from(integer as i32) == integer {
            NbtTag::Int(integer as i32)
        } else {
            NbtTag::Long(integer)
        };
    }
    let double = number.as_f64().unwrap_or(f64::NAN);
    if f64::from(double as f32) == double {
        NbtTag::Float(double as f32)
    } else {
        NbtTag::Double(double)
    }
}

fn list_tag(path: &str, items: &[Value]) -> Result<NbtList, String> {
    let tags = items
        .iter()
        .enumerate()
        .map(|(index, item)| json_to_nbt(&format!("{path}[{index}]"), item))
        .collect::<Result<Vec<_>, _>>()?;
    let mixed = || format!("{path}: a list mixing different value types is not supported");
    macro_rules! homogeneous {
        ($variant:ident, $list:ident) => {
            tags.iter()
                .map(|tag| match tag {
                    NbtTag::$variant(value) => Ok(value.clone()),
                    _ => Err(mixed()),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(NbtList::$list)
        };
    }
    match tags.first() {
        None => Ok(NbtList::Empty),
        Some(NbtTag::Byte(_)) => homogeneous!(Byte, Byte),
        Some(NbtTag::Short(_)) => homogeneous!(Short, Short),
        Some(NbtTag::Int(_)) => homogeneous!(Int, Int),
        Some(NbtTag::Long(_)) => homogeneous!(Long, Long),
        Some(NbtTag::Float(_)) => homogeneous!(Float, Float),
        Some(NbtTag::Double(_)) => homogeneous!(Double, Double),
        Some(NbtTag::String(_)) => homogeneous!(String, String),
        Some(NbtTag::List(_)) => homogeneous!(List, List),
        Some(NbtTag::Compound(_)) => homogeneous!(Compound, Compound),
        Some(_) => Err(mixed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_take_the_smallest_type_that_holds_them() {
        let tag = json_to_nbt(
            "t",
            &serde_json::json!({"a": 1, "b": 300, "c": 70000, "d": 5_000_000_000_i64, "e": 0.5}),
        )
        .expect("converts");
        let NbtTag::Compound(compound) = tag else {
            panic!("an object should become a compound");
        };
        assert_eq!(compound.get("a"), Some(&NbtTag::Byte(1)));
        assert_eq!(compound.get("b"), Some(&NbtTag::Short(300)));
        assert_eq!(compound.get("c"), Some(&NbtTag::Int(70_000)));
        assert_eq!(compound.get("d"), Some(&NbtTag::Long(5_000_000_000)));
        assert_eq!(compound.get("e"), Some(&NbtTag::Float(0.5)));
    }
}
