use std::cell::RefCell;
use std::collections::BTreeSet;

use super::{
    BlockPredicateJson, DamageSourcePredicateJson, EnchantmentOptionsJson, EntityEquipmentJson,
    EntityFlagsJson, EntityPredicateJson, EquipmentSlotJson, FromStr, Ident, Identifier,
    IntBoundJson, LocationPredicateJson, NumberProviderJson, PredicateJson, Span,
    ToShoutySnakeCase, TokenStream, quote,
};

pub(crate) fn generate_number_provider(value: &NumberProviderJson) -> TokenStream {
    match value {
        NumberProviderJson::Constant(v) => {
            quote! { NumberProvider::Constant(#v) }
        }
        NumberProviderJson::Object {
            provider_type,
            value,
            min,
            max,
            n,
            p,
            summands,
        } => match provider_type.as_str() {
            "minecraft:constant" => {
                let v = value.unwrap_or(1.0);
                quote! { NumberProvider::Constant(#v) }
            }
            "minecraft:uniform" => {
                let min = min.unwrap_or(0.0);
                let max = max.unwrap_or(1.0);
                quote! { NumberProvider::Uniform { min: #min, max: #max } }
            }
            "minecraft:binomial" => {
                let n = n.unwrap_or(1.0) as i32;
                let p = p.unwrap_or(0.5);
                quote! { NumberProvider::Binomial { n: #n, p: #p } }
            }
            "minecraft:sum" => {
                let summands = summands.as_ref().unwrap_or_else(|| {
                    panic!("`minecraft:sum` number provider is missing its `summands`")
                });
                let summands: Vec<TokenStream> =
                    summands.iter().map(generate_number_provider).collect();
                quote! { NumberProvider::Sum(&[#(#summands),*]) }
            }
            // A provider Steel cannot model must not silently become a constant:
            // that hands out a count nobody computed. Fail the build instead,
            // the way an unknown loot function already does.
            other => panic!("Unknown number provider type: {other}"),
        },
    }
}

/// Generate the `LootContextEntity` enum variant at build time.
pub(super) fn generate_loot_context_entity(entity: &str) -> TokenStream {
    match entity {
        "this" => quote! { LootContextEntity::This },
        "killer" | "attacker" => quote! { LootContextEntity::Killer },
        "direct_killer" | "direct_attacker" => quote! { LootContextEntity::DirectKiller },
        "killer_player" | "last_damage_player" => quote! { LootContextEntity::KillerPlayer },
        "interacting_entity" => quote! { LootContextEntity::Interacting },
        _ => quote! { LootContextEntity::This },
    }
}

/// Generate the `EquipmentSlotGroup` enum variant at build time.
#[expect(
    dead_code,
    reason = "loot table generator keeps slot helper for extracted predicate coverage"
)]
pub(super) fn generate_equipment_slot_group(slot: &str) -> TokenStream {
    match slot {
        "any" => quote! { EquipmentSlotGroup::Any },
        "mainhand" | "main_hand" => quote! { EquipmentSlotGroup::MainHand },
        "offhand" | "off_hand" => quote! { EquipmentSlotGroup::OffHand },
        "hand" => quote! { EquipmentSlotGroup::Hand },
        "head" => quote! { EquipmentSlotGroup::Head },
        "chest" => quote! { EquipmentSlotGroup::Chest },
        "legs" => quote! { EquipmentSlotGroup::Legs },
        "feet" => quote! { EquipmentSlotGroup::Feet },
        "armor" => quote! { EquipmentSlotGroup::Armor },
        "body" => quote! { EquipmentSlotGroup::Body },
        _ => quote! { EquipmentSlotGroup::Any },
    }
}

/// Generate the `DyeColor` enum variant at build time.
pub(super) fn generate_dye_color(color: &str) -> TokenStream {
    match color {
        "white" => quote! { DyeColor::White },
        "orange" => quote! { DyeColor::Orange },
        "magenta" => quote! { DyeColor::Magenta },
        "light_blue" => quote! { DyeColor::LightBlue },
        "yellow" => quote! { DyeColor::Yellow },
        "lime" => quote! { DyeColor::Lime },
        "pink" => quote! { DyeColor::Pink },
        "gray" => quote! { DyeColor::Gray },
        "light_gray" => quote! { DyeColor::LightGray },
        "cyan" => quote! { DyeColor::Cyan },
        "purple" => quote! { DyeColor::Purple },
        "blue" => quote! { DyeColor::Blue },
        "brown" => quote! { DyeColor::Brown },
        "green" => quote! { DyeColor::Green },
        "red" => quote! { DyeColor::Red },
        "black" => quote! { DyeColor::Black },
        _ => quote! { DyeColor::White },
    }
}

/// Generate the `LootType` enum variant at build time.
pub(super) fn generate_loot_type(loot_type: &str) -> TokenStream {
    match loot_type {
        "minecraft:block" => quote! { LootType::Block },
        "minecraft:entity" => quote! { LootType::Entity },
        "minecraft:chest" => quote! { LootType::Chest },
        "minecraft:fishing" => quote! { LootType::Fishing },
        "minecraft:gift" => quote! { LootType::Gift },
        "minecraft:archaeology" => quote! { LootType::Archaeology },
        "minecraft:vault" => quote! { LootType::Vault },
        "minecraft:shearing" => quote! { LootType::Shearing },
        "minecraft:equipment" => quote! { LootType::Equipment },
        "minecraft:selector" => quote! { LootType::Selector },
        "minecraft:entity_interact" => quote! { LootType::EntityInteract },
        "minecraft:block_interact" => quote! { LootType::BlockInteract },
        "minecraft:barter" => quote! { LootType::Barter },
        _ => quote! { LootType::Block }, // Default to Block
    }
}

pub(super) fn generate_tool_predicate(predicate: &Option<PredicateJson>) -> TokenStream {
    let Some(pred) = predicate else {
        return quote! { ToolPredicate::Any };
    };

    // Only handle tool predicates; location/entity/damage_source predicates return Any
    let pred = match pred {
        PredicateJson::Tool(p) => p,
        PredicateJson::Location(_) => return quote! { ToolPredicate::Any },
        PredicateJson::DamageSource(_) => return quote! { ToolPredicate::Any },
        PredicateJson::Entity(_) => return quote! { ToolPredicate::Any },
    };

    // Check for items field (can be a string or tag reference)
    if let Some(item_str) = &pred.items {
        if item_str.starts_with('#') {
            // Tag reference like "#minecraft:pickaxes"
            let tag = item_str
                .strip_prefix("#minecraft:")
                .unwrap_or(item_str.strip_prefix('#').unwrap_or(item_str));
            return quote! { ToolPredicate::Tag(Identifier::vanilla_static(#tag)) };
        } else {
            let item = item_str.strip_prefix("minecraft:").unwrap_or(item_str);
            return quote! { ToolPredicate::Item(Identifier::vanilla_static(#item)) };
        }
    }

    // Check for enchantment predicates
    if let Some(predicates) = &pred.predicates
        && let Some(enchants) = &predicates.enchantments
        && let Some(first) = enchants.first()
        && let Some(enchant_name) = &first.enchantments
    {
        let enchant_name = enchant_name.strip_prefix("#minecraft:").unwrap_or(
            enchant_name
                .strip_prefix("minecraft:")
                .unwrap_or(enchant_name),
        );
        let min_level = first.levels.as_ref().and_then(|l| l.min).unwrap_or(1);

        return quote! {
            ToolPredicate::HasEnchantment {
                enchantment: Identifier::vanilla_static(#enchant_name),
                min_level: #min_level,
            }
        };
    }

    quote! { ToolPredicate::Any }
}

/// Generates the body of a `RegistryCodecs.homogeneousList` field.
///
/// The three vanilla shapes are `"#namespace:tag"`, a bare `"namespace:id"`
/// naming exactly one entry, and a list of ids. Only the `#` marks a tag --
/// treating a bare id as one was the old behavior and silently produced an
/// empty set, so `piglin_bartering`'s soul speed boots came out unenchanted.
fn generate_homogeneous_list(
    options: &EnchantmentOptionsJson,
    tag_variant: &TokenStream,
    list_variant: &TokenStream,
) -> TokenStream {
    match options {
        EnchantmentOptionsJson::Tag(s) => {
            if let Some(tag) = s.strip_prefix('#') {
                let tag = tag.strip_prefix("minecraft:").unwrap_or(tag);
                quote! { #tag_variant(Identifier::vanilla_static(#tag)) }
            } else {
                let id = s.strip_prefix("minecraft:").unwrap_or(s);
                quote! { #list_variant(&[Identifier::vanilla_static(#id)]) }
            }
        }
        EnchantmentOptionsJson::List(arr) => {
            let ids: Vec<TokenStream> = arr
                .iter()
                .map(|s| {
                    assert!(
                        !s.starts_with('#'),
                        "a homogeneous list may not contain the tag {s}"
                    );
                    let s = s.strip_prefix("minecraft:").unwrap_or(s);
                    quote! { Identifier::vanilla_static(#s) }
                })
                .collect();
            quote! { #list_variant(&[#(#ids),*]) }
        }
    }
}

/// The `Optional<HolderSet<Enchantment>>` shape: absent means the whole registry.
pub(super) fn generate_optional_enchantment_options(
    options: &Option<EnchantmentOptionsJson>,
) -> TokenStream {
    let tag = quote! { EnchantmentOptions::Tag };
    let list = quote! { EnchantmentOptions::List };
    if let Some(options) = options {
        let options = generate_homogeneous_list(options, &tag, &list);
        quote! { Some(#options) }
    } else {
        quote! { None }
    }
}

/// The `Optional<HolderSet<Potion>>` of `minecraft:set_random_potion`.
pub(super) fn generate_potion_options(options: &Option<EnchantmentOptionsJson>) -> TokenStream {
    let tag = quote! { PotionOptions::Tag };
    let list = quote! { PotionOptions::List };
    if let Some(options) = options {
        let options = generate_homogeneous_list(options, &tag, &list);
        quote! { Some(#options) }
    } else {
        quote! { None }
    }
}

pub(super) fn generate_instrument_ref(value: &str) -> TokenStream {
    let id = Identifier::from_str(value)
        .unwrap_or_else(|error| panic!("invalid instrument id {value:?}: {error}"));
    assert_eq!(
        id.namespace.as_ref(),
        "minecraft",
        "vanilla loot table references a non-vanilla instrument: {id}"
    );
    let ident = Ident::new(&id.path.to_shouty_snake_case(), Span::call_site());
    quote! { &crate::vanilla_instruments::#ident }
}

pub(super) fn generate_instrument_options(options: &Option<EnchantmentOptionsJson>) -> TokenStream {
    match options {
        Some(EnchantmentOptionsJson::Tag(value)) if value.starts_with('#') => {
            let tag = value.trim_start_matches('#');
            let id = Identifier::from_str(tag)
                .unwrap_or_else(|error| panic!("invalid instrument tag {value:?}: {error}"));
            let namespace = id.namespace.as_ref();
            let path = id.path.as_ref();
            quote! {
                InstrumentOptions::Tag(Identifier::new_static(#namespace, #path))
            }
        }
        Some(EnchantmentOptionsJson::Tag(value)) => {
            let instrument = generate_instrument_ref(value);
            quote! { InstrumentOptions::Direct(&[#instrument]) }
        }
        Some(EnchantmentOptionsJson::List(values)) => {
            let instruments = values.iter().map(|value| generate_instrument_ref(value));
            quote! { InstrumentOptions::Direct(&[#(#instruments),*]) }
        }
        None => panic!("set_instrument function is missing its options holder set"),
    }
}

pub(super) fn generate_entity_flags(flags: &Option<EntityFlagsJson>) -> TokenStream {
    if let Some(f) = flags {
        let is_on_fire = if let Some(v) = f.is_on_fire {
            quote! { Some(#v) }
        } else {
            quote! { None }
        };
        let is_sneaking = if let Some(v) = f.is_sneaking {
            quote! { Some(#v) }
        } else {
            quote! { None }
        };
        let is_sprinting = if let Some(v) = f.is_sprinting {
            quote! { Some(#v) }
        } else {
            quote! { None }
        };
        let is_swimming = if let Some(v) = f.is_swimming {
            quote! { Some(#v) }
        } else {
            quote! { None }
        };
        let is_baby = if let Some(v) = f.is_baby {
            quote! { Some(#v) }
        } else {
            quote! { None }
        };
        quote! {
            Some(EntityFlags {
                is_on_fire: #is_on_fire,
                is_sneaking: #is_sneaking,
                is_sprinting: #is_sprinting,
                is_swimming: #is_swimming,
                is_baby: #is_baby,
            })
        }
    } else {
        quote! { None }
    }
}

pub(super) fn generate_equipment_slot_predicate(slot: &Option<EquipmentSlotJson>) -> TokenStream {
    if let Some(s) = slot {
        if let Some(items) = &s.items {
            if items.starts_with('#') {
                let tag = items
                    .strip_prefix("#minecraft:")
                    .unwrap_or(items.strip_prefix('#').unwrap_or(items));
                return quote! { Some(ToolPredicate::Tag(Identifier::vanilla_static(#tag))) };
            } else {
                let item = items.strip_prefix("minecraft:").unwrap_or(items);
                return quote! { Some(ToolPredicate::Item(Identifier::vanilla_static(#item))) };
            }
        }

        if let Some(predicates) = &s.predicates
            && let Some(enchants) = &predicates.enchantments
            && let Some(first) = enchants.first()
            && let Some(enchant_name) = &first.enchantments
        {
            let enchant_name = enchant_name.strip_prefix("#minecraft:").unwrap_or(
                enchant_name
                    .strip_prefix("minecraft:")
                    .unwrap_or(enchant_name),
            );
            let min_level = first.levels.as_ref().and_then(|l| l.min).unwrap_or(1);
            return quote! {
                Some(ToolPredicate::HasEnchantment {
                    enchantment: Identifier::vanilla_static(#enchant_name),
                    min_level: #min_level,
                })
            };
        }

        quote! { Some(ToolPredicate::Any) }
    } else {
        quote! { None }
    }
}

pub(super) fn generate_entity_equipment(equipment: &Option<EntityEquipmentJson>) -> TokenStream {
    if let Some(e) = equipment {
        let mainhand = generate_equipment_slot_predicate(&e.mainhand);
        let offhand = generate_equipment_slot_predicate(&e.offhand);
        let head = generate_equipment_slot_predicate(&e.head);
        let chest = generate_equipment_slot_predicate(&e.chest);
        let legs = generate_equipment_slot_predicate(&e.legs);
        let feet = generate_equipment_slot_predicate(&e.feet);

        quote! {
            Some(EntityEquipment {
                mainhand: #mainhand,
                offhand: #offhand,
                head: #head,
                chest: #chest,
                legs: #legs,
                feet: #feet,
            })
        }
    } else {
        quote! { None }
    }
}

pub(super) fn generate_entity_predicate(predicate: &EntityPredicateJson) -> TokenStream {
    let entity_type = if let Some(t) = &predicate.entity_type {
        let t = t.strip_prefix("minecraft:").unwrap_or(t);
        quote! { Some(Identifier::vanilla_static(#t)) }
    } else {
        quote! { None }
    };

    let flags = generate_entity_flags(&predicate.flags);
    let equipment = generate_entity_equipment(&predicate.equipment);

    let sheep_color = predicate
        .components
        .as_ref()
        .and_then(|components| components.sheep_color.as_deref())
        .map_or_else(
            || quote! { None },
            |color| {
                let color = generate_dye_color(color);
                quote! { Some(#color) }
            },
        );
    let sheep_sheared = predicate
        .sheep_type_specific
        .as_ref()
        .and_then(|sheep| sheep.sheared)
        .map_or_else(|| quote! { None }, |sheared| quote! { Some(#sheared) });

    let chicken_variant = predicate
        .components
        .as_ref()
        .and_then(|components| components.chicken_variant.as_deref())
        .map_or_else(
            || quote! { None },
            |variant| {
                let variant = variant.strip_prefix("minecraft:").unwrap_or(variant);
                quote! { Some(Identifier::vanilla_static(#variant)) }
            },
        );

    let mooshroom_variant = predicate
        .components
        .as_ref()
        .and_then(|components| components.mooshroom_variant.as_deref())
        .map_or_else(
            || quote! { None },
            |variant| {
                let variant = variant.strip_prefix("minecraft:").unwrap_or(variant);
                quote! { Some(#variant) }
            },
        );

    let cube_size = predicate
        .cube_mob_type_specific
        .as_ref()
        .and_then(|cube| cube.size.as_ref())
        .map_or_else(
            || quote! { None },
            |size| {
                let range = generate_int_bound(size);
                quote! { Some(#range) }
            },
        );

    let in_open_water = predicate
        .fishing_hook_type_specific
        .as_ref()
        .and_then(|hook| hook.in_open_water)
        .map_or_else(|| quote! { None }, |open| quote! { Some(#open) });

    let villager_variant: Vec<TokenStream> = predicate
        .component_predicates
        .as_ref()
        .and_then(|predicates| predicates.villager_variant.as_ref())
        .map(|variants| match variants {
            // Vanilla's `HolderSet` shape: one id, or a list of them. No trade
            // uses a `#tag` here, and a tag would need a runtime lookup the
            // predicate has no registry for, so it fails the build.
            EnchantmentOptionsJson::Tag(id) => {
                assert!(
                    !id.starts_with('#'),
                    "a villager/variant predicate given as the tag {id} is not modeled"
                );
                vec![id.clone()]
            }
            EnchantmentOptionsJson::List(ids) => ids.clone(),
        })
        .unwrap_or_default()
        .iter()
        .map(|id| {
            let id = id.strip_prefix("minecraft:").unwrap_or(id);
            quote! { Identifier::vanilla_static(#id) }
        })
        .collect();

    // A predicate key the generator cannot lower must be visible: it is warned
    // about at build time and fails at evaluation time, never silently passes.
    let mut unsupported: Vec<String> = predicate.unmodeled.keys().cloned().collect();
    if let Some(components) = &predicate.components {
        unsupported.extend(
            components
                .unmodeled
                .keys()
                .map(|key| format!("minecraft:components -> {key}")),
        );
    }
    if let Some(predicates) = &predicate.component_predicates {
        unsupported.extend(
            predicates
                .unmodeled
                .keys()
                .map(|key| format!("minecraft:predicates -> {key}")),
        );
    }
    unsupported.sort();
    warn_unsupported_predicate_keys(&unsupported);

    quote! {
        EntityPredicate {
            entity_type: #entity_type,
            flags: #flags,
            equipment: #equipment,
            sheep_color: #sheep_color,
            sheep_sheared: #sheep_sheared,
            chicken_variant: #chicken_variant,
            mooshroom_variant: #mooshroom_variant,
            cube_size: #cube_size,
            in_open_water: #in_open_water,
            villager_variant: &[#(#villager_variant),*],
            unsupported: &[#(#unsupported),*],
        }
    }
}

/// Warns once per distinct predicate key the generator cannot lower.
///
/// The same key shows up in many tables, so the raw list would bury the signal.
fn warn_unsupported_predicate_keys(keys: &[String]) {
    thread_local! {
        static SEEN: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
    }

    SEEN.with_borrow_mut(|seen| {
        for key in keys {
            if seen.insert(key.clone()) {
                println!(
                    "cargo:warning=Loot entity predicate key `{key}` is not modeled; \
                     every predicate using it will fail rather than match."
                );
            }
        }
    });
}

/// Lowers a vanilla `IntRange` into a [`NumberProviderRange`].
fn generate_int_bound(bound: &IntBoundJson) -> TokenStream {
    match bound {
        IntBoundJson::Exact(value) => {
            let value = *value as f32;
            quote! { NumberProviderRange::exact(#value) }
        }
        IntBoundJson::Range { min, max } => match (min, max) {
            (Some(min), Some(max)) => {
                let (min, max) = (*min as f32, *max as f32);
                quote! { NumberProviderRange::between(#min, #max) }
            }
            (Some(min), None) => {
                let min = *min as f32;
                quote! { NumberProviderRange::at_least(#min) }
            }
            (None, Some(max)) => {
                let max = *max as f32;
                quote! { NumberProviderRange::at_most(#max) }
            }
            (None, None) => quote! { NumberProviderRange { min: None, max: None } },
        },
    }
}

pub(super) fn generate_damage_source_predicate(
    predicate: &DamageSourcePredicateJson,
) -> TokenStream {
    let tags: Vec<TokenStream> = predicate
        .tags
        .as_ref()
        .map(|t| {
            t.iter()
                .map(|tag| {
                    let id = tag.id.strip_prefix("minecraft:").unwrap_or(&tag.id);
                    let expected = tag.expected;
                    quote! {
                        DamageTagPredicate {
                            id: Identifier::vanilla_static(#id),
                            expected: #expected,
                        }
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let source_entity = if let Some(e) = &predicate.source_entity {
        let pred = generate_entity_predicate(e);
        quote! { Some(#pred) }
    } else {
        quote! { None }
    };

    let direct_entity = if let Some(e) = &predicate.direct_entity {
        let pred = generate_entity_predicate(e);
        quote! { Some(#pred) }
    } else {
        quote! { None }
    };

    let is_direct = if let Some(v) = predicate.is_direct {
        quote! { Some(#v) }
    } else {
        quote! { None }
    };

    quote! {
        DamageSourcePredicate {
            tags: &[#(#tags),*],
            source_entity: #source_entity,
            direct_entity: #direct_entity,
            is_direct: #is_direct,
        }
    }
}

pub(super) fn generate_block_predicate(predicate: &BlockPredicateJson) -> TokenStream {
    let blocks = if let Some(b) = &predicate.blocks {
        let b = b.strip_prefix("minecraft:").unwrap_or(b);
        quote! { Some(Identifier::vanilla_static(#b)) }
    } else {
        quote! { None }
    };

    let state: Vec<TokenStream> = predicate
        .state
        .as_ref()
        .map(|props| {
            props
                .iter()
                .map(|(name, value)| {
                    quote! { (#name, #value) }
                })
                .collect()
        })
        .unwrap_or_default();

    quote! {
        BlockPredicate {
            blocks: #blocks,
            state: &[#(#state),*],
        }
    }
}

pub(super) fn generate_location_predicate(predicate: &LocationPredicateJson) -> TokenStream {
    let block = if let Some(b) = &predicate.block {
        let block_pred = generate_block_predicate(b);
        quote! { Some(#block_pred) }
    } else {
        quote! { None }
    };

    quote! {
        LocationPredicate {
            block: #block,
        }
    }
}
