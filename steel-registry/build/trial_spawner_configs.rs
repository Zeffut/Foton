#![expect(
    clippy::unwrap_used,
    reason = "build script must fail immediately on invalid extracted trial spawner data"
)]

//! Generates the `minecraft:trial_spawner` datapack registry.
//!
//! Vanilla parity: `TrialSpawnerConfigs.bootstrap`, whose output is the builtin
//! datapack this reads. Fields the JSON omits fall back to the codec defaults
//! from `TrialSpawnerConfig.Builder`, which live in
//! `steel-registry/src/trial_spawner_config.rs`.

use std::{fs, path::Path};

use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use serde::Deserialize;
use serde_json::{Map, Value};

const CONFIG_DIR: &str = "../steel-utils/build_assets/builtin_datapacks/minecraft/trial_spawner";

#[derive(Deserialize, Debug)]
struct TrialSpawnerConfigJson {
    spawn_range: Option<i32>,
    total_mobs: Option<f32>,
    simultaneous_mobs: Option<f32>,
    total_mobs_added_per_player: Option<f32>,
    simultaneous_mobs_added_per_player: Option<f32>,
    ticks_between_spawn: Option<i32>,
    spawn_potentials: Option<Vec<WeightedJson<SpawnDataJson>>>,
    loot_tables_to_eject: Option<Vec<WeightedJson<String>>>,
    items_to_drop_when_ominous: Option<String>,
}

#[derive(Deserialize, Debug)]
struct WeightedJson<T> {
    data: T,
    #[serde(default = "one")]
    weight: i32,
}

const fn one() -> i32 {
    1
}

#[derive(Deserialize, Debug)]
struct SpawnDataJson {
    entity: Map<String, Value>,
    custom_spawn_rules: Option<CustomSpawnRulesJson>,
    equipment: Option<EquipmentTableJson>,
}

#[derive(Deserialize, Debug)]
struct CustomSpawnRulesJson {
    block_light_limit: Option<LightLimitJson>,
    sky_light_limit: Option<LightLimitJson>,
}

#[derive(Deserialize, Debug)]
struct LightLimitJson {
    #[serde(default)]
    min_inclusive: i32,
    #[serde(default = "fifteen")]
    max_inclusive: i32,
}

const fn fifteen() -> i32 {
    15
}

#[derive(Deserialize, Debug)]
struct EquipmentTableJson {
    loot_table: String,
    /// Vanilla's `DROP_CHANCES_CODEC` also accepts a slot-keyed map. Every
    /// extracted trial spawner uses the single-float form; the map form makes
    /// this build fail rather than be silently dropped.
    slot_drop_chances: Option<f32>,
}

/// Turns `minecraft:spawners/trial_chamber/key` into the generated loot-table const.
///
/// Mirrors the naming in `steel-registry/build/loot_tables/mod.rs`.
fn loot_table_ident(key: &str) -> Ident {
    let path = key.strip_prefix("minecraft:").unwrap_or(key);
    Ident::new(
        &path.replace('/', "_").to_shouty_snake_case(),
        Span::call_site(),
    )
}

fn loot_table_ref(key: &str) -> TokenStream {
    let ident = loot_table_ident(key);
    quote! { &vanilla_loot_tables::#ident as LootTableRef }
}

/// Emits the NBT insert for one entry of a spawner's `entity` tag.
///
/// Vanilla converts this JSON with `NbtOps.createNumeric`, which makes every
/// number a double and a boolean a byte. Steel's entity loaders read typed
/// tags, so an integer is emitted as an int and a boolean as a byte -- the tag
/// type differs from vanilla's in-memory form, the mob that comes out does not.
fn entity_tag_insert(name: &str, value: &Value) -> TokenStream {
    match value {
        Value::String(text) => quote! { entity.insert(#name, #text); },
        Value::Bool(flag) => {
            let byte = i8::from(*flag);
            quote! { entity.insert(#name, #byte); }
        }
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                let int = i32::try_from(int)
                    .unwrap_or_else(|_| panic!("spawner entity tag {name} does not fit an int"));
                quote! { entity.insert(#name, #int); }
            } else {
                let double = number.as_f64().unwrap();
                quote! { entity.insert(#name, #double); }
            }
        }
        other => panic!("unsupported spawner entity tag {name}: {other}"),
    }
}

fn spawn_data_tokens(data: &SpawnDataJson) -> TokenStream {
    let inserts = data
        .entity
        .iter()
        .map(|(name, value)| entity_tag_insert(name, value));

    let custom_spawn_rules = match &data.custom_spawn_rules {
        None => quote! { None },
        Some(rules) => {
            let block = light_limit_tokens(rules.block_light_limit.as_ref());
            let sky = light_limit_tokens(rules.sky_light_limit.as_ref());
            quote! {
                Some(CustomSpawnRules {
                    block_light_limit: #block,
                    sky_light_limit: #sky,
                })
            }
        }
    };

    let equipment = match &data.equipment {
        None => quote! { None },
        Some(equipment) => {
            let table = loot_table_ref(&equipment.loot_table);
            let chance = equipment.slot_drop_chances.unwrap_or_else(|| {
                panic!(
                    "equipment table {} needs the single-float slot_drop_chances form",
                    equipment.loot_table
                )
            });
            quote! { Some(EquipmentTable::with_uniform_drop_chance(#table, #chance)) }
        }
    };

    quote! {
        {
            let mut entity = NbtCompound::new();
            #(#inserts)*
            SpawnData::new(entity, #custom_spawn_rules, #equipment)
        }
    }
}

fn light_limit_tokens(limit: Option<&LightLimitJson>) -> TokenStream {
    match limit {
        None => quote! { LIGHT_RANGE },
        Some(limit) => {
            let min = limit.min_inclusive;
            let max = limit.max_inclusive;
            quote! { InclusiveRange::new(#min, #max) }
        }
    }
}

fn config_tokens(config: &TrialSpawnerConfigJson) -> TokenStream {
    let spawn_range = config
        .spawn_range
        .map_or_else(|| quote! { DEFAULT_SPAWN_RANGE }, |value| quote! { #value });
    let total_mobs = config
        .total_mobs
        .map_or_else(|| quote! { DEFAULT_TOTAL_MOBS }, |value| quote! { #value });
    let simultaneous_mobs = config.simultaneous_mobs.map_or_else(
        || quote! { DEFAULT_SIMULTANEOUS_MOBS },
        |value| quote! { #value },
    );
    let total_added = config.total_mobs_added_per_player.map_or_else(
        || quote! { DEFAULT_TOTAL_MOBS_ADDED_PER_PLAYER },
        |value| quote! { #value },
    );
    let simultaneous_added = config.simultaneous_mobs_added_per_player.map_or_else(
        || quote! { DEFAULT_SIMULTANEOUS_MOBS_ADDED_PER_PLAYER },
        |value| quote! { #value },
    );
    let ticks_between_spawn = config.ticks_between_spawn.map_or_else(
        || quote! { DEFAULT_TICKS_BETWEEN_SPAWN },
        |value| quote! { #value },
    );

    let spawn_potentials = match &config.spawn_potentials {
        None => quote! { WeightedList::empty() },
        Some(entries) => {
            let entries = entries.iter().map(|entry| {
                let value = spawn_data_tokens(&entry.data);
                let weight = entry.weight;
                quote! { Weighted { value: #value, weight: #weight } }
            });
            quote! { WeightedList::new(vec![#(#entries),*]) }
        }
    };

    let loot_tables_to_eject = match &config.loot_tables_to_eject {
        None => quote! { default_loot_tables_to_eject() },
        Some(entries) => {
            let entries = entries.iter().map(|entry| {
                let value = loot_table_ref(&entry.data);
                let weight = entry.weight;
                quote! { Weighted { value: #value, weight: #weight } }
            });
            quote! { WeightedList::new(vec![#(#entries),*]) }
        }
    };

    let items_to_drop_when_ominous = config.items_to_drop_when_ominous.as_ref().map_or_else(
        || quote! { &vanilla_loot_tables::SPAWNERS_TRIAL_CHAMBER_ITEMS_TO_DROP_WHEN_OMINOUS as LootTableRef },
        |key| loot_table_ref(key),
    );

    quote! {
        TrialSpawnerConfig {
            spawn_range: #spawn_range,
            total_mobs: #total_mobs,
            simultaneous_mobs: #simultaneous_mobs,
            total_mobs_added_per_player: #total_added,
            simultaneous_mobs_added_per_player: #simultaneous_added,
            ticks_between_spawn: #ticks_between_spawn,
            spawn_potentials: #spawn_potentials,
            loot_tables_to_eject: #loot_tables_to_eject,
            items_to_drop_when_ominous: #items_to_drop_when_ominous,
        }
    }
}

fn collect(dir: &Path, base: &Path, configs: &mut Vec<(String, TrialSpawnerConfigJson)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, base, configs);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let key = path
            .strip_prefix(base)
            .unwrap()
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        let content = fs::read_to_string(&path).unwrap();
        let config: TrialSpawnerConfigJson = serde_json::from_str(&content)
            .unwrap_or_else(|error| panic!("failed to parse trial spawner {key}: {error}"));
        configs.push((key, config));
    }
}

pub(crate) fn build() -> TokenStream {
    println!("cargo:rerun-if-changed={CONFIG_DIR}");
    let base = Path::new(CONFIG_DIR);
    let mut configs = Vec::new();
    collect(base, base, &mut configs);
    configs.sort_by(|left, right| left.0.cmp(&right.0));
    assert!(
        !configs.is_empty(),
        "no trial spawner configs found in {CONFIG_DIR}"
    );

    let inserts = configs.iter().map(|(key, config)| {
        let tokens = config_tokens(config);
        quote! {
            configs.insert(Identifier::vanilla_static(#key), #tokens);
        }
    });

    quote! {
        use std::sync::LazyLock;

        use rustc_hash::FxHashMap;
        use simdnbt::owned::NbtCompound;
        use steel_utils::Identifier;
        use steel_utils::random::weighted_list::{Weighted, WeightedList};
        use steel_utils::types::InclusiveRange;

        use crate::loot_table::LootTableRef;
        use crate::spawn_data::{CustomSpawnRules, EquipmentTable, LIGHT_RANGE, SpawnData};
        use crate::trial_spawner_config::{
            DEFAULT_SIMULTANEOUS_MOBS, DEFAULT_SIMULTANEOUS_MOBS_ADDED_PER_PLAYER,
            DEFAULT_SPAWN_RANGE, DEFAULT_TICKS_BETWEEN_SPAWN, DEFAULT_TOTAL_MOBS,
            DEFAULT_TOTAL_MOBS_ADDED_PER_PLAYER, TrialSpawnerConfig, default_loot_tables_to_eject,
        };
        use crate::vanilla_loot_tables;

        static CONFIGS: LazyLock<FxHashMap<Identifier, TrialSpawnerConfig>> = LazyLock::new(|| {
            let mut configs: FxHashMap<Identifier, TrialSpawnerConfig> = FxHashMap::default();
            #(#inserts)*
            configs
        });

        /// Returns the registered trial spawner configuration under `key`.
        #[must_use]
        pub fn by_key(key: &Identifier) -> Option<&'static TrialSpawnerConfig> {
            CONFIGS.get(key)
        }

        /// Returns every registered key, for tests and diagnostics.
        pub fn keys() -> impl Iterator<Item = &'static Identifier> {
            CONFIGS.keys()
        }
    }
}
