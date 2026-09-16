use super::*;
use foton_registry::data_component_predicate::DataComponentExactPredicate;
use foton_registry::data_components::components::{BundleContents, CustomData, EntityData};
use foton_registry::data_components::vanilla_components::{CAN_BREAK, CAN_PLACE_ON, ENTITY_DATA};
use foton_registry::data_components::{ComponentData, ComponentEntryRef, DataComponentPatch};
use foton_registry::item_predicate::{AdventureModePredicate, BlockPredicate};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

fn entry(name: &'static str) -> ComponentEntryRef {
    REGISTRY
        .data_components
        .by_key(&Identifier::vanilla_static(name))
        .expect("registered component")
}

fn exact(name: &'static str, value: ComponentData) -> DataComponentMatchers {
    DataComponentMatchers::new(
        DataComponentExactPredicate::new(vec![(entry(name), value)]).expect("exact predicate"),
        vec![],
    )
    .expect("matchers")
}

fn lock(name: &'static str, value: ComponentData) -> LockCode {
    LockCode::new(ItemPredicate::new(None, IntBounds::ANY, exact(name, value)))
}

fn envelope(name: &'static str, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for n in [
        1,
        vanilla_items::STONE.id() as i32,
        1,
        0,
        REGISTRY
            .data_components
            .id_from_key(&entry(name).key)
            .expect("id") as i32,
    ] {
        VarInt(n).write(&mut bytes).expect("envelope");
    }
    bytes.extend_from_slice(body);
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    bytes
}

#[test]
fn brewing_reachable_entity_data_output_direct_and_exact() {
    init_vanilla_registry();
    let mut compound = NbtCompound::new();
    compound.insert("payload", NbtTag::ByteArray(vec![0; 2 << 20]));
    let pig = REGISTRY
        .entity_types
        .by_key(&Identifier::vanilla_static("pig"))
        .expect("pig");
    let data = EntityData::new(pig, CustomData::try_from_compound(compound).expect("data"));
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(ENTITY_DATA, data.clone());
    let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
    snapshot.lock = lock("entity_data", ComponentData::new(data));
    let direct = allocation_counter::measure(|| assert!(encode_brewing_item(&item).is_none()));
    let exact =
        allocation_counter::measure(|| assert!(encode_brewing_snapshot(&snapshot).is_none()));
    eprintln!("entity direct={direct:?}, exact={exact:?}");
    assert!(direct.bytes_max < 1 << 20 && exact.bytes_max < 1 << 20);
}

#[test]
fn brewing_reachable_custom_model_declared_strings() {
    init_vanilla_registry();
    let mut body = vec![0, 0];
    VarInt(65_536).write(&mut body).expect("count");
    let mut bytes = envelope("custom_model_data", &body);
    bytes.truncate(bytes.len() - 4); // exact audit shape: then EOF
    let stats = allocation_counter::measure(|| assert!(read_brewing_item(&bytes).is_none()));
    eprintln!("custom model declared strings: {stats:?}");
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn brewing_reachable_owned_nbt_empty_string_list() {
    init_vanilla_registry();
    let mut compound = NbtCompound::new();
    compound.insert(
        "strings",
        NbtTag::List(NbtList::String(vec!["".into(); 65_536])),
    );
    let mut body = Vec::new();
    NbtTag::Compound(compound).write(&mut body);
    let bytes = envelope("custom_data", &body);
    assert!(bytes.len() < MAX_BREWING_ITEM_BYTES);
    let stats = allocation_counter::measure(|| assert!(read_brewing_item(&bytes).is_none()));
    eprintln!("owned NBT empty strings: {stats:?}");
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn brewing_reachable_nested_patch_map_retention() {
    init_vanilla_registry();
    let mut removed: Vec<_> = ["custom_name", "lore", "damage", "repair_cost"]
        .into_iter()
        .map(|name| {
            REGISTRY
                .data_components
                .id_from_key(&entry(name).key)
                .expect("id") as i32
        })
        .collect();
    removed.sort_unstable();
    let mut body = Vec::new();
    VarInt(8000).write(&mut body).expect("templates");
    for _ in 0..8000 {
        for n in [vanilla_items::STONE.id() as i32, 1, 0, 4] {
            VarInt(n).write(&mut body).expect("template");
        }
        for &id in &removed {
            VarInt(id).write(&mut body).expect("removed");
        }
    }
    let bytes = envelope("bundle_contents", &body);
    assert!(bytes.len() < MAX_BREWING_ITEM_BYTES);
    let mut rejected = false;
    let stats = allocation_counter::measure(|| rejected = read_brewing_item(&bytes).is_none());
    eprintln!("nested patch maps: rejected={rejected}, {stats:?}");
    assert!(rejected && stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn brewing_reachable_adventure_nested_template_canonicality() {
    init_vanilla_registry();
    for name in [
        "bundle_contents",
        "charged_projectiles",
        "container",
        "use_remainder",
        "sulfur_cube_content",
    ] {
        for component in [CAN_BREAK, CAN_PLACE_ON] {
            let encode = |reverse| {
                let template = super::recursive_bounds::populated_template(reverse);
                let adventure = AdventureModePredicate::new(vec![BlockPredicate::new(
                    None,
                    None,
                    None,
                    exact(name, recursive_value(name, template)),
                )])
                .expect("predicate");
                let mut item = ItemStack::new(&vanilla_items::STONE);
                item.set(component.clone(), adventure.clone());
                let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
                snapshot.lock = lock(
                    if component.key() == CAN_BREAK.key() {
                        "can_break"
                    } else {
                        "can_place_on"
                    },
                    ComponentData::new(adventure),
                );
                (
                    encode_brewing_item(&item).expect("item"),
                    encode_brewing_snapshot(&snapshot).expect("snapshot"),
                )
            };
            let a = encode(false);
            // Compare the indirect adventure payload to the direct canonical template codec.
            let template = super::recursive_bounds::populated_template(false);
            let mut direct = Vec::new();
            entry(name)
                .write_network_bounded(
                    &recursive_value(name, template),
                    MAX_BREWING_ITEM_BYTES,
                    &mut direct,
                )
                .expect("direct bounded bundle");
            let mut expected_body = vec![1, 0, 0, 0, 1]; // predicate, absent block/state/NBT, exact count
            VarInt(
                REGISTRY
                    .data_components
                    .id_from_key(&entry(name).key)
                    .expect("recursive component id") as i32,
            )
            .write(&mut expected_body)
            .expect("id");
            expected_body.extend_from_slice(&direct);
            expected_body.push(0); // no partial predicates
            let expected = envelope(
                if component.key() == CAN_BREAK.key() {
                    "can_break"
                } else {
                    "can_place_on"
                },
                &expected_body,
            );
            assert_eq!(a.0, expected, "indirect template IDs must be sorted");
            let b = encode(true);
            assert_eq!(a, b, "indirect nested patch order depends on history");
            assert!(read_brewing_item(&a.0).is_some());
            assert!(decode_brewing_snapshot(&a.1).is_some());
        }
    }
}

#[test]
fn brewing_reachable_snapshot_lock_rejects_permuted_nested_fields() {
    init_vanilla_registry();
    for wrapper in [None, Some("can_break"), Some("can_place_on")] {
        let mut patch = DataComponentPatch::new();
        patch.remove(LORE);
        patch.remove(CUSTOM_NAME);
        let template = ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
            .expect("template");
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        let bundle = ComponentData::new(BundleContents::new(vec![template]));
        value.lock = if let Some(name) = wrapper {
            let adventure = AdventureModePredicate::new(vec![BlockPredicate::new(
                None,
                None,
                None,
                exact("bundle_contents", bundle),
            )])
            .expect("adventure");
            lock(name, ComponentData::new(adventure))
        } else {
            lock("bundle_contents", bundle)
        };
        let mut canonical_lock = Vec::new();
        value
            .lock
            .write_bounded(65536, &mut canonical_lock)
            .expect("lock");
        let mut bytes = encode_brewing_snapshot(&value).expect("snapshot");
        let offset = bytes.len() - canonical_lock.len() - 4;
        let mut patch = NbtCompound::new();
        patch.insert("!minecraft:lore", NbtCompound::new());
        patch.insert("!minecraft:custom_name", NbtCompound::new());
        let mut template = NbtCompound::new();
        template.insert("id", "minecraft:stone");
        template.insert("components", patch);
        let mut exact = NbtCompound::new();
        exact.insert(
            "minecraft:bundle_contents",
            NbtList::Compound(vec![template]),
        );
        let mut predicate = NbtCompound::new();
        predicate.insert("components", exact);
        if let Some(name) = wrapper {
            let mut outer = NbtCompound::new();
            outer.insert(format!("minecraft:{name}"), predicate);
            predicate = NbtCompound::new();
            predicate.insert("components", outer);
        }
        let mut permuted = Vec::new();
        NbtTag::Compound(predicate).write(&mut permuted);
        bytes.truncate(offset);
        bytes.extend_from_slice(&(permuted.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&permuted);
        assert!(
            decode_brewing_snapshot(&bytes).is_none(),
            "noncanonical lock was accepted"
        );
    }
}

#[test]
fn brewing_reachable_current_collection_reservations() {
    init_vanilla_registry();
    let mut count = Vec::new();
    VarInt(65_536).write(&mut count).expect("count");
    let mut failures = Vec::new();
    for (name, prefix) in [
        ("tool", vec![]),
        ("attribute_modifiers", vec![]),
        ("enchantments", vec![]),
        ("tooltip_display", vec![0]),
        ("bees", vec![]),
        ("potion_contents", vec![0, 0]),
        ("banner_patterns", vec![]),
        ("blocks_attacks", vec![0; 8]),
        ("death_protection", vec![]),
        ("consumable", vec![0, 0, 0, 0, 0, 1, 0]),
    ] {
        let mut body = prefix;
        body.extend_from_slice(&count);
        let mut bytes = envelope(name, &body);
        bytes.truncate(bytes.len() - 4);
        let stats = allocation_counter::measure(|| assert!(read_brewing_item(&bytes).is_none()));
        eprintln!("collection {name}: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "unbounded readers: {failures:?}");
}

#[test]
fn brewing_reachable_nbt_readers_preflight_before_ownership() {
    init_vanilla_registry();
    let mut compound = NbtCompound::new();
    compound.insert("strings", NbtList::String(vec!["".into(); 65_536]));
    let mut tag = Vec::new();
    NbtTag::Compound(compound).write(&mut tag);
    let mut failures = Vec::new();
    for (name, prefix) in [
        ("bucket_entity_data", vec![]),
        ("entity_data", vec![0]),
        ("block_entity_data", vec![0]),
        ("custom_name", vec![]),
        ("item_name", vec![]),
        ("lore", vec![1]),
        ("lock", vec![]),
        ("recipes", vec![]),
        ("map_decorations", vec![]),
        ("debug_stick_state", vec![]),
        ("intangible_projectile", vec![]),
    ] {
        let mut body = prefix;
        body.extend_from_slice(&tag);
        let bytes = envelope(name, &body);
        let stats = allocation_counter::measure(|| assert!(read_brewing_item(&bytes).is_none()));
        eprintln!("NBT reader {name}: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "unbounded NBT readers: {failures:?}");
}

#[test]
fn brewing_reachable_typed_nbt_and_recipe_encoders() {
    use foton_registry::data_components::components::{
        BeehiveOccupant, Bees, BlockEntityData, Recipes,
    };
    init_vanilla_registry();
    let mut compound = NbtCompound::new();
    compound.insert("bytes", NbtTag::ByteArray(vec![0; 2 << 20]));
    let data = CustomData::try_from_compound(compound).expect("data");
    let pig = REGISTRY
        .entity_types
        .by_key(&Identifier::vanilla_static("pig"))
        .expect("pig");
    let chest = REGISTRY
        .block_entity_types
        .by_key(&Identifier::vanilla_static("chest"))
        .expect("chest");
    let mut failures = Vec::new();
    for (name, value) in [
        (
            "block_entity_data",
            ComponentData::new(BlockEntityData::new(chest, data.clone())),
        ),
        (
            "bees",
            ComponentData::new(Bees::new(vec![BeehiveOccupant::new(
                EntityData::new(pig, data),
                0,
                0,
            )])),
        ),
        (
            "recipes",
            ComponentData::new(Recipes::new(vec![
                Identifier::vanilla_static("stone");
                100_000
            ])),
        ),
    ] {
        let mut patch = DataComponentPatch::new();
        assert!(patch.set_raw(entry(name).key.clone(), value.clone()));
        let item = ItemStack::from_raw_parts(&vanilla_items::STONE, 1, patch);
        let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
        snapshot.lock = lock(name, value);
        let direct = allocation_counter::measure(|| assert!(encode_brewing_item(&item).is_none()));
        let exact =
            allocation_counter::measure(|| assert!(encode_brewing_snapshot(&snapshot).is_none()));
        eprintln!("encoder {name}: direct={direct:?}, exact={exact:?}");
        if direct.bytes_max >= 1 << 20 || exact.bytes_max >= 1 << 20 {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "unbounded encoders: {failures:?}");
}

#[test]
fn brewing_reachable_inline_text_and_consume_effect_encoders() {
    use foton_registry::consume_effect::{
        ApplyStatusEffectsConsumeEffect, ConsumeEffectData, vanilla_consume_effect_types,
    };
    use foton_registry::data_components::components::{
        DeathProtection, InstrumentComponent, ItemAttributeModifierDisplay,
        ItemAttributeModifierEntry, ItemAttributeModifiers,
    };
    use foton_registry::{
        MobEffectInstance, RegistryHolder, attribute::AttributeModifierOperation,
        equipment::EquipmentSlotGroup, instrument::InstrumentValue, sound_event::SoundEventHolder,
        sound_events,
    };
    init_vanilla_registry();
    let sound = SoundEventHolder::registry(&sound_events::ENTITY_GENERIC_EAT);
    let text = || TextComponent::plain("x".repeat(2 << 20));
    let attribute = REGISTRY
        .attributes
        .by_key(&Identifier::vanilla_static("max_health"))
        .expect("attribute");
    let effect = REGISTRY
        .mob_effects
        .by_key(&Identifier::vanilla_static("speed"))
        .expect("effect");
    let mut failures = Vec::new();
    for (name, value) in [
        (
            "instrument",
            ComponentData::new(InstrumentComponent::new(RegistryHolder::direct(
                InstrumentValue::new(sound, 1.0, 1.0, text()).expect("instrument"),
            ))),
        ),
        (
            "attribute_modifiers",
            ComponentData::new(ItemAttributeModifiers {
                modifiers: vec![ItemAttributeModifierEntry {
                    attribute,
                    id: Identifier::vanilla_static("test"),
                    amount: 1.0,
                    operation: AttributeModifierOperation::AddValue,
                    slot: EquipmentSlotGroup::Any,
                    display: ItemAttributeModifierDisplay::OverrideText(Box::new(text())),
                }],
            }),
        ),
        (
            "death_protection",
            ComponentData::new(DeathProtection::new(vec![ConsumeEffectData::new(
                &vanilla_consume_effect_types::APPLY_EFFECTS,
                ApplyStatusEffectsConsumeEffect::new(
                    vec![MobEffectInstance::simple(effect, 100, 0); 200_000],
                    1.0,
                )
                .expect("effect"),
            )])),
        ),
    ] {
        let mut patch = DataComponentPatch::new();
        assert!(patch.set_raw(entry(name).key.clone(), value));
        let item = ItemStack::from_raw_parts(&vanilla_items::STONE, 1, patch);
        let stats = allocation_counter::measure(|| assert!(encode_brewing_item(&item).is_none()));
        eprintln!("inline/effect encoder {name}: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "unbounded inline/effect encoders: {failures:?}"
    );
}

#[test]
fn brewing_reachable_snapshot_aggregate_rejects_before_large_output() {
    init_vanilla_registry();
    let items = (0..5)
        .map(|_| {
            let mut item = ItemStack::new(&vanilla_items::STONE);
            item.set_opaque_nbt(Some("x".repeat(230_000)));
            item
        })
        .collect();
    let value = snapshot(items);
    let stats = allocation_counter::measure(|| assert!(encode_brewing_snapshot(&value).is_none()));
    eprintln!("snapshot aggregate: {stats:?}");
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn brewing_reachable_vanilla_prototype_codec_parity() {
    use std::collections::BTreeSet;
    init_vanilla_registry();
    let mut types = BTreeSet::new();
    for (_, item_type) in REGISTRY.items.iter() {
        for (key, value) in item_type.components.iter() {
            let component = REGISTRY
                .data_components
                .by_key(key)
                .expect("registered prototype component");
            let mut ordinary = Vec::new();
            component
                .write_network(value, &mut ordinary)
                .expect("ordinary network codec");
            let mut bounded = Vec::new();
            component
                .write_network_bounded(value, MAX_BREWING_ITEM_BYTES, &mut bounded)
                .expect("bounded network codec");
            assert_eq!(bounded, ordinary, "network bytes for {key}");
            if component.is_persistent() {
                let lock = lock_key(component, value.clone());
                ordinary.clear();
                bounded.clear();
                lock.write(&mut ordinary).expect("ordinary lock");
                lock.write_bounded(65536, &mut bounded)
                    .expect("bounded lock");
                assert_eq!(bounded, ordinary, "persistent bytes for {key}");
                let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
                snapshot.lock = lock;
                let bytes = encode_brewing_snapshot(&snapshot).expect("snapshot");
                assert!(
                    decode_brewing_snapshot(&bytes).is_some(),
                    "persistent decode for {key}"
                );
            }
            types.insert(key.to_string());
        }
    }
    eprintln!("prototype codec parity: {} types: {types:?}", types.len());
}

fn lock_key(entry: ComponentEntryRef, value: ComponentData) -> LockCode {
    LockCode::new(ItemPredicate::new(
        None,
        IntBounds::ANY,
        DataComponentMatchers::new(
            DataComponentExactPredicate::new(vec![(entry, value)]).expect("exact"),
            vec![],
        )
        .expect("matchers"),
    ))
}

#[test]
fn brewing_reachable_map_decorations_output_has_no_owned_tree() {
    use foton_registry::data_components::components::{MapDecorationEntry, MapDecorations};
    use std::collections::BTreeMap;
    init_vanilla_registry();
    let decoration = REGISTRY
        .map_decoration_types
        .by_key(&Identifier::vanilla_static("player"))
        .expect("decoration");
    let value = MapDecorations::new(
        (0..30000)
            .map(|i| {
                (
                    format!("mark_{i}"),
                    MapDecorationEntry::new(RegistryReference::new(decoration), 0.0, 0.0, 0.0),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    );
    let mut patch = DataComponentPatch::new();
    assert!(patch.set_raw(
        entry("map_decorations").key.clone(),
        ComponentData::new(value.clone())
    ));
    let item = ItemStack::from_raw_parts(&vanilla_items::STONE, 1, patch);
    let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
    snapshot.lock = lock("map_decorations", ComponentData::new(value));
    let direct = allocation_counter::measure(|| assert!(encode_brewing_item(&item).is_none()));
    let exact =
        allocation_counter::measure(|| assert!(encode_brewing_snapshot(&snapshot).is_none()));
    eprintln!("map decorations: direct={direct:?}, exact={exact:?}");
    assert!(direct.bytes_max < 1 << 20 && exact.bytes_max < 1 << 20);
}

fn recursive_value(name: &str, template: ItemStackTemplate) -> ComponentData {
    use foton_registry::data_components::components::{
        ChargedProjectiles, ItemContainerContents, SulfurCubeContent, UseRemainder,
    };
    match name {
        "bundle_contents" => ComponentData::new(BundleContents::new(vec![template])),
        "charged_projectiles" => {
            ComponentData::new(ChargedProjectiles::new(vec![template]).expect("projectile"))
        }
        "container" => ComponentData::new(
            ItemContainerContents::new(vec![None, Some(template)]).expect("container"),
        ),
        "use_remainder" => ComponentData::new(UseRemainder::new(template)),
        "sulfur_cube_content" => ComponentData::new(SulfurCubeContent::new(template)),
        _ => unreachable!("test component"),
    }
}
