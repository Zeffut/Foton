//! Decoding the item-modifier codec.
//!
//! Vanilla parity: `LootItemFunctions.ROOT_CODEC` over `NumberProviders.CODEC`.
//! Errors are plain strings: the caller wraps them in whichever error its
//! source needs (a datapack load message, or `argument.resource_or_id`).

use foton_registry::{
    REGISTRY, RegistryExt as _,
    loot_table::{EnchantmentOptions, LootFunction, NameTarget, NumberProvider},
};
use foton_utils::{Identifier, nbt::parse_snbt_compound};
use serde_json::{Map, Value};
use simdnbt::owned::NbtTag;

use super::{ItemModifier, ModifierFunction, nbt_to_json};
use crate::text_json::parse_component;

/// Decodes a datapack `item_modifier/*.json` file.
///
/// # Errors
/// The text is not JSON, or is not a modifier this server can run.
pub fn from_json(text: &str) -> Result<ItemModifier, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    decode(&value)
}

/// Decodes an inline modifier written as SNBT.
///
/// # Errors
/// The tag is not a modifier this server can run.
pub fn from_nbt(tag: &NbtTag) -> Result<ItemModifier, String> {
    decode(&nbt_to_json(tag))
}

fn decode(value: &Value) -> Result<ItemModifier, String> {
    let mut functions = Vec::new();
    decode_into(value, &mut functions)?;
    Ok(ItemModifier { functions })
}

/// A list is a sequence, and so is a `minecraft:sequence` function.
fn decode_into(value: &Value, out: &mut Vec<ModifierFunction>) -> Result<(), String> {
    match value {
        Value::Array(items) => items.iter().try_for_each(|item| decode_into(item, out)),
        Value::Object(object) => decode_function(object, out),
        _ => Err("an item modifier is a function or a list of functions".to_owned()),
    }
}

fn decode_function(
    object: &Map<String, Value>,
    out: &mut Vec<ModifierFunction>,
) -> Result<(), String> {
    let name = object
        .get("function")
        .and_then(Value::as_str)
        .ok_or("missing field 'function'")?;
    let id = name
        .parse::<Identifier>()
        .map_err(|error| format!("invalid function id '{name}': {error}"))?;
    if object
        .get("conditions")
        .is_some_and(|conditions| conditions.as_array().is_none_or(|list| !list.is_empty()))
    {
        return Err(format!(
            "function {id} has conditions, which item modifiers do not support yet"
        ));
    }
    if id.namespace != Identifier::VANILLA_NAMESPACE {
        return Err(format!("unknown function {id}"));
    }

    let function = match &*id.path {
        "sequence" => {
            let functions = object.get("functions").ok_or("missing field 'functions'")?;
            return decode_into(functions, out);
        }
        "set_count" => LootFunction::SetCount {
            count: provider(required(object, "count")?)?,
            add: boolean(object, "add", false)?,
        },
        "limit_count" => {
            let (min, max) = int_range(required(object, "limit")?)?;
            LootFunction::LimitCount { min, max }
        }
        "set_damage" => LootFunction::SetDamage {
            damage: provider(required(object, "damage")?)?,
            add: boolean(object, "add", false)?,
        },
        "set_item" => {
            let item = identifier(required(object, "item")?)?;
            if REGISTRY.items.by_key(&item).is_none() {
                return Err(format!("unknown item {item}"));
            }
            LootFunction::SetItem { item }
        }
        "set_potion" => LootFunction::SetPotion {
            id: identifier(required(object, "id")?)?,
        },
        "set_ominous_bottle_amplifier" => LootFunction::SetOminousBottleAmplifier {
            amplifier: provider(required(object, "amplifier")?)?,
        },
        "furnace_smelt" => LootFunction::FurnaceSmelt {
            use_input_count: false,
        },
        "enchant_randomly" => LootFunction::EnchantRandomly {
            options: options(object)?,
            only_compatible: boolean(object, "only_compatible", true)?,
            include_additional_cost_component: boolean(
                object,
                "include_additional_cost_component",
                false,
            )?,
        },
        "enchant_with_levels" => LootFunction::EnchantWithLevels {
            levels: provider(required(object, "levels")?)?,
            options: options(object)?,
            include_additional_cost_component: boolean(
                object,
                "include_additional_cost_component",
                false,
            )?,
        },
        "set_name" => return decode_set_name(object, out),
        "set_enchantments" => return decode_set_enchantments(object, out),
        "set_custom_data" => return decode_set_custom_data(object, out),
        _ => return Err(format!("item modifier function {id} is not supported")),
    };
    out.push(ModifierFunction::Loot(function));
    Ok(())
}

fn decode_set_name(
    object: &Map<String, Value>,
    out: &mut Vec<ModifierFunction>,
) -> Result<(), String> {
    if object.contains_key("entity") {
        return Err("set_name with an 'entity' selector is not supported".to_owned());
    }
    let name = required(object, "name")?;
    let name = parse_component(&name.to_string()).ok_or("'name' is not a text component")?;
    let target = match object.get("target").and_then(Value::as_str) {
        None | Some("custom_name") => NameTarget::CustomName,
        Some("item_name") => NameTarget::ItemName,
        Some(other) => return Err(format!("unknown name target '{other}'")),
    };
    out.push(ModifierFunction::SetName {
        name: Box::new(name),
        target,
    });
    Ok(())
}

fn decode_set_enchantments(
    object: &Map<String, Value>,
    out: &mut Vec<ModifierFunction>,
) -> Result<(), String> {
    let map = required(object, "enchantments")?
        .as_object()
        .ok_or("'enchantments' is a map of enchantment to level")?;
    let enchantments = map
        .iter()
        .map(|(key, level)| {
            let id = key
                .parse::<Identifier>()
                .map_err(|error| format!("invalid enchantment id '{key}': {error}"))?;
            Ok((id, provider(level)?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    out.push(ModifierFunction::SetEnchantments {
        enchantments,
        add: boolean(object, "add", false)?,
    });
    Ok(())
}

/// Vanilla parity: `SetCustomDataFunction`, whose `tag` is SNBT in a string.
fn decode_set_custom_data(
    object: &Map<String, Value>,
    out: &mut Vec<ModifierFunction>,
) -> Result<(), String> {
    let tag = required(object, "tag")?
        .as_str()
        .ok_or("'tag' is a string of SNBT")?;
    let compound = parse_snbt_compound(tag)
        .map_err(|error| format!("'tag' is not an SNBT compound: {}", error.component()))?;
    out.push(ModifierFunction::SetCustomData(compound));
    Ok(())
}

fn required<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a Value, String> {
    object
        .get(field)
        .ok_or_else(|| format!("missing field '{field}'"))
}

/// A boolean the codec accepts as `true`/`false`, or SNBT as `1b`/`0b`.
fn boolean(object: &Map<String, Value>, field: &str, default: bool) -> Result<bool, String> {
    match object.get(field) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::Number(number)) if number.as_i64().is_some_and(|n| n == 0 || n == 1) => {
            Ok(number.as_i64() == Some(1))
        }
        Some(_) => Err(format!("'{field}' is not a boolean")),
    }
}

fn identifier(value: &Value) -> Result<Identifier, String> {
    let text = value.as_str().ok_or("expected an identifier")?;
    text.parse::<Identifier>()
        .map_err(|error| format!("invalid identifier '{text}': {error}"))
}

/// `options` is optional; only a tag (`#minecraft:on_random_loot`) can be held
/// at run time, since the engine keeps explicit lists as `&'static`.
fn options(object: &Map<String, Value>) -> Result<Option<EnchantmentOptions>, String> {
    let Some(value) = object.get("options") else {
        return Ok(None);
    };
    let tag = value
        .as_str()
        .and_then(|text| text.strip_prefix('#'))
        .ok_or("only an enchantment tag is supported for 'options'")?;
    let id = tag
        .parse::<Identifier>()
        .map_err(|error| format!("invalid tag '{tag}': {error}"))?;
    Ok(Some(EnchantmentOptions::Tag(id)))
}

/// Vanilla parity: `IntRange`, a number being the exact value and an object
/// bounding either side.
fn int_range(value: &Value) -> Result<(Option<i32>, Option<i32>), String> {
    if let Some(exact) = value.as_i64() {
        let exact = i32::try_from(exact).map_err(|_| "range bound is out of range")?;
        return Ok((Some(exact), Some(exact)));
    }
    let object = value.as_object().ok_or("'limit' is a number or a range")?;
    let bound = |field: &str| -> Result<Option<i32>, String> {
        object
            .get(field)
            .map(|bound| {
                let number = constant(bound)?;
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "an IntRange bound is an integer the float read back exactly"
                )]
                Ok(number as i32)
            })
            .transpose()
    };
    Ok((bound("min")?, bound("max")?))
}

/// Vanilla parity: `NumberProviders.CODEC`, for the providers that do not read
/// the world. A bare number is a constant, and an object with no `type` is a
/// uniform one.
fn provider(value: &Value) -> Result<NumberProvider, String> {
    if let Some(number) = value.as_f64() {
        return Ok(NumberProvider::Constant(narrow(number)));
    }
    let object = value
        .as_object()
        .ok_or("a number provider is a number or an object")?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .map_or("uniform", |kind| {
            kind.strip_prefix("minecraft:").unwrap_or(kind)
        });
    match kind {
        "constant" => Ok(NumberProvider::Constant(constant(value)?)),
        "uniform" => Ok(NumberProvider::Uniform {
            min: constant(required(object, "min")?)?,
            max: constant(required(object, "max")?)?,
        }),
        "binomial" => {
            let n = constant(required(object, "n")?)?;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a trial count is an integer the float read back exactly"
            )]
            Ok(NumberProvider::Binomial {
                n: n as i32,
                p: constant(required(object, "p")?)?,
            })
        }
        other => Err(format!("number provider {other} is not supported")),
    }
}

/// A provider that is a fixed number, the only kind its own bounds may be.
fn constant(value: &Value) -> Result<f32, String> {
    if let Some(number) = value.as_f64() {
        return Ok(narrow(number));
    }
    let object = value.as_object();
    let is_constant = object
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.strip_prefix("minecraft:").unwrap_or(kind) == "constant");
    object
        .filter(|_| is_constant)
        .and_then(|object| object.get("value"))
        .and_then(Value::as_f64)
        .map(narrow)
        .ok_or_else(|| "expected a constant number".to_owned())
}

/// Vanilla reads providers as floats.
#[expect(
    clippy::cast_possible_truncation,
    reason = "vanilla's ConstantValue is a float"
)]
const fn narrow(number: f64) -> f32 {
    number as f32
}
