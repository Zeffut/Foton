use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Span};

use crate::generator_functions::generate_text_component;
use crate::shared_structs::TextComponentJson;

/// The generated constant name for a vanilla registry id.
fn const_ident(id: &str) -> Ident {
    let id = id.strip_prefix("minecraft:").unwrap_or(id);
    Ident::new(&id.to_shouty_snake_case(), Span::call_site())
}

use super::{
    ItemFilterJson, LimitJson, LootFunctionJson, TokenStream, generate_condition,
    generate_instrument_options, generate_number_provider, generate_optional_enchantment_options,
    generate_potion_options, quote,
};

/// Generates the `ItemPredicate` a `minecraft:filtered` function tests.
///
/// Every `predicates` entry a vanilla file uses is a presence check -- either
/// `DataComponentPredicate.AnyValueType` (`{}`) or an enchantment list built
/// from `[{}]`. Anything richer fails the build rather than passing unchecked,
/// because a filter that always matches turns an `on_fail: discard` into a
/// villager selling the unenchanted item vanilla would have withheld.
fn generate_item_filter(filter: &ItemFilterJson) -> TokenStream {
    assert!(
        filter.components.is_none(),
        "`item_filter.components` is not modeled; the only vanilla uses are `predicates`"
    );

    let items = match &filter.items {
        None => quote! { None },
        Some(items) => {
            if let Some(tag) = items.strip_prefix('#') {
                let tag = tag.strip_prefix("minecraft:").unwrap_or(tag);
                quote! { Some(ItemFilterItems::Tag(Identifier::vanilla_static(#tag))) }
            } else {
                let id = items.strip_prefix("minecraft:").unwrap_or(items);
                quote! { Some(ItemFilterItems::List(&[Identifier::vanilla_static(#id)])) }
            }
        }
    };

    let mut predicates: Vec<(&str, TokenStream)> = Vec::new();
    for (key, value) in filter.predicates.iter().flatten() {
        let generated = match key.as_str() {
            "minecraft:enchantments" | "minecraft:stored_enchantments" => {
                let entries = value
                    .as_array()
                    .unwrap_or_else(|| panic!("`{key}` item filter predicate must be a list"));
                assert!(
                    entries.len() == 1
                        && entries[0]
                            .as_object()
                            .is_some_and(serde_json::Map::is_empty),
                    "only the `[{{}}]` form of `{key}` is modeled, got {value}"
                );
                if key == "minecraft:enchantments" {
                    quote! { ItemComponentPredicate::AnyEnchantment }
                } else {
                    quote! { ItemComponentPredicate::AnyStoredEnchantment }
                }
            }
            other => {
                assert!(
                    value.as_object().is_some_and(serde_json::Map::is_empty),
                    "item filter predicate `{other}` is only modeled in its empty \
                     `DataComponentPredicate.AnyValueType` form, got {value}"
                );
                let component = other.strip_prefix("minecraft:").unwrap_or(other);
                quote! { ItemComponentPredicate::Present(Identifier::vanilla_static(#component)) }
            }
        };
        predicates.push((key.as_str(), generated));
    }
    // A JSON map has no order; sort so the generated file is reproducible.
    predicates.sort_by_key(|(key, _)| *key);
    let predicates: Vec<TokenStream> = predicates.into_iter().map(|(_, tokens)| tokens).collect();

    quote! {
        ItemFilter {
            items: #items,
            predicates: &[#(#predicates),*],
        }
    }
}

/// Generates one `on_pass` / `on_fail` branch of a `minecraft:filtered` function.
fn generate_optional_branch(branch: Option<&LootFunctionJson>) -> TokenStream {
    match branch {
        None => quote! { None },
        Some(branch) => {
            let branch = generate_function(branch);
            quote! { Some(&#branch) }
        }
    }
}

pub(crate) fn generate_function(function: &LootFunctionJson) -> TokenStream {
    let func_body = match function.function.as_str() {
        "minecraft:set_count" => {
            let count = function.count.as_ref().map_or_else(
                || quote! { NumberProvider::Constant(1.0) },
                generate_number_provider,
            );
            let add = function.add;
            quote! { LootFunction::SetCount { count: #count, add: #add } }
        }
        "minecraft:explosion_decay" => {
            quote! { LootFunction::ExplosionDecay }
        }
        "minecraft:apply_bonus" => {
            let enchantment = function
                .enchantment
                .as_deref()
                .unwrap_or("minecraft:fortune");
            let enchantment = enchantment
                .strip_prefix("minecraft:")
                .unwrap_or(enchantment);

            let formula = match function.formula.as_deref() {
                Some("minecraft:ore_drops") => {
                    quote! { BonusFormula::OreDrops }
                }
                Some("minecraft:uniform_bonus_count") => {
                    let multiplier = function
                        .parameters
                        .as_ref()
                        .and_then(|p| p.bonus_multiplier)
                        .unwrap_or(1);
                    quote! { BonusFormula::UniformBonusCount { bonus_multiplier: #multiplier } }
                }
                Some("minecraft:binomial_with_bonus_count") => {
                    let extra = function
                        .parameters
                        .as_ref()
                        .and_then(|p| p.extra)
                        .unwrap_or(0);
                    let probability = function
                        .parameters
                        .as_ref()
                        .and_then(|p| p.probability)
                        .unwrap_or(0.5);
                    quote! { BonusFormula::BinomialWithBonusCount { extra: #extra, probability: #probability } }
                }
                _ => {
                    quote! { BonusFormula::OreDrops }
                }
            };

            quote! {
                LootFunction::ApplyBonus {
                    enchantment: Identifier::vanilla_static(#enchantment),
                    formula: #formula,
                }
            }
        }
        "minecraft:enchanted_count_increase" => {
            let enchantment = function
                .enchantment
                .as_deref()
                .unwrap_or("minecraft:looting");
            let enchantment = enchantment
                .strip_prefix("minecraft:")
                .unwrap_or(enchantment);

            let count = function.count.as_ref().map_or_else(
                || quote! { NumberProvider::Uniform { min: 0.0, max: 1.0 } },
                generate_number_provider,
            );

            let limit = match &function.limit {
                Some(LimitJson::Integer(v)) => *v,
                Some(LimitJson::Object { max, .. }) => max.map_or(0, |v| v as i32),
                None => 0,
            };

            quote! {
                LootFunction::EnchantedCountIncrease {
                    enchantment: Identifier::vanilla_static(#enchantment),
                    count: #count,
                    limit: #limit,
                }
            }
        }
        "minecraft:limit_count" => {
            let (min, max) = match &function.limit {
                Some(LimitJson::Integer(v)) => (Some(*v), Some(*v)),
                Some(LimitJson::Object { min, max }) => {
                    (min.map(|v| v as i32), max.map(|v| v as i32))
                }
                None => (None, None),
            };

            let min_tokens = if let Some(v) = min {
                quote! { Some(#v) }
            } else {
                quote! { None }
            };
            let max_tokens = if let Some(v) = max {
                quote! { Some(#v) }
            } else {
                quote! { None }
            };

            quote! { LootFunction::LimitCount { min: #min_tokens, max: #max_tokens } }
        }
        "minecraft:set_damage" => {
            let damage = function.damage.as_ref().map_or_else(
                || quote! { NumberProvider::Constant(1.0) },
                generate_number_provider,
            );
            let add = function.add;
            quote! { LootFunction::SetDamage { damage: #damage, add: #add } }
        }
        "minecraft:enchant_randomly" => {
            let options = generate_optional_enchantment_options(&function.options);
            // Vanilla's `only_compatible` defaults to true.
            let only_compatible = function.only_compatible.unwrap_or(true);
            let include_additional_cost_component = function.include_additional_cost_component;
            quote! {
                LootFunction::EnchantRandomly {
                    options: #options,
                    only_compatible: #only_compatible,
                    include_additional_cost_component: #include_additional_cost_component,
                }
            }
        }
        "minecraft:enchant_with_levels" => {
            let levels = function.levels.as_ref().map_or_else(
                || panic!("`minecraft:enchant_with_levels` is missing its `levels`"),
                generate_number_provider,
            );
            let options = generate_optional_enchantment_options(&function.options);
            let include_additional_cost_component = function.include_additional_cost_component;
            quote! {
                LootFunction::EnchantWithLevels {
                    levels: #levels,
                    options: #options,
                    include_additional_cost_component: #include_additional_cost_component,
                }
            }
        }
        "minecraft:set_random_potion" => {
            let options = generate_potion_options(&function.options);
            quote! { LootFunction::SetRandomPotion { options: #options } }
        }
        "minecraft:set_random_dyes" => {
            let number_of_dyes = function.number_of_dyes.as_ref().map_or_else(
                || panic!("`minecraft:set_random_dyes` is missing its `number_of_dyes`"),
                generate_number_provider,
            );
            quote! { LootFunction::SetRandomDyes { number_of_dyes: #number_of_dyes } }
        }
        "minecraft:discard" => {
            quote! { LootFunction::Discard }
        }
        "minecraft:filtered" => {
            let item_filter =
                generate_item_filter(function.item_filter.as_ref().unwrap_or_else(|| {
                    panic!("`minecraft:filtered` is missing its `item_filter`")
                }));
            let on_pass = generate_optional_branch(function.on_pass.as_deref());
            let on_fail = generate_optional_branch(function.on_fail.as_deref());
            quote! {
                LootFunction::Filtered {
                    item_filter: #item_filter,
                    on_pass: #on_pass,
                    on_fail: #on_fail,
                }
            }
        }
        "minecraft:copy_components" => {
            let source = match function.source.as_deref() {
                Some("block_entity") => quote! { CopySource::BlockEntity },
                Some("this") => quote! { CopySource::This },
                Some("attacker") => quote! { CopySource::Attacker },
                Some("direct_attacker") => quote! { CopySource::DirectAttacker },
                _ => quote! { CopySource::BlockEntity },
            };

            let include: Vec<TokenStream> = function
                .include
                .as_ref()
                .map(|inc| {
                    inc.iter()
                        .map(|s| {
                            let s = s.strip_prefix("minecraft:").unwrap_or(s);
                            quote! { Identifier::vanilla_static(#s) }
                        })
                        .collect()
                })
                .unwrap_or_default();

            quote! {
                LootFunction::CopyComponents {
                    source: #source,
                    include: &[#(#include),*],
                }
            }
        }
        "minecraft:copy_state" => {
            let block = function.block.as_deref().unwrap_or("minecraft:air");
            let block = block.strip_prefix("minecraft:").unwrap_or(block);

            let properties: Vec<TokenStream> = function
                .properties
                .as_ref()
                .map(|props| props.iter().map(|p| quote! { #p }).collect())
                .unwrap_or_default();

            quote! {
                LootFunction::CopyState {
                    block: Identifier::vanilla_static(#block),
                    properties: &[#(#properties),*],
                }
            }
        }
        "minecraft:set_components" => {
            let components = function
                .components
                .as_ref()
                .expect("`set_components` without `components`");
            let material = const_ident(&components.trim.material);
            let pattern = const_ident(&components.trim.pattern);
            quote! {
                LootFunction::SetComponents {
                    apply: |item| {
                        item.set(
                            TRIM,
                            ArmorTrim::new(
                                RegistryHolder::reference(&vanilla_trim_materials::#material),
                                RegistryHolder::reference(&vanilla_trim_patterns::#pattern),
                            ),
                        );
                    },
                }
            }
        }
        "minecraft:furnace_smelt" => {
            let use_input_count = function.use_input_count.unwrap_or(true);
            quote! { LootFunction::FurnaceSmelt { use_input_count: #use_input_count } }
        }
        "minecraft:exploration_map" => {
            let destination = function
                .destination
                .as_deref()
                .unwrap_or("minecraft:buried_treasure");
            let destination = destination
                .strip_prefix("minecraft:")
                .unwrap_or(destination);

            let decoration = function.decoration.as_deref().unwrap_or("minecraft:red_x");
            let decoration = decoration.strip_prefix("minecraft:").unwrap_or(decoration);

            let zoom = function.zoom.unwrap_or(2);
            let skip_existing_chunks = function.skip_existing_chunks.unwrap_or(true);

            quote! {
                LootFunction::ExplorationMap {
                    destination: Identifier::vanilla_static(#destination),
                    decoration: Identifier::vanilla_static(#decoration),
                    zoom: #zoom,
                    skip_existing_chunks: #skip_existing_chunks,
                }
            }
        }
        "minecraft:set_name" => {
            // Vanilla's `target` is optional and defaults to `custom_name`.
            let target = match function.target.as_deref() {
                Some("item_name") => quote! { NameTarget::ItemName },
                Some("custom_name") | None => quote! { NameTarget::CustomName },
                Some(other) => panic!("unknown `set_name` target `{other}`"),
            };

            let Some(name) = &function.name else {
                // Vanilla parity: `SetNameFunction.run` writes nothing when the
                // optional `name` is absent, so neither does an empty sequence.
                return quote! { LootFunction::Sequence { functions: &[] } };
            };
            let name: TextComponentJson = match serde_json::from_value(name.clone()) {
                Ok(name) => name,
                Err(error) => panic!("`set_name` name {name} is not modeled: {error}"),
            };
            let name = generate_text_component(&name);

            quote! {
                LootFunction::SetName {
                    name: || #name,
                    target: #target,
                }
            }
        }
        "minecraft:set_ominous_bottle_amplifier" => {
            let amplifier = function.amplifier.as_ref().map_or_else(
                || quote! { NumberProvider::Constant(0.0) },
                generate_number_provider,
            );
            quote! { LootFunction::SetOminousBottleAmplifier { amplifier: #amplifier } }
        }
        "minecraft:set_potion" => {
            let id = function.id.as_deref().unwrap_or("minecraft:water");
            let id = id.strip_prefix("minecraft:").unwrap_or(id);
            quote! { LootFunction::SetPotion { id: Identifier::vanilla_static(#id) } }
        }
        "minecraft:set_stew_effect" => {
            let effects: Vec<TokenStream> = function
                .effects
                .as_ref()
                .map(|effs| {
                    effs.iter()
                        .map(|e| {
                            let effect_type = e
                                .effect_type
                                .strip_prefix("minecraft:")
                                .unwrap_or(&e.effect_type);
                            let duration = generate_number_provider(&e.duration);

                            quote! {
                                StewEffect {
                                    effect_type: Identifier::vanilla_static(#effect_type),
                                    duration: #duration,
                                }
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            quote! { LootFunction::SetStewEffect { effects: &[#(#effects),*] } }
        }
        "minecraft:set_instrument" => {
            let options = generate_instrument_options(&function.options);
            quote! { LootFunction::SetInstrument { options: #options } }
        }
        "minecraft:set_enchantments" => {
            let enchantments: Vec<TokenStream> = function
                .enchantments
                .as_ref()
                .map(|enc| {
                    enc.iter()
                        .map(|(name, level)| {
                            let name = name.strip_prefix("minecraft:").unwrap_or(name);
                            let level = generate_number_provider(level);
                            quote! { (Identifier::vanilla_static(#name), #level) }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let add = function.add;
            quote! {
                LootFunction::SetEnchantments {
                    enchantments: &[#(#enchantments),*],
                    add: #add,
                }
            }
        }
        other => {
            panic!("Unknown loot function type: {other}");
        }
    };

    // Wrap the function with conditions
    let conditions: Vec<TokenStream> = function
        .conditions
        .as_ref()
        .map(|conds| conds.iter().map(generate_condition).collect())
        .unwrap_or_default();

    quote! {
        ConditionalLootFunction {
            function: #func_body,
            conditions: &[#(#conditions),*],
        }
    }
}
