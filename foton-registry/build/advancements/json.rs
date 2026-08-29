//! The datapack shapes an advancement file is allowed to have.
//!
//! Everything that can be a plain struct is one, with `deny_unknown_fields`, so
//! a field vanilla adds later fails the build instead of being dropped. The
//! parts whose keys are dispatched at runtime -- criterion conditions, the
//! `predicates`/`components` maps of an item predicate, the sub-predicate map
//! of an entity predicate -- go through [`ObjectReader`], which panics on any
//! key nothing consumed.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::shared_structs::TextComponentJson;

/// One advancement file.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AdvancementJson {
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub display: Option<DisplayJson>,
    #[serde(default)]
    pub rewards: Option<RewardsJson>,
    pub criteria: BTreeMap<String, CriterionJson>,
    #[serde(default)]
    pub requirements: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub sends_telemetry_event: bool,
}

/// The `display` block.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct DisplayJson {
    pub icon: IconJson,
    pub title: TextComponentJson,
    pub description: TextComponentJson,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub frame: Option<String>,
    #[serde(default)]
    pub show_toast: Option<bool>,
    #[serde(default)]
    pub announce_to_chat: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

/// The `display.icon` block.
///
/// Vanilla's codec also accepts a bare item id string; vanilla's own data
/// never uses that form, and the build fails loudly if it starts to.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct IconJson {
    pub id: String,
    #[serde(default)]
    pub count: Option<i64>,
    #[serde(default)]
    pub components: Option<Map<String, Value>>,
}

/// The `rewards` block.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RewardsJson {
    #[serde(default)]
    pub experience: Option<i32>,
    #[serde(default)]
    pub loot: Option<Vec<String>>,
    #[serde(default)]
    pub recipes: Option<Vec<String>>,
    #[serde(default)]
    pub function: Option<String>,
}

/// One entry of the `criteria` map.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CriterionJson {
    pub trigger: String,
    #[serde(default)]
    pub conditions: Option<Value>,
}

/// A JSON object read key by key, which refuses to leave any key behind.
///
/// The point is that a condition Foton cannot model must stop the build. A
/// criterion that quietly forgets half its conditions hands the advancement
/// out for the wrong thing, which is worse than not shipping it at all.
pub struct ObjectReader {
    path: String,
    entries: Map<String, Value>,
}

impl ObjectReader {
    /// Reads an object at `path`. An absent or null value is an empty object,
    /// which is how vanilla's `dispatchOptionalValue` treats a missing
    /// `conditions` block.
    ///
    /// # Panics
    /// If the value is present but is not an object.
    pub fn new(path: impl Into<String>, value: Option<&Value>) -> Self {
        let path = path.into();
        let entries = match value {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            Some(other) => panic!("{path}: expected an object, found {other}"),
        };
        Self { path, entries }
    }

    /// The path of a child key.
    pub fn child_path(&self, key: &str) -> String {
        format!("{}.{key}", self.path)
    }

    /// Whether the object had no keys at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Removes and returns one key.
    pub fn take(&mut self, key: &str) -> Option<Value> {
        self.entries.remove(key)
    }

    /// Removes one key, panicking if it is absent.
    ///
    /// # Panics
    /// If the key is missing.
    pub fn take_required(&mut self, key: &str) -> Value {
        self.entries
            .remove(key)
            .unwrap_or_else(|| panic!("{}: required key `{key}` is missing", self.path))
    }

    /// Removes one key and deserializes it.
    ///
    /// # Panics
    /// If the value does not fit `T`.
    pub fn take_as<T: serde::de::DeserializeOwned>(&mut self, key: &str) -> Option<T> {
        let value = self.entries.remove(key)?;
        Some(serde_json::from_value(value).unwrap_or_else(|e| panic!("{}.{key}: {e}", self.path)))
    }

    /// Removes one key and reads it as a nested object.
    pub fn take_object(&mut self, key: &str) -> Option<Self> {
        let value = self.entries.remove(key)?;
        Some(Self::new(self.child_path(key), Some(&value)))
    }

    /// Every remaining key, consumed, so a caller can dispatch on open maps.
    pub fn drain(self) -> (String, Map<String, Value>) {
        (self.path, self.entries)
    }

    /// Asserts that nothing was left unread.
    ///
    /// # Panics
    /// If any key was not consumed.
    pub fn finish(self) {
        assert!(
            self.entries.is_empty(),
            "{}: unmodeled keys {:?}. Model them or the criterion silently \
             stops asking for them.",
            self.path,
            self.entries.keys().collect::<Vec<_>>()
        );
    }
}

/// A registry set written as one id, one `#tag`, or a list of ids.
///
/// Vanilla parity: the `HolderSet` codec.
#[derive(Debug, Clone)]
pub enum RegistrySetJson {
    /// `"#minecraft:beehives"`.
    Tag(String),
    /// `"minecraft:stone"` or `["minecraft:stone", "minecraft:granite"]`.
    Entries(Vec<String>),
}

impl RegistrySetJson {
    /// Reads the value of an `items` / `blocks` / `biomes` style field.
    ///
    /// # Panics
    /// If the value is neither a string nor a list of strings, or if a list
    /// contains a tag -- vanilla's `HolderSet` list form cannot hold tags.
    pub fn parse(path: &str, value: &Value) -> Self {
        match value {
            Value::String(one) => match one.strip_prefix('#') {
                Some(tag) => Self::Tag(tag.to_owned()),
                None => Self::Entries(vec![one.clone()]),
            },
            Value::Array(many) => Self::Entries(
                many.iter()
                    .map(|entry| {
                        let Value::String(entry) = entry else {
                            panic!("{path}: registry set entries must be strings, found {entry}");
                        };
                        assert!(
                            !entry.starts_with('#'),
                            "{path}: a registry set list cannot contain the tag {entry}"
                        );
                        entry.clone()
                    })
                    .collect(),
            ),
            other => panic!("{path}: expected a registry set, found {other}"),
        }
    }
}

/// A `MinMaxBounds` written as a bare value or as a `{min, max}` object.
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct RangeJson<T> {
    #[serde(default = "none")]
    pub min: Option<T>,
    #[serde(default = "none")]
    pub max: Option<T>,
}

const fn none<T>() -> Option<T> {
    None
}

/// Either an exact value or a range.
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(untagged)]
pub enum BoundsJson<T> {
    Exact(T),
    Range(RangeJson<T>),
}

impl<T: Copy> BoundsJson<T> {
    /// The pair of bounds this value stands for.
    ///
    /// Vanilla parity: `MinMaxBounds`'s codec, where a bare value means both
    /// bounds at once.
    pub const fn min_max(self) -> (Option<T>, Option<T>) {
        match self {
            Self::Exact(value) => (Some(value), Some(value)),
            Self::Range(range) => (range.min, range.max),
        }
    }
}

/// An `{x, y, z}` block of bounds.
#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(deny_unknown_fields)]
pub struct PositionJson {
    #[serde(default = "none")]
    pub x: Option<BoundsJson<f64>>,
    #[serde(default = "none")]
    pub y: Option<BoundsJson<f64>>,
    #[serde(default = "none")]
    pub z: Option<BoundsJson<f64>>,
}

/// A `DistancePredicate` block.
#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(deny_unknown_fields)]
pub struct DistanceJson {
    #[serde(default = "none")]
    pub x: Option<BoundsJson<f64>>,
    #[serde(default = "none")]
    pub y: Option<BoundsJson<f64>>,
    #[serde(default = "none")]
    pub z: Option<BoundsJson<f64>>,
    #[serde(default = "none")]
    pub horizontal: Option<BoundsJson<f64>>,
    #[serde(default = "none")]
    pub absolute: Option<BoundsJson<f64>>,
}
