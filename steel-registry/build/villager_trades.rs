#![expect(
    clippy::unwrap_used,
    reason = "build script must fail immediately on invalid extracted trade data"
)]

//! Generates the `minecraft:villager_trade` and `minecraft:trade_set` registries.
//!
//! Vanilla parity: `VillagerTrades.bootstrap` and `TradeSets.bootstrap`, whose
//! output is the builtin datapack this reads. Since 26.2 a villager's trades are
//! data, so nothing here is transcribed: the 388 trades, the 68 sets and the 73
//! `#villager_trade` tags that join them all come out of
//! `steel-utils/build_assets/builtin_datapacks/minecraft/`.
//!
//! A trade's `given_item_modifiers` are ordinary loot functions, so they are
//! lowered by the loot-table generator. That generator panics on a function,
//! condition or number provider it does not model, which is deliberate: a
//! villager quietly offering an unenchanted book where vanilla offers an
//! enchanted one is worse than a build that stops.

use std::{fs, path::Path};

use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::loot_tables::{LootConditionJson, LootFunctionJson, NumberProviderJson};
use crate::loot_tables::{generate_condition, generate_function, generate_number_provider};
use crate::tags::common::{read_all_tags, resolve_all_tags};

const TRADE_DIR: &str = "../steel-utils/build_assets/builtin_datapacks/minecraft/villager_trade";
const TRADE_SET_DIR: &str = "../steel-utils/build_assets/builtin_datapacks/minecraft/trade_set";
const TRADE_TAG_DIR: &str =
    "../steel-utils/build_assets/builtin_datapacks/minecraft/tags/villager_trade";

/// Vanilla parity: `TradeCost.CODEC`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TradeCostJson {
    id: String,
    #[serde(default)]
    count: Option<NumberProviderJson>,
    #[serde(default)]
    components: Option<FxHashMap<String, serde_json::Value>>,
}

/// Vanilla parity: `ItemStackTemplate.MAP_CODEC`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ItemStackTemplateJson {
    id: String,
    #[serde(default)]
    count: Option<i32>,
    #[serde(default)]
    components: Option<serde_json::Value>,
}

/// Vanilla parity: `VillagerTrade.CODEC`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct VillagerTradeJson {
    wants: TradeCostJson,
    #[serde(default)]
    additional_wants: Option<TradeCostJson>,
    gives: ItemStackTemplateJson,
    #[serde(default)]
    max_uses: Option<NumberProviderJson>,
    #[serde(default)]
    reputation_discount: Option<NumberProviderJson>,
    #[serde(default)]
    xp: Option<NumberProviderJson>,
    #[serde(default)]
    merchant_predicate: Option<LootConditionJson>,
    #[serde(default)]
    given_item_modifiers: Vec<LootFunctionJson>,
    #[serde(default)]
    double_trade_price_enchantments: Option<String>,
}

/// Vanilla parity: `TradeSet.CODEC`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TradeSetJson {
    trades: String,
    amount: NumberProviderJson,
    #[serde(default)]
    allow_duplicates: bool,
    #[serde(default)]
    random_sequence: Option<String>,
}

/// Reads every `*.json` under `dir`, keyed by its registry path.
fn read_registry_dir<T: serde::de::DeserializeOwned>(dir: &str) -> Vec<(String, T)> {
    fn walk<T: serde::de::DeserializeOwned>(
        dir: &Path,
        base: &Path,
        entries: &mut Vec<(String, T)>,
    ) {
        let read = fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()));
        for entry in read {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, base, entries);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let key = path
                .strip_prefix(base)
                .unwrap()
                .with_extension("")
                .to_str()
                .unwrap()
                .replace('\\', "/");
            let content = fs::read_to_string(&path).unwrap();
            let value: T = serde_json::from_str(&content)
                .unwrap_or_else(|error| panic!("cannot parse {key}: {error}"));
            entries.push((key, value));
        }
    }

    println!("cargo:rerun-if-changed={dir}");
    let base = Path::new(dir);
    let mut entries = Vec::new();
    assert!(base.exists(), "missing extracted registry directory {dir}");
    walk(base, base, &mut entries);
    // Directory order is filesystem order; sort so the generated file is
    // reproducible and registry ids are stable across machines.
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    entries
}

/// The `SHOUTY_SNAKE_CASE` constant a registry path becomes.
fn const_ident(key: &str) -> Ident {
    Ident::new(
        &key.replace('/', "_").to_shouty_snake_case(),
        Span::call_site(),
    )
}

/// Strips the implicit `minecraft:` namespace an `Identifier::vanilla_static` adds back.
fn vanilla_path(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

fn generate_trade_cost(cost: &TradeCostJson) -> TokenStream {
    let item = vanilla_path(&cost.id).to_shouty_snake_case();
    let item = Ident::new(&item, Span::call_site());
    let count = cost.count.as_ref().map_or_else(
        // Vanilla parity: `TradeCost.CODEC`'s `ConstantValue.exactly(1.0F)`.
        || quote! { NumberProvider::Constant(1.0) },
        generate_number_provider,
    );

    let components = match &cost.components {
        None => quote! { None },
        Some(components) => {
            assert!(
                components.len() == 1,
                "only a single-component trade cost is modeled, got {components:?}"
            );
            let (key, value) = components.iter().next().unwrap();
            assert!(
                key == "minecraft:potion_contents",
                "trade cost component `{key}` is not modeled; Steel has no \
                 JSON-to-component-value path, so lowering it would drop the requirement"
            );
            let potion = value
                .as_object()
                .and_then(|contents| {
                    assert!(
                        contents.len() == 1,
                        "only a bare `potion` is modeled in a trade cost's potion contents, \
                         got {value}"
                    );
                    contents.get("potion")
                })
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| panic!("potion contents trade cost without a potion: {value}"));
            let potion = vanilla_path(potion);
            quote! {
                Some(TradeCostComponents::Potion(Identifier::vanilla_static(#potion)))
            }
        }
    };

    quote! {
        TradeCost {
            item: &vanilla_items::#item,
            count: #count,
            components: #components,
        }
    }
}

/// Lowers the `ItemStackTemplate` a trade `gives` into an item and a count.
fn generate_gives(gives: &ItemStackTemplateJson) -> (TokenStream, i32) {
    assert!(
        gives.components.is_none(),
        "a trade giving an item with components up front is not modeled; every \
         vanilla trade builds its result through `given_item_modifiers`"
    );
    let item = vanilla_path(&gives.id).to_shouty_snake_case();
    let item = Ident::new(&item, Span::call_site());
    // Vanilla parity: `ItemStackTemplate.MAP_CODEC`'s `intRange(1, 99)` default of 1.
    let count = gives.count.unwrap_or(1);
    assert!(
        (1..=99).contains(&count),
        "a trade gives {count} items, outside vanilla's 1..=99 range"
    );
    (quote! { &vanilla_items::#item }, count)
}

/// Generates the `villager_trade` and `trade_set` registries.
pub(crate) fn build() -> TokenStream {
    let trades: Vec<(String, VillagerTradeJson)> = read_registry_dir(TRADE_DIR);
    let trade_sets: Vec<(String, TradeSetJson)> = read_registry_dir(TRADE_SET_DIR);

    println!("cargo:rerun-if-changed={TRADE_TAG_DIR}");
    let raw_tags = read_all_tags(TRADE_TAG_DIR);
    let resolved_tags: FxHashMap<String, Vec<String>> =
        resolve_all_tags(&raw_tags).into_iter().collect();

    let known_trades: FxHashMap<&str, Ident> = trades
        .iter()
        .map(|(key, _)| (key.as_str(), const_ident(key)))
        .collect();

    let mut stream = TokenStream::new();
    stream.extend(quote! {
        use crate::loot_table::{
            ConditionalLootFunction, EnchantmentOptions, EntityPredicate, ItemComponentPredicate,
            ItemFilter, ItemFilterItems, LootCondition, LootContextEntity, LootFunction,
            NameTarget, NumberProvider, NumberProviderRange, PotionOptions, StewEffect,
            ToolPredicate,
        };
        use crate::trading::{
            TradeCost, TradeCostComponents, TradeSet, TradeSetRegistry, VillagerTrade,
            VillagerTradeRegistry,
        };
        use crate::vanilla_items;
        use steel_utils::Identifier;
    });

    for (key, trade) in &trades {
        let ident = &known_trades[key.as_str()];
        let wants = generate_trade_cost(&trade.wants);
        let additional_wants = trade.additional_wants.as_ref().map_or_else(
            || quote! { None },
            |cost| {
                let cost = generate_trade_cost(cost);
                quote! { Some(#cost) }
            },
        );
        let (gives, gives_count) = generate_gives(&trade.gives);
        // Vanilla parity: the `lenientOptionalFieldOf` defaults of `VillagerTrade.CODEC`.
        let max_uses = trade.max_uses.as_ref().map_or_else(
            || quote! { NumberProvider::Constant(4.0) },
            generate_number_provider,
        );
        let reputation_discount = trade.reputation_discount.as_ref().map_or_else(
            || quote! { NumberProvider::Constant(0.0) },
            generate_number_provider,
        );
        let xp = trade.xp.as_ref().map_or_else(
            || quote! { NumberProvider::Constant(1.0) },
            generate_number_provider,
        );
        let merchant_predicate = trade.merchant_predicate.as_ref().map_or_else(
            || quote! { None },
            |condition| {
                let condition = generate_condition(condition);
                quote! { Some(#condition) }
            },
        );
        let modifiers: Vec<TokenStream> = trade
            .given_item_modifiers
            .iter()
            .map(generate_function)
            .collect();
        let double_trade_price_enchantments =
            trade.double_trade_price_enchantments.as_ref().map_or_else(
                || quote! { None },
                |set| {
                    let tag = set.strip_prefix('#').unwrap_or_else(|| {
                        panic!(
                            "`double_trade_price_enchantments` is only modeled as a tag, got {set}"
                        )
                    });
                    let tag = vanilla_path(tag);
                    quote! { Some(EnchantmentOptions::Tag(Identifier::vanilla_static(#tag))) }
                },
            );

        stream.extend(quote! {
            pub static #ident: VillagerTrade = VillagerTrade {
                key: Identifier::vanilla_static(#key),
                wants: #wants,
                additional_wants: #additional_wants,
                gives: #gives,
                gives_count: #gives_count,
                max_uses: #max_uses,
                reputation_discount: #reputation_discount,
                xp: #xp,
                merchant_predicate: #merchant_predicate,
                given_item_modifiers: &[#(#modifiers),*],
                double_trade_price_enchantments: #double_trade_price_enchantments,
            };
        });
    }

    for (key, set) in &trade_sets {
        let ident = const_ident(key);
        let tag = set.trades.strip_prefix('#').unwrap_or_else(|| {
            panic!("trade set {key} names its trades as {}, but only a tag is modeled -- vanilla's `RegistryCodecs.homogeneousList` field is always a tag here", set.trades)
        });
        let tag = vanilla_path(tag);
        let members = resolved_tags
            .get(tag)
            .unwrap_or_else(|| panic!("trade set {key} references the missing tag #{tag}"));
        assert!(
            !members.is_empty(),
            "trade set {key} draws from the empty tag #{tag}"
        );
        let members: Vec<TokenStream> = members
            .iter()
            .map(|member| {
                let member_ident = known_trades
                    .get(member.as_str())
                    .unwrap_or_else(|| panic!("tag #{tag} names the unknown trade {member}"));
                quote! { &#member_ident }
            })
            .collect();

        let amount = generate_number_provider(&set.amount);
        let allow_duplicates = set.allow_duplicates;
        let random_sequence = set.random_sequence.as_ref().map_or_else(
            || quote! { None },
            |sequence| {
                let sequence = vanilla_path(sequence);
                quote! { Some(Identifier::vanilla_static(#sequence)) }
            },
        );

        stream.extend(quote! {
            pub static #ident: TradeSet = TradeSet {
                key: Identifier::vanilla_static(#key),
                trades: &[#(#members),*],
                amount: #amount,
                allow_duplicates: #allow_duplicates,
                random_sequence: #random_sequence,
            };
        });
    }

    let trade_registrations: Vec<TokenStream> = trades
        .iter()
        .map(|(key, _)| {
            let ident = &known_trades[key.as_str()];
            quote! { registry.register(&#ident); }
        })
        .collect();
    let set_registrations: Vec<TokenStream> = trade_sets
        .iter()
        .map(|(key, _)| {
            let ident = const_ident(key);
            quote! { registry.register(&#ident); }
        })
        .collect();

    stream.extend(quote! {
        pub fn register_villager_trades(registry: &mut VillagerTradeRegistry) {
            #(#trade_registrations)*
        }

        pub fn register_trade_sets(registry: &mut TradeSetRegistry) {
            #(#set_registrations)*
        }
    });

    stream
}
