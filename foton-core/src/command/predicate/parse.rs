//! Lowers a datapack `predicate/*.json` file into the typed predicates of
//! [`foton_registry::advancement::predicate`].
//!
//! Vanilla parity: `LootItemCondition.DIRECT_CODEC`, which reads either one
//! typed condition or a list of them (an implicit `all_of`), with the entity
//! sub-predicate map of `EntityPredicate.CODEC` inside.
//!
//! Every object is read key by key through [`Object`], and anything left over
//! is an error. A predicate Foton cannot model is therefore refused at load,
//! with the path of the key, instead of loading as one that asks less than the
//! datapack wrote and answering `true` for the wrong things.

use foton_registry::advancement::predicate::{
    BlockPredicate, ConditionTerm, ContextAwarePredicate, DistancePredicate, DoubleBounds,
    EntityComponentMatch, EntityEquipmentPredicate, EntityFlagsPredicate, EntityPredicate,
    IntBounds, ItemPredicate, LocationPredicate, RegistrySet, StatePropertyMatch,
};
use foton_registry::item_predicate::ItemPredicate as FullItemPredicate;
use foton_utils::Identifier;
use serde_json::{Map, Value};

use super::json_nbt::json_to_nbt;

type Parsed<T> = Result<T, String>;

/// Hands a parsed value to the lifetime the predicate types ask for.
///
/// The advancement predicate model is built from `&'static` data because its
/// vanilla instances are generated at build time. [`super::PredicateCache`]
/// interns by source text, so a `/reload` of unchanged files leaks nothing new.
fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}

fn leak_slice<T>(values: Vec<T>) -> &'static [T] {
    Vec::leak(values)
}

fn leak_str(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

/// Parses a whole predicate file.
pub(super) fn parse_predicate(value: &Value) -> Parsed<ContextAwarePredicate> {
    match value {
        Value::Array(list) => condition_list("predicate", list),
        other => Ok(leak_slice(vec![condition("predicate", other)?])),
    }
}

fn condition_list(path: &str, list: &[Value]) -> Parsed<&'static [ConditionTerm]> {
    let terms = list
        .iter()
        .enumerate()
        .map(|(index, term)| condition(&format!("{path}[{index}]"), term))
        .collect::<Parsed<Vec<_>>>()?;
    Ok(leak_slice(terms))
}

/// A condition that never passes: `AnyOf` with no alternatives.
const NEVER: ConditionTerm = ConditionTerm::AnyOf(&[]);
/// A condition that always passes: `AllOf` with no requirements.
const ALWAYS: ConditionTerm = ConditionTerm::AllOf(&[]);

fn condition(path: &str, value: &Value) -> Parsed<ConditionTerm> {
    let mut object = Object::new(path, value)?;
    let kind = object.string("condition")?;
    let term = match strip_minecraft(&kind) {
        "entity_properties" => {
            let target = object.optional_string("entity")?;
            let predicate = object.take("predicate");
            // Vanilla `LootItemEntityPropertyCondition.test` passes outright
            // without a predicate; `/execute if predicate` only supplies
            // `this`, so any other target is a null entity that fails.
            match (predicate, target.as_deref().map(strip_minecraft)) {
                (None, _) => ALWAYS,
                (Some(_), Some(other)) if other != "this" => NEVER,
                (Some(predicate), _) => ConditionTerm::EntityProperties(leak(entity_predicate(
                    &object.child("predicate"),
                    &predicate,
                )?)),
            }
        }
        "location_check" => {
            let offset_x = object.optional_i32("offsetX")?.unwrap_or(0);
            let offset_y = object.optional_i32("offsetY")?.unwrap_or(0);
            let offset_z = object.optional_i32("offsetZ")?.unwrap_or(0);
            let predicate = object.take("predicate");
            ConditionTerm::LocationCheck {
                offset_x,
                offset_y,
                offset_z,
                predicate: leak(match predicate {
                    Some(predicate) => location_predicate(&object.child("predicate"), &predicate)?,
                    None => LocationPredicate::ANY,
                }),
            }
        }
        // `/execute if predicate` has no TOOL and no BLOCK_STATE, and vanilla's
        // `MatchTool` and `BlockStateProperty` both fail without theirs. They
        // are still parsed so a malformed file is refused like any other.
        "match_tool" => {
            let predicate = object.take("predicate");
            if let Some(predicate) = predicate {
                item_predicate(&object.child("predicate"), &predicate)?;
            }
            NEVER
        }
        "block_state_property" => {
            object.string("block")?;
            if let Some(properties) = object.take("properties") {
                state_properties(&object.child("properties"), &properties)?;
            }
            NEVER
        }
        "any_of" | "all_of" => {
            let terms = object.take_required("terms")?;
            let Value::Array(list) = &terms else {
                return Err(format!(
                    "{}: expected a list of conditions",
                    object.child("terms")
                ));
            };
            let terms = condition_list(&object.child("terms"), list)?;
            if strip_minecraft(&kind) == "any_of" {
                ConditionTerm::AnyOf(terms)
            } else {
                ConditionTerm::AllOf(terms)
            }
        }
        "inverted" => {
            let term = object.take_required("term")?;
            ConditionTerm::Inverted(leak(condition(&object.child("term"), &term)?))
        }
        "random_chance" => {
            let chance = object.take_required("chance")?;
            ConditionTerm::RandomChance(constant_number(&object.child("chance"), &chance)?)
        }
        "weather_check" => ConditionTerm::WeatherCheck {
            raining: object.optional_bool("raining")?,
            thundering: object.optional_bool("thundering")?,
        },
        // Conditions that need a context parameter the command source does not
        // carry, or a Foton system that does not exist yet, are refused by
        // name instead of being guessed at.
        other => {
            return Err(format!(
                "{path}: the condition `{other}` is not supported by Foton's datapack predicates"
            ));
        }
    };
    object.finish()?;
    Ok(term)
}

/// Vanilla parity: `NumberProviders.CODEC`, narrowed to a plain number or the
/// `minecraft:constant` provider, which is all a chance needs to be fixed.
fn constant_number(path: &str, value: &Value) -> Parsed<f32> {
    let number = match value {
        Value::Number(number) => number.as_f64(),
        Value::Object(_) => {
            let mut object = Object::new(path, value)?;
            let kind = object.optional_string("type")?;
            if kind.as_deref().map_or("constant", strip_minecraft) != "constant" {
                return Err(format!(
                    "{path}: only constant number providers are supported"
                ));
            }
            let value = object.take_required("value")?;
            object.finish()?;
            value.as_f64()
        }
        _ => None,
    };
    number
        .map(|number| number as f32)
        .ok_or_else(|| format!("{path}: expected a number"))
}

fn entity_predicate(path: &str, value: &Value) -> Parsed<EntityPredicate> {
    let mut object = Object::new(path, value)?;
    let mut predicate = EntityPredicate::ANY;

    if let Some(types) = object.take_sub("entity_type") {
        predicate.entity_type = Some(registry_set(&object.child("entity_type"), &types)?);
    }
    if let Some(location) = object.take_sub("location") {
        let location = location_predicate(&object.child("location"), &location)?;
        predicate.location = Some(leak(location));
    }
    if let Some(location) = object.take_sub("stepping_on") {
        let location = location_predicate(&object.child("stepping_on"), &location)?;
        predicate.stepping_on = Some(leak(location));
    }
    if let Some(distance) = object.take_sub("distance") {
        predicate.distance = Some(distance_predicate(&object.child("distance"), &distance)?);
    }
    if let Some(flags) = object.take_sub("flags") {
        predicate.flags = Some(flags_predicate(&object.child("flags"), &flags)?);
    }
    if let Some(equipment) = object.take_sub("equipment") {
        let equipment = equipment_predicate(&object.child("equipment"), &equipment)?;
        predicate.equipment = Some(leak(equipment));
    }
    if let Some(components) = object.take_sub("components") {
        predicate.components = entity_components(&object.child("components"), &components)?;
    }
    if let Some(vehicle) = object.take_sub("vehicle") {
        let vehicle = entity_predicate(&object.child("vehicle"), &vehicle)?;
        predicate.vehicle = Some(leak(vehicle));
    }
    if let Some(passenger) = object.take_sub("passenger") {
        let passenger = entity_predicate(&object.child("passenger"), &passenger)?;
        predicate.passenger = Some(leak(passenger));
    }
    if let Some(player) = object.take_sub("type_specific/player") {
        let path = object.child("type_specific/player");
        let mut player = Object::new(&path, &player)?;
        if let Some(target) = player.take("looking_at") {
            let target = entity_predicate(&player.child("looking_at"), &target)?;
            predicate.looking_at = Some(leak(target));
        }
        player.finish()?;
    }
    if let Some(lightning) = object.take_sub("type_specific/lightning") {
        let path = object.child("type_specific/lightning");
        let mut lightning = Object::new(&path, &lightning)?;
        if let Some(blocks) = lightning.take("blocks_set_on_fire") {
            predicate.lightning_blocks_set_on_fire =
                Some(int_bounds(&lightning.child("blocks_set_on_fire"), &blocks)?);
        }
        lightning.finish()?;
    }

    object.finish()?;
    Ok(predicate)
}

fn flags_predicate(path: &str, value: &Value) -> Parsed<EntityFlagsPredicate> {
    let mut object = Object::new(path, value)?;
    let flags = EntityFlagsPredicate {
        is_on_fire: object.optional_bool("is_on_fire")?,
        is_sneaking: object.optional_bool("is_sneaking")?,
        is_sprinting: object.optional_bool("is_sprinting")?,
        is_swimming: object.optional_bool("is_swimming")?,
        is_baby: object.optional_bool("is_baby")?,
        is_flying: object.optional_bool("is_flying")?,
    };
    object.finish()?;
    Ok(flags)
}

fn equipment_predicate(path: &str, value: &Value) -> Parsed<EntityEquipmentPredicate> {
    let mut object = Object::new(path, value)?;
    let mut slot = |key: &str| -> Parsed<Option<ItemPredicate>> {
        object
            .take(key)
            .map(|stack| item_predicate(&format!("{path}.{key}"), &stack))
            .transpose()
    };
    let equipment = EntityEquipmentPredicate {
        head: slot("head")?,
        chest: slot("chest")?,
        legs: slot("legs")?,
        feet: slot("feet")?,
        mainhand: slot("mainhand")?,
        offhand: slot("offhand")?,
        body: slot("body")?,
    };
    object.finish()?;
    Ok(equipment)
}

/// Vanilla parity: `ItemPredicate.CODEC`, decoded whole by the same code that
/// reads item predicates out of lock components, so every `components` and
/// `predicates` entry vanilla knows is honored rather than the few that the
/// advancement model has fields for.
fn item_predicate(path: &str, value: &Value) -> Parsed<ItemPredicate> {
    if !value.is_object() {
        return Err(format!("{path}: expected an object"));
    }
    let tag = json_to_nbt(path, value)?;
    let full = FullItemPredicate::from_nbt(&tag)
        .ok_or_else(|| format!("{path}: not a valid item predicate"))?;
    Ok(ItemPredicate {
        datapack: Some(leak(full)),
        ..ItemPredicate::ANY
    })
}

fn entity_components(path: &str, value: &Value) -> Parsed<&'static [EntityComponentMatch]> {
    let object = Object::new(path, value)?;
    let components = object
        .entries
        .iter()
        .map(|(key, expected)| {
            let Value::String(expected) = expected else {
                return Err(format!(
                    "{path}.{key}: only registry-keyed component values are supported"
                ));
            };
            Ok(EntityComponentMatch {
                component: leak_str(key),
                value: identifier(&format!("{path}.{key}"), expected)?,
            })
        })
        .collect::<Parsed<Vec<_>>>()?;
    Ok(leak_slice(components))
}

fn location_predicate(path: &str, value: &Value) -> Parsed<LocationPredicate> {
    let mut object = Object::new(path, value)?;
    let mut predicate = LocationPredicate::ANY;
    if let Some(position) = object.take("position") {
        let mut position = Object::new(&object.child("position"), &position)?;
        for (axis, target) in [
            ("x", &mut predicate.x),
            ("y", &mut predicate.y),
            ("z", &mut predicate.z),
        ] {
            if let Some(bounds) = position.take(axis) {
                *target = double_bounds(&position.child(axis), &bounds)?;
            }
        }
        position.finish()?;
    }
    if let Some(biomes) = object.take("biomes") {
        predicate.biomes = Some(registry_set(&object.child("biomes"), &biomes)?);
    }
    if let Some(structures) = object.take("structures") {
        predicate.structures = Some(registry_set(&object.child("structures"), &structures)?);
    }
    if let Some(dimension) = object.optional_string("dimension")? {
        predicate.dimension = Some(identifier(&object.child("dimension"), &dimension)?);
    }
    if let Some(block) = object.take("block") {
        predicate.block = Some(block_predicate(&object.child("block"), &block)?);
    }
    predicate.smokey = object.optional_bool("smokey")?;
    object.finish()?;
    Ok(predicate)
}

fn block_predicate(path: &str, value: &Value) -> Parsed<BlockPredicate> {
    let mut object = Object::new(path, value)?;
    let blocks = object
        .take("blocks")
        .map(|blocks| registry_set(&object.child("blocks"), &blocks))
        .transpose()?;
    let state = match object.take("state") {
        Some(state) => state_properties(&object.child("state"), &state)?,
        None => &[],
    };
    object.finish()?;
    Ok(BlockPredicate { blocks, state })
}

/// Vanilla parity: `StatePropertiesPredicate`, exact values only.
fn state_properties(path: &str, value: &Value) -> Parsed<&'static [StatePropertyMatch]> {
    let object = Object::new(path, value)?;
    let properties = object
        .entries
        .iter()
        .map(|(name, expected)| {
            let expected = match expected {
                Value::String(text) => text.clone(),
                Value::Bool(flag) => flag.to_string(),
                Value::Number(number) => number.to_string(),
                _ => {
                    return Err(format!(
                        "{path}.{name}: ranged state values are not supported"
                    ));
                }
            };
            Ok(StatePropertyMatch {
                name: leak_str(name),
                value: leak_str(&expected),
            })
        })
        .collect::<Parsed<Vec<_>>>()?;
    Ok(leak_slice(properties))
}

fn distance_predicate(path: &str, value: &Value) -> Parsed<DistancePredicate> {
    let mut object = Object::new(path, value)?;
    let mut axis = |key: &str| -> Parsed<DoubleBounds> {
        object.take(key).map_or(Ok(DoubleBounds::ANY), |value| {
            double_bounds(&format!("{path}.{key}"), &value)
        })
    };
    let distance = DistancePredicate {
        x: axis("x")?,
        y: axis("y")?,
        z: axis("z")?,
        horizontal: axis("horizontal")?,
        absolute: axis("absolute")?,
    };
    object.finish()?;
    Ok(distance)
}

/// Vanilla parity: the `HolderSet` codec -- `"#tag"`, one id, or a list of ids.
fn registry_set(path: &str, value: &Value) -> Parsed<RegistrySet> {
    match value {
        Value::String(text) => match text.strip_prefix('#') {
            Some(tag) => Ok(RegistrySet::Tag(identifier(path, tag)?)),
            None => Ok(RegistrySet::Entries(leak_slice(vec![identifier(
                path, text,
            )?]))),
        },
        Value::Array(list) => {
            let entries = list
                .iter()
                .map(|entry| match entry {
                    Value::String(text) if !text.starts_with('#') => identifier(path, text),
                    _ => Err(format!(
                        "{path}: a list holds ids, not tags or other values"
                    )),
                })
                .collect::<Parsed<Vec<_>>>()?;
            Ok(RegistrySet::Entries(leak_slice(entries)))
        }
        _ => Err(format!("{path}: expected an id, a #tag or a list of ids")),
    }
}

/// Vanilla parity: `MinMaxBounds`, a bare number or a `{min, max}` object.
fn bounds<T: Copy>(
    path: &str,
    value: &Value,
    read: impl Fn(&Value) -> Option<T>,
) -> Parsed<(Option<T>, Option<T>)> {
    if let Some(exact) = read(value) {
        return Ok((Some(exact), Some(exact)));
    }
    let mut object = Object::new(path, value)?;
    let mut bound = |key: &str| -> Parsed<Option<T>> {
        object.take(key).map_or(Ok(None), |bound| {
            read(&bound)
                .map(Some)
                .ok_or_else(|| format!("{path}.{key}: expected a number"))
        })
    };
    let range = (bound("min")?, bound("max")?);
    object.finish()?;
    Ok(range)
}

fn double_bounds(path: &str, value: &Value) -> Parsed<DoubleBounds> {
    let (min, max) = bounds(path, value, Value::as_f64)?;
    Ok(DoubleBounds { min, max })
}

fn int_bounds(path: &str, value: &Value) -> Parsed<IntBounds> {
    let read = |value: &Value| value.as_i64().and_then(|number| i32::try_from(number).ok());
    let (min, max) = bounds(path, value, read)?;
    Ok(IntBounds { min, max })
}

fn identifier(path: &str, text: &str) -> Parsed<Identifier> {
    text.parse::<Identifier>()
        .map_err(|error| format!("{path}: invalid id '{text}': {error}"))
}

fn strip_minecraft(text: &str) -> &str {
    text.strip_prefix("minecraft:").unwrap_or(text)
}

/// A JSON object read key by key, which refuses to leave any key behind.
struct Object {
    path: String,
    entries: Map<String, Value>,
}

impl Object {
    fn new(path: &str, value: &Value) -> Parsed<Self> {
        match value {
            Value::Object(entries) => Ok(Self {
                path: path.to_owned(),
                entries: entries.clone(),
            }),
            _ => Err(format!("{path}: expected an object")),
        }
    }

    fn child(&self, key: &str) -> String {
        format!("{}.{key}", self.path)
    }

    fn take(&mut self, key: &str) -> Option<Value> {
        self.entries.remove(key)
    }

    /// Takes an entity sub-predicate, which may be spelled with or without the
    /// `minecraft:` namespace because the map is keyed by registry id.
    fn take_sub(&mut self, key: &str) -> Option<Value> {
        self.entries
            .remove(key)
            .or_else(|| self.entries.remove(&format!("minecraft:{key}")))
    }

    fn take_required(&mut self, key: &str) -> Parsed<Value> {
        self.take(key)
            .ok_or_else(|| format!("{}: required key `{key}` is missing", self.path))
    }

    fn string(&mut self, key: &str) -> Parsed<String> {
        self.optional_string(key)?
            .ok_or_else(|| format!("{}: required key `{key}` is missing", self.path))
    }

    fn optional_string(&mut self, key: &str) -> Parsed<Option<String>> {
        match self.take(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(_) => Err(format!("{}: expected a string", self.child(key))),
        }
    }

    fn optional_bool(&mut self, key: &str) -> Parsed<Option<bool>> {
        match self.take(key) {
            None => Ok(None),
            Some(Value::Bool(flag)) => Ok(Some(flag)),
            Some(_) => Err(format!("{}: expected a boolean", self.child(key))),
        }
    }

    fn optional_i32(&mut self, key: &str) -> Parsed<Option<i32>> {
        match self.take(key) {
            None => Ok(None),
            Some(value) => value
                .as_i64()
                .and_then(|number| i32::try_from(number).ok())
                .map(Some)
                .ok_or_else(|| format!("{}: expected an integer", self.child(key))),
        }
    }

    fn finish(self) -> Parsed<()> {
        if self.entries.is_empty() {
            return Ok(());
        }
        Err(format!(
            "{}: unsupported keys {:?}; Foton refuses the predicate rather than load one that \
             asks for less than the datapack wrote",
            self.path,
            self.entries.keys().collect::<Vec<_>>()
        ))
    }
}
