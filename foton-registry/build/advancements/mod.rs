//! Generates the vanilla advancement definitions.
//!
//! Reads every file of the built-in datapack's `advancement/` tree, lowers the
//! criteria into typed triggers and predicates, runs vanilla's tree layout so
//! the client draws the icons where vanilla does, and emits one `static` per
//! advancement plus a registration function.

mod json;
mod layout;
mod predicates;
mod triggers;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use heck::ToShoutySnakeCase as _;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use serde_json::{Map, Value};

use crate::generator_functions::generate_text_component;

use json::{AdvancementJson, DisplayJson, IconJson};
use predicates::identifier;

const ADVANCEMENT_DIR: &str = "../foton-utils/build_assets/builtin_datapacks/minecraft/advancement";

struct Entry {
    /// The registry path, such as `story/mine_stone`.
    key: String,
    advancement: AdvancementJson,
}

pub(crate) fn build() -> TokenStream {
    println!("cargo:rerun-if-changed={ADVANCEMENT_DIR}");

    let base = Path::new(ADVANCEMENT_DIR);
    let mut entries = Vec::new();
    read_dir(base, base, &mut entries);
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    assert!(
        !entries.is_empty(),
        "no advancements found under {ADVANCEMENT_DIR}; the built-in datapack is missing"
    );

    let positions = layout_positions(&entries);

    let mut stream = quote! {
        use foton_utils::Identifier;
        use text_components::{TextComponent, translation::TranslatedMessage};

        use crate::advancement::{
            Advancement, AdvancementIcon, AdvancementRegistry, AdvancementRequirements,
            AdvancementRewards, AdvancementType, Criterion, DisplayInfo,
        };
        use crate::advancement::predicate::{
            BannerPatternLayer, BlockPredicate, ConditionTerm, DamagePredicate,
            DamageSourcePredicate, DamageTypeTagMatch, DistancePredicate, DoubleBounds,
            EnchantmentPredicate, EntityComponentMatch, EntityEquipmentPredicate,
            EntityFlagsPredicate, EntityPredicate, IntBounds, ItemComponentsPredicate,
            ItemPredicate, LocationPredicate, RegistrySet, StatePropertyMatch,
        };
        use crate::advancement::trigger::{MobEffectMatch, SlotsPredicate, TriggerInstance};
    };

    let mut register_stream = TokenStream::new();
    for (index, entry) in entries.iter().enumerate() {
        let ident = advancement_ident(&entry.key);
        let value = generate_advancement(entry, positions.get(&index).copied());
        stream.extend(quote! {
            pub static #ident: Advancement = #value;
        });
        register_stream.extend(quote! { registry.register(&#ident); });
    }

    let count = entries.len();
    stream.extend(quote! {
        /// How many advancements the built-in datapack defines.
        ///
        /// A registry that ends up with a different number has lost entries,
        /// and a registry that has lost entries answers `None` to lookups that
        /// should succeed.
        pub const VANILLA_ADVANCEMENT_COUNT: usize = #count;

        pub fn register_advancements(registry: &mut AdvancementRegistry) {
            #register_stream
        }
    });

    stream
}

fn read_dir(dir: &Path, base: &Path, entries: &mut Vec<Entry>) {
    let listing =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in listing {
        let path = entry.expect("directory entry should be readable").path();
        if path.is_dir() {
            read_dir(&path, base, entries);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }

        let key = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let advancement: AdvancementJson = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("failed to parse advancement {key}: {e}"));
        entries.push(Entry { key, advancement });
    }
}

/// Runs vanilla's tree layout over every visible root.
///
/// Vanilla parity: the `for (AdvancementNode root : tree.roots())` loop of
/// `ServerAdvancementManager.apply`, which positions only the roots that are
/// drawn at all.
fn layout_positions(entries: &[Entry]) -> BTreeMap<usize, (f32, f32)> {
    let index_of: BTreeMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.key.as_str(), index))
        .collect();

    let mut children_of = vec![Vec::new(); entries.len()];
    let mut roots = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(parent) = entry.advancement.parent.as_deref() else {
            roots.push(index);
            continue;
        };
        let parent_key = parent.strip_prefix("minecraft:").unwrap_or(parent);
        let parent_index = index_of.get(parent_key).copied().unwrap_or_else(|| {
            panic!(
                "advancement {} declares the parent {parent}, which no advancement file defines",
                entry.key
            )
        });
        children_of[parent_index].push(index);
    }
    // Entries were sorted by key before this runs, so every child list is
    // already in key order; that is what makes the layout reproducible.

    let has_display: Vec<bool> = entries
        .iter()
        .map(|entry| entry.advancement.display.is_some())
        .collect();

    let mut positions = BTreeMap::new();
    for root in roots {
        if !has_display[root] {
            continue;
        }
        for (index, x, y) in layout::run(root, &children_of, &has_display) {
            positions.insert(index, (x, y));
        }
    }
    positions
}

fn advancement_ident(key: &str) -> Ident {
    Ident::new(
        &key.replace('/', "_").to_shouty_snake_case(),
        Span::call_site(),
    )
}

/// The icon's item key, always fully qualified so the runtime can look it up.
fn normalized_item_id(path: &str, id: &str) -> String {
    let path_only = id.strip_prefix("minecraft:").unwrap_or(id);
    assert!(
        !path_only.contains(':'),
        "{path}: only vanilla items can be advancement icons, found {id}"
    );
    format!("minecraft:{path_only}")
}

fn generate_advancement(entry: &Entry, position: Option<(f32, f32)>) -> TokenStream {
    let advancement = &entry.advancement;
    let key = &entry.key;
    let key_tokens = quote! { Identifier::vanilla_static(#key) };

    let parent = advancement.parent.as_deref().map_or_else(
        || quote! { None },
        |parent| {
            let parent = identifier(&format!("{key}.parent"), parent);
            quote! { Some(#parent) }
        },
    );

    let display = advancement.display.as_ref().map_or_else(
        || quote! { None },
        |display| {
            let (x, y) = position.unwrap_or_else(|| {
                panic!("advancement {key} is drawn but the layout gave it no position")
            });
            let display = generate_display(key, display, x, y);
            quote! { Some(#display) }
        },
    );

    let rewards = generate_rewards(entry);

    let criteria = advancement.criteria.iter().map(|(name, criterion)| {
        let instance = triggers::trigger_instance(
            &format!("{key}.criteria.{name}"),
            &criterion.trigger,
            criterion.conditions.as_ref(),
        );
        quote! { Criterion { name: #name, trigger: #instance } }
    });

    let requirements = generate_requirements(entry);
    let sends_telemetry_event = advancement.sends_telemetry_event;

    quote! {
        Advancement {
            key: #key_tokens,
            parent: #parent,
            display: #display,
            rewards: #rewards,
            criteria: &[#(#criteria),*],
            requirements: #requirements,
            sends_telemetry_event: #sends_telemetry_event,
        }
    }
}

/// Emits the requirement matrix, defaulting the way vanilla's codec does.
///
/// Vanilla parity: `Advancement.CODEC`'s `orElseGet(() -> allOf(criteria))`,
/// which turns an absent `requirements` into one group per criterion.
fn generate_requirements(entry: &Entry) -> TokenStream {
    let advancement = &entry.advancement;
    let groups: Vec<Vec<String>> = advancement.requirements.clone().unwrap_or_else(|| {
        advancement
            .criteria
            .keys()
            .map(|name| vec![name.clone()])
            .collect()
    });

    // Vanilla's `AdvancementRequirements.validate` rejects a matrix whose names
    // do not exactly match the criteria, and a mismatch here would silently
    // make the advancement unearnable.
    let mut referenced: Vec<&str> = groups
        .iter()
        .flat_map(|group| group.iter().map(String::as_str))
        .collect();
    referenced.sort_unstable();
    referenced.dedup();
    let mut declared: Vec<&str> = advancement.criteria.keys().map(String::as_str).collect();
    declared.sort_unstable();
    assert_eq!(
        referenced, declared,
        "advancement {}: requirements and criteria do not match",
        entry.key
    );

    // The outer slice's element type drives the unsize coercion, so each group
    // is written as a plain array reference.
    let groups = groups.iter().map(|group| {
        let names = group.iter().map(|name| quote! { #name });
        quote! { &[#(#names),*] }
    });
    quote! { AdvancementRequirements { groups: &[#(#groups),*] } }
}

fn generate_rewards(entry: &Entry) -> TokenStream {
    let Some(rewards) = entry.advancement.rewards.as_ref() else {
        return quote! { AdvancementRewards::EMPTY };
    };
    let key = &entry.key;
    let experience = rewards.experience.unwrap_or(0);
    let loot = rewards
        .loot
        .iter()
        .flatten()
        .map(|table| identifier(&format!("{key}.rewards.loot"), table));
    let recipes = rewards
        .recipes
        .iter()
        .flatten()
        .map(|recipe| identifier(&format!("{key}.rewards.recipes"), recipe));
    let function = rewards.function.as_deref().map_or_else(
        || quote! { None },
        |function| {
            let function = identifier(&format!("{key}.rewards.function"), function);
            quote! { Some(#function) }
        },
    );
    quote! {
        AdvancementRewards {
            experience: #experience,
            loot: &[#(#loot),*],
            recipes: &[#(#recipes),*],
            function: #function,
        }
    }
}

fn generate_display(key: &str, display: &DisplayJson, x: f32, y: f32) -> TokenStream {
    let title = generate_text_component(&display.title);
    let description = generate_text_component(&display.description);
    let icon = generate_icon(key, &display.icon);
    let background = display.background.as_deref().map_or_else(
        || quote! { None },
        |background| {
            let background = identifier(&format!("{key}.display.background"), background);
            quote! { Some(#background) }
        },
    );
    let advancement_type = match display.frame.as_deref() {
        None | Some("task") => quote! { AdvancementType::Task },
        Some("challenge") => quote! { AdvancementType::Challenge },
        Some("goal") => quote! { AdvancementType::Goal },
        Some(other) => panic!("{key}.display.frame: unknown frame `{other}`"),
    };
    let show_toast = display.show_toast.unwrap_or(true);
    let announce_chat = display.announce_to_chat.unwrap_or(true);
    let hidden = display.hidden.unwrap_or(false);

    quote! {
        DisplayInfo {
            title: #title,
            description: #description,
            icon: #icon,
            background: #background,
            advancement_type: #advancement_type,
            show_toast: #show_toast,
            announce_chat: #announce_chat,
            hidden: #hidden,
            x: #x,
            y: #y,
        }
    }
}

fn generate_icon(key: &str, icon: &IconJson) -> TokenStream {
    let path = format!("{key}.display.icon");
    assert!(
        icon.count.is_none_or(|count| count == 1),
        "{path}: an advancement icon with a count other than one is not modeled"
    );
    let item = normalized_item_id(&path, &icon.id);
    let components = match icon.components.as_ref() {
        None => String::new(),
        Some(components) => snbt_compound(&path, components),
    };
    quote! { AdvancementIcon::new(#item, #components) }
}

/// Writes a datapack component patch as SNBT.
///
/// Vanilla's `DataComponentPatch` codec has the same field names and value
/// shapes over JSON and NBT, so the datapack's own object becomes the SNBT the
/// runtime decodes -- nothing is transcribed by hand.
fn snbt_compound(path: &str, value: &Map<String, Value>) -> String {
    let mut out = String::from("{");
    for (index, (key, entry)) in value.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&snbt_string(key));
        out.push(':');
        out.push_str(&snbt_value(&format!("{path}.{key}"), entry));
    }
    out.push('}');
    out
}

fn snbt_value(path: &str, value: &Value) -> String {
    match value {
        Value::Null => panic!("{path}: null has no NBT form"),
        Value::Bool(flag) => (if *flag { "1b" } else { "0b" }).to_owned(),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                integer.to_string()
            } else {
                format!("{}d", number.as_f64().expect("a JSON number is finite"))
            }
        }
        Value::String(text) => snbt_string(text),
        Value::Array(items) => {
            let items: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(index, item)| snbt_value(&format!("{path}[{index}]"), item))
                .collect();
            format!("[{}]", items.join(","))
        }
        Value::Object(map) => snbt_compound(path, map),
    }
}

fn snbt_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        if character == '"' || character == '\\' {
            out.push('\\');
        }
        out.push(character);
    }
    out.push('"');
    out
}
