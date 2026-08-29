//! Lowers datapack predicate JSON into the typed predicates of
//! `crate::advancement::predicate`.
//!
//! Every function here consumes its keys through [`ObjectReader`], so a key
//! vanilla adds later stops the build rather than quietly disappearing from
//! the predicate.

use proc_macro2::TokenStream;
use quote::quote;
use serde_json::Value;

use super::json::{BoundsJson, DistanceJson, ObjectReader, PositionJson, RegistrySetJson};

/// Emits an `Identifier` for a datapack id.
pub fn identifier(path: &str, value: &str) -> TokenStream {
    let value = value.strip_prefix('#').unwrap_or(value);
    match value.split_once(':') {
        None => quote! { Identifier::vanilla_static(#value) },
        Some(("minecraft", rest)) => quote! { Identifier::vanilla_static(#rest) },
        Some((namespace, rest)) => {
            panic!("{path}: Foton only generates vanilla ids, found {namespace}:{rest}")
        }
    }
}

fn string_of(path: &str, value: &Value) -> String {
    let Value::String(text) = value else {
        panic!("{path}: expected a string, found {value}");
    };
    text.clone()
}

/// Emits an `Option<T>` from an optional token stream.
fn option(value: Option<TokenStream>) -> TokenStream {
    value.map_or_else(|| quote! { None }, |value| quote! { Some(#value) })
}

/// Emits a `RegistrySet`.
pub fn registry_set(path: &str, value: &Value) -> TokenStream {
    match RegistrySetJson::parse(path, value) {
        RegistrySetJson::Tag(tag) => {
            let tag = identifier(path, &tag);
            quote! { RegistrySet::Tag(#tag) }
        }
        RegistrySetJson::Entries(entries) => {
            let entries = entries.iter().map(|entry| identifier(path, entry));
            quote! { RegistrySet::Entries(&[#(#entries),*]) }
        }
    }
}

/// Emits an `IntBounds`.
pub fn int_bounds(path: &str, value: &Value) -> TokenStream {
    let bounds: BoundsJson<i32> =
        serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (min, max) = bounds.min_max();
    let min = option(min.map(|min| quote! { #min }));
    let max = option(max.map(|max| quote! { #max }));
    quote! { IntBounds { min: #min, max: #max } }
}

/// Emits a `DoubleBounds`.
fn double_bounds_of(bounds: Option<BoundsJson<f64>>) -> TokenStream {
    let Some(bounds) = bounds else {
        return quote! { DoubleBounds::ANY };
    };
    let (min, max) = bounds.min_max();
    let min = option(min.map(|min| quote! { #min }));
    let max = option(max.map(|max| quote! { #max }));
    quote! { DoubleBounds { min: #min, max: #max } }
}

/// Emits a `DistancePredicate`.
pub fn distance(path: &str, value: &Value) -> TokenStream {
    let distance: DistanceJson =
        serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{path}: {e}"));
    let x = double_bounds_of(distance.x);
    let y = double_bounds_of(distance.y);
    let z = double_bounds_of(distance.z);
    let horizontal = double_bounds_of(distance.horizontal);
    let absolute = double_bounds_of(distance.absolute);
    quote! {
        DistancePredicate {
            x: #x, y: #y, z: #z, horizontal: #horizontal, absolute: #absolute,
        }
    }
}

/// Emits the `state` property list of a block predicate.
///
/// Vanilla parity: `StatePropertiesPredicate`. Only exact-value entries appear
/// in vanilla advancement data; a ranged one would need a different shape and
/// stops the build.
fn state_properties(path: &str, value: &Value) -> TokenStream {
    let Value::Object(entries) = value else {
        panic!("{path}: expected a state property object, found {value}");
    };
    let entries = entries.iter().map(|(name, expected)| {
        let expected = match expected {
            Value::String(text) => text.clone(),
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            other => panic!("{path}.{name}: only exact state values are modeled, found {other}"),
        };
        quote! { StatePropertyMatch { name: #name, value: #expected } }
    });
    quote! { &[#(#entries),*] }
}

/// Emits a `BlockPredicate`.
pub fn block_predicate(path: &str, value: &Value) -> TokenStream {
    let mut reader = ObjectReader::new(path, Some(value));
    let blocks = reader
        .take("blocks")
        .map(|blocks| registry_set(&reader.child_path("blocks"), &blocks));
    let state = reader.take("state").map_or_else(
        || quote! { &[] },
        |state| state_properties(&reader.child_path("state"), &state),
    );
    reader.finish();
    let blocks = option(blocks);
    quote! { BlockPredicate { blocks: #blocks, state: #state } }
}

/// Emits an `ItemPredicate`.
pub fn item_predicate(path: &str, value: &Value) -> TokenStream {
    let mut reader = ObjectReader::new(path, Some(value));

    let items = reader
        .take("items")
        .map(|items| registry_set(&reader.child_path("items"), &items));
    let count = reader.take("count").map_or_else(
        || quote! { IntBounds::ANY },
        |count| int_bounds(&reader.child_path("count"), &count),
    );

    let mut enchantments = Vec::new();
    let mut jukebox_playable = false;
    if let Some(predicates) = reader.take_object("predicates") {
        let (predicates_path, entries) = predicates.drain();
        for (key, entry) in entries {
            match key.as_str() {
                "minecraft:enchantments" => {
                    let Value::Array(list) = &entry else {
                        panic!("{predicates_path}.{key}: expected a list, found {entry}");
                    };
                    for (index, item) in list.iter().enumerate() {
                        let mut inner = ObjectReader::new(
                            format!("{predicates_path}.{key}[{index}]"),
                            Some(item),
                        );
                        let set = inner
                            .take("enchantments")
                            .map(|set| registry_set(&inner.child_path("enchantments"), &set));
                        let levels = inner.take("levels").map_or_else(
                            || quote! { IntBounds::ANY },
                            |levels| int_bounds(&inner.child_path("levels"), &levels),
                        );
                        inner.finish();
                        let set = option(set);
                        enchantments.push(quote! {
                            EnchantmentPredicate { enchantments: #set, levels: #levels }
                        });
                    }
                }
                "minecraft:jukebox_playable" => {
                    let inner = ObjectReader::new(format!("{predicates_path}.{key}"), Some(&entry));
                    assert!(
                        inner.is_empty(),
                        "{predicates_path}.{key}: only the bare presence check is modeled"
                    );
                    inner.finish();
                    jukebox_playable = true;
                }
                other => panic!(
                    "{predicates_path}: unmodeled item sub-predicate `{other}`. Model it or the \
                     predicate stops asking for it."
                ),
            }
        }
    }

    let mut damage = None;
    let mut banner_patterns = None;
    let mut item_name_translate = None;
    if let Some(components) = reader.take_object("components") {
        let (components_path, entries) = components.drain();
        for (key, entry) in entries {
            match key.as_str() {
                "minecraft:damage" => {
                    let Value::Number(number) = &entry else {
                        panic!("{components_path}.{key}: expected a number, found {entry}");
                    };
                    let value = i32::try_from(
                        number
                            .as_i64()
                            .unwrap_or_else(|| panic!("{components_path}.{key}: not an integer")),
                    )
                    .unwrap_or_else(|_| panic!("{components_path}.{key}: out of range"));
                    damage = Some(quote! { #value });
                }
                "minecraft:banner_patterns" => {
                    let Value::Array(list) = &entry else {
                        panic!("{components_path}.{key}: expected a list, found {entry}");
                    };
                    let layers = list.iter().enumerate().map(|(index, layer)| {
                        let layer_path = format!("{components_path}.{key}[{index}]");
                        let mut inner = ObjectReader::new(layer_path.clone(), Some(layer));
                        let color =
                            string_of(&inner.child_path("color"), &inner.take_required("color"));
                        let pattern_value = inner.take_required("pattern");
                        let pattern = identifier(
                            &inner.child_path("pattern"),
                            &string_of(&inner.child_path("pattern"), &pattern_value),
                        );
                        inner.finish();
                        quote! { BannerPatternLayer { color: #color, pattern: #pattern } }
                    });
                    banner_patterns = Some(quote! { &[#(#layers),*] });
                }
                "minecraft:item_name" => {
                    let mut inner =
                        ObjectReader::new(format!("{components_path}.{key}"), Some(&entry));
                    let translate_value = inner.take_required("translate");
                    let translate = string_of(&inner.child_path("translate"), &translate_value);
                    inner.finish();
                    item_name_translate = Some(quote! { #translate });
                }
                other => panic!(
                    "{components_path}: unmodeled item component check `{other}`. Model it or the \
                     predicate stops asking for it."
                ),
            }
        }
    }

    reader.finish();

    let items = option(items);
    let damage = option(damage);
    let banner_patterns = option(banner_patterns);
    let item_name_translate = option(item_name_translate);
    quote! {
        ItemPredicate {
            items: #items,
            count: #count,
            enchantments: &[#(#enchantments),*],
            jukebox_playable: #jukebox_playable,
            components: ItemComponentsPredicate {
                damage: #damage,
                banner_patterns: #banner_patterns,
                item_name_translate: #item_name_translate,
            },
        }
    }
}

/// Emits a `LocationPredicate`.
pub fn location_predicate(path: &str, value: &Value) -> TokenStream {
    let mut reader = ObjectReader::new(path, Some(value));

    let position: PositionJson = reader.take_as("position").unwrap_or_default();
    let x = double_bounds_of(position.x);
    let y = double_bounds_of(position.y);
    let z = double_bounds_of(position.z);

    let biomes = reader
        .take("biomes")
        .map(|biomes| registry_set(&reader.child_path("biomes"), &biomes));
    let structures = reader
        .take("structures")
        .map(|structures| registry_set(&reader.child_path("structures"), &structures));
    let dimension = reader.take("dimension").map(|dimension| {
        identifier(
            &reader.child_path("dimension"),
            &string_of(&reader.child_path("dimension"), &dimension),
        )
    });
    let block = reader
        .take("block")
        .map(|block| block_predicate(&reader.child_path("block"), &block));
    let smokey = reader.take("smokey").map(|smokey| {
        let Value::Bool(flag) = smokey else {
            panic!(
                "{}: expected a boolean, found {smokey}",
                reader.child_path("smokey")
            );
        };
        quote! { #flag }
    });
    reader.finish();

    let biomes = option(biomes);
    let structures = option(structures);
    let dimension = option(dimension);
    let block = option(block);
    let smokey = option(smokey);
    quote! {
        LocationPredicate {
            x: #x, y: #y, z: #z,
            biomes: #biomes,
            structures: #structures,
            dimension: #dimension,
            block: #block,
            smokey: #smokey,
        }
    }
}

/// Emits an `EntityPredicate` from the 26.2 sub-predicate map.
pub fn entity_predicate(path: &str, value: &Value) -> TokenStream {
    let mut reader = ObjectReader::new(path, Some(value));

    let entity_type = reader
        .take("minecraft:entity_type")
        .map(|set| registry_set(&reader.child_path("minecraft:entity_type"), &set));
    let location = reader.take("minecraft:location").map(|location| {
        let inner = location_predicate(&reader.child_path("minecraft:location"), &location);
        quote! { &#inner }
    });
    let stepping_on = reader.take("minecraft:stepping_on").map(|location| {
        let inner = location_predicate(&reader.child_path("minecraft:stepping_on"), &location);
        quote! { &#inner }
    });
    let distance_predicate = reader
        .take("minecraft:distance")
        .map(|value| distance(&reader.child_path("minecraft:distance"), &value));
    let vehicle = reader.take("minecraft:vehicle").map(|vehicle| {
        let inner = entity_predicate(&reader.child_path("minecraft:vehicle"), &vehicle);
        quote! { &#inner }
    });
    let passenger = reader.take("minecraft:passenger").map(|passenger| {
        let inner = entity_predicate(&reader.child_path("minecraft:passenger"), &passenger);
        quote! { &#inner }
    });

    let flags = reader.take_object("minecraft:flags").map(|mut flags| {
        let read = |flags: &mut ObjectReader, key: &str| {
            option(flags.take(key).map(|value| {
                let Value::Bool(flag) = value else {
                    panic!("expected a boolean flag, found {value}");
                };
                quote! { #flag }
            }))
        };
        let is_on_fire = read(&mut flags, "is_on_fire");
        let is_sneaking = read(&mut flags, "is_sneaking");
        let is_sprinting = read(&mut flags, "is_sprinting");
        let is_swimming = read(&mut flags, "is_swimming");
        let is_baby = read(&mut flags, "is_baby");
        let is_flying = read(&mut flags, "is_flying");
        flags.finish();
        quote! {
            EntityFlagsPredicate {
                is_on_fire: #is_on_fire,
                is_sneaking: #is_sneaking,
                is_sprinting: #is_sprinting,
                is_swimming: #is_swimming,
                is_baby: #is_baby,
                is_flying: #is_flying,
            }
        }
    });

    let equipment = reader
        .take_object("minecraft:equipment")
        .map(|mut equipment| {
            let head = optional_item_predicate(&mut equipment, "head");
            let chest = optional_item_predicate(&mut equipment, "chest");
            let legs = optional_item_predicate(&mut equipment, "legs");
            let feet = optional_item_predicate(&mut equipment, "feet");
            let mainhand = optional_item_predicate(&mut equipment, "mainhand");
            let offhand = optional_item_predicate(&mut equipment, "offhand");
            let body = optional_item_predicate(&mut equipment, "body");
            equipment.finish();
            quote! {
                &EntityEquipmentPredicate {
                    head: #head, chest: #chest, legs: #legs, feet: #feet,
                    mainhand: #mainhand, offhand: #offhand, body: #body,
                }
            }
        });

    let mut components = Vec::new();
    if let Some(map) = reader.take_object("minecraft:components") {
        let (components_path, entries) = map.drain();
        for (key, entry) in entries {
            let expected = identifier(
                &format!("{components_path}.{key}"),
                &string_of(&format!("{components_path}.{key}"), &entry),
            );
            components.push(quote! {
                EntityComponentMatch { component: #key, value: #expected }
            });
        }
    }

    let mut looking_at = None;
    if let Some(mut player) = reader.take_object("minecraft:type_specific/player") {
        if let Some(target) = player.take("looking_at") {
            let inner = entity_predicate(&player.child_path("looking_at"), &target);
            looking_at = Some(quote! { &#inner });
        }
        player.finish();
    }

    let mut lightning_blocks_set_on_fire = None;
    if let Some(mut lightning) = reader.take_object("minecraft:type_specific/lightning") {
        if let Some(blocks) = lightning.take("blocks_set_on_fire") {
            lightning_blocks_set_on_fire = Some(int_bounds(
                &lightning.child_path("blocks_set_on_fire"),
                &blocks,
            ));
        }
        lightning.finish();
    }

    reader.finish();

    let entity_type = option(entity_type);
    let location = option(location);
    let stepping_on = option(stepping_on);
    let distance_predicate = option(distance_predicate);
    let flags = option(flags);
    let equipment = option(equipment);
    let vehicle = option(vehicle);
    let passenger = option(passenger);
    let looking_at = option(looking_at);
    let lightning_blocks_set_on_fire = option(lightning_blocks_set_on_fire);
    quote! {
        EntityPredicate {
            entity_type: #entity_type,
            location: #location,
            stepping_on: #stepping_on,
            distance: #distance_predicate,
            flags: #flags,
            equipment: #equipment,
            components: &[#(#components),*],
            vehicle: #vehicle,
            passenger: #passenger,
            looking_at: #looking_at,
            lightning_blocks_set_on_fire: #lightning_blocks_set_on_fire,
        }
    }
}

/// Emits one `ConditionTerm`.
fn condition_term(path: &str, value: &Value) -> TokenStream {
    let mut reader = ObjectReader::new(path, Some(value));
    let condition_value = reader.take_required("condition");
    let condition = string_of(&reader.child_path("condition"), &condition_value);

    let term = match condition.as_str() {
        "minecraft:entity_properties" => {
            let entity_value = reader.take_required("entity");
            let entity = string_of(&reader.child_path("entity"), &entity_value);
            assert_eq!(
                entity, "this",
                "{path}: only the `this` loot target is modeled; the subject of an advancement \
                 predicate is whatever the trigger handed it"
            );
            let predicate_value = reader.take_required("predicate");
            let predicate = entity_predicate(&reader.child_path("predicate"), &predicate_value);
            quote! { ConditionTerm::EntityProperties(&#predicate) }
        }
        "minecraft:location_check" => {
            let offset_x: i32 = reader.take_as("offsetX").unwrap_or(0);
            let offset_y: i32 = reader.take_as("offsetY").unwrap_or(0);
            let offset_z: i32 = reader.take_as("offsetZ").unwrap_or(0);
            let predicate_value = reader.take_required("predicate");
            let predicate = location_predicate(&reader.child_path("predicate"), &predicate_value);
            quote! {
                ConditionTerm::LocationCheck {
                    offset_x: #offset_x,
                    offset_y: #offset_y,
                    offset_z: #offset_z,
                    predicate: &#predicate,
                }
            }
        }
        "minecraft:match_tool" => {
            let predicate_value = reader.take_required("predicate");
            let predicate = item_predicate(&reader.child_path("predicate"), &predicate_value);
            quote! { ConditionTerm::MatchTool(&#predicate) }
        }
        "minecraft:block_state_property" => {
            let block_value = reader.take_required("block");
            let block = identifier(
                &reader.child_path("block"),
                &string_of(&reader.child_path("block"), &block_value),
            );
            let properties = reader.take("properties").map_or_else(
                || quote! { &[] },
                |properties| state_properties(&reader.child_path("properties"), &properties),
            );
            quote! {
                ConditionTerm::BlockStateProperty { block: #block, properties: #properties }
            }
        }
        "minecraft:any_of" | "minecraft:all_of" => {
            let terms_value = reader.take_required("terms");
            let terms = condition_terms(&reader.child_path("terms"), &terms_value);
            if condition == "minecraft:any_of" {
                quote! { ConditionTerm::AnyOf(#terms) }
            } else {
                quote! { ConditionTerm::AllOf(#terms) }
            }
        }
        "minecraft:inverted" => {
            let term_value = reader.take_required("term");
            let term = condition_term(&reader.child_path("term"), &term_value);
            quote! { ConditionTerm::Inverted(&#term) }
        }
        other => panic!(
            "{path}: unmodeled advancement condition `{other}`. Model it or the criterion stops \
             asking for it."
        ),
    };

    reader.finish();
    term
}

/// Emits a `ContextAwarePredicate`, which is a slice of terms.
pub fn condition_terms(path: &str, value: &Value) -> TokenStream {
    let Value::Array(list) = value else {
        panic!("{path}: expected a list of conditions, found {value}");
    };
    let terms = list
        .iter()
        .enumerate()
        .map(|(index, term)| condition_term(&format!("{path}[{index}]"), term));
    quote! { &[#(#terms),*] }
}

/// Emits a `ContextAwarePredicate` for an optional field, empty when absent.
pub fn optional_condition_terms(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    reader
        .take(key)
        .map_or_else(|| quote! { &[] }, |value| condition_terms(&path, &value))
}

/// Emits a `DamageSourcePredicate`.
pub fn damage_source_predicate(path: &str, value: &Value) -> TokenStream {
    let mut reader = ObjectReader::new(path, Some(value));

    let tags = reader.take("tags").map_or_else(
        || quote! { &[] },
        |tags| {
            let Value::Array(list) = &tags else {
                panic!("{path}.tags: expected a list, found {tags}");
            };
            let entries = list.iter().enumerate().map(|(index, entry)| {
                let entry_path = format!("{path}.tags[{index}]");
                let mut inner = ObjectReader::new(entry_path.clone(), Some(entry));
                let id_value = inner.take_required("id");
                let id = identifier(
                    &inner.child_path("id"),
                    &string_of(&inner.child_path("id"), &id_value),
                );
                let expected: bool = inner
                    .take_as("expected")
                    .unwrap_or_else(|| panic!("{entry_path}: `expected` is required"));
                inner.finish();
                quote! { DamageTypeTagMatch { id: #id, expected: #expected } }
            });
            quote! { &[#(#entries),*] }
        },
    );

    let direct_entity = reader.take("direct_entity").map(|entity| {
        let inner = entity_predicate(&reader.child_path("direct_entity"), &entity);
        quote! { &#inner }
    });
    let source_entity = reader.take("source_entity").map(|entity| {
        let inner = entity_predicate(&reader.child_path("source_entity"), &entity);
        quote! { &#inner }
    });
    reader.finish();

    let direct_entity = option(direct_entity);
    let source_entity = option(source_entity);
    quote! {
        DamageSourcePredicate {
            tags: #tags,
            direct_entity: #direct_entity,
            source_entity: #source_entity,
        }
    }
}

/// Emits a `DamagePredicate`.
pub fn damage_predicate(path: &str, value: &Value) -> TokenStream {
    let mut reader = ObjectReader::new(path, Some(value));

    let dealt: Option<BoundsJson<f64>> = reader.take_as("dealt");
    let taken: Option<BoundsJson<f64>> = reader.take_as("taken");
    let dealt = double_bounds_of(dealt);
    let taken = double_bounds_of(taken);
    let blocked = reader.take("blocked").map(|blocked| {
        let Value::Bool(flag) = blocked else {
            panic!("{path}.blocked: expected a boolean, found {blocked}");
        };
        quote! { #flag }
    });
    let source = reader.take("type").map(|source| {
        let inner = damage_source_predicate(&reader.child_path("type"), &source);
        quote! { &#inner }
    });
    let source_entity = reader.take("source_entity");
    assert!(
        source_entity.is_none(),
        "{path}: `source_entity` on a damage predicate is not modeled yet"
    );
    reader.finish();

    let blocked = option(blocked);
    let source = option(source);
    quote! {
        DamagePredicate {
            dealt: #dealt,
            taken: #taken,
            blocked: #blocked,
            source: #source,
        }
    }
}

/// Emits an `Option<ItemPredicate>` for an optional key.
pub fn optional_item_predicate(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    option(reader.take(key).map(|value| item_predicate(&path, &value)))
}

/// Emits an `Option<Identifier>` for an optional key.
pub fn optional_identifier(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    option(
        reader
            .take(key)
            .map(|value| identifier(&path, &string_of(&path, &value))),
    )
}

/// Emits a required `Identifier`.
pub fn required_identifier(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    let value = reader.take_required(key);
    identifier(&path, &string_of(&path, &value))
}

/// Emits an `IntBounds` for an optional key, `ANY` when absent.
pub fn optional_int_bounds(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    reader.take(key).map_or_else(
        || quote! { IntBounds::ANY },
        |value| int_bounds(&path, &value),
    )
}

/// Emits a `DistancePredicate` for an optional key, all-`ANY` when absent.
pub fn optional_distance(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    reader.take(key).map_or_else(
        || {
            quote! {
                DistancePredicate {
                    x: DoubleBounds::ANY,
                    y: DoubleBounds::ANY,
                    z: DoubleBounds::ANY,
                    horizontal: DoubleBounds::ANY,
                    absolute: DoubleBounds::ANY,
                }
            }
        },
        |value| distance(&path, &value),
    )
}

/// Emits an `Option<&'static LocationPredicate>` for an optional key.
pub fn optional_location(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    option(reader.take(key).map(|value| {
        let inner = location_predicate(&path, &value);
        quote! { &#inner }
    }))
}

/// Emits an `Option<DamagePredicate>` for an optional key.
pub fn optional_damage(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    option(
        reader
            .take(key)
            .map(|value| damage_predicate(&path, &value)),
    )
}

/// Emits an `Option<DamageSourcePredicate>` for an optional key.
///
/// Vanilla parity: `KilledTrigger`'s `killing_blow` is a `DamageSourcePredicate`,
/// not the `DamagePredicate` its `player_hurt_entity` neighbour uses.
pub fn optional_damage_source(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    option(
        reader
            .take(key)
            .map(|value| damage_source_predicate(&path, &value)),
    )
}

/// Emits an `Option<i32>` for an optional key.
pub fn optional_i32(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    option(reader.take(key).map(|value| {
        let number: i64 = value
            .as_i64()
            .unwrap_or_else(|| panic!("{path}: expected an integer, found {value}"));
        let number =
            i32::try_from(number).unwrap_or_else(|_| panic!("{path}: {number} is out of range"));
        quote! { #number }
    }))
}
