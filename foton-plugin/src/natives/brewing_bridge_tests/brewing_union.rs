use super::*;
use foton_registry::data_component_predicate::{
    DamagePredicate, DataComponentPredicateData, DataComponentPredicateType,
};
use foton_registry::data_components::{DataComponentPatch, components::*};
use foton_registry::{
    RegistryHolder, instrument::InstrumentValue, sound_event::SoundEventHolder,
    trim_material::canonical_override_work,
};
use foton_utils::serial::{ReadFrom, nbt_encode, text_stream::child_classification_work};
use rustc_hash::FxHashMap;
use std::io::sink;
use text_components::{
    custom::{CustomData as TextCustomData, Payload},
    interactivity::ClickEvent,
};

fn custom_text(reverse: bool) -> TextComponent {
    let mut payload = NbtCompound::new();
    for (key, value) in if reverse {
        [("b", 2), ("a", 1)]
    } else {
        [("a", 1), ("b", 2)]
    } {
        payload.insert(key, value);
    }
    let mut text = TextComponent::plain("text");
    text.interactions.click = Some(ClickEvent::Custom(TextCustomData {
        id: "test:click".into(),
        payload: Payload::Nbt(NbtTag::Compound(payload).into()),
    }));
    text
}

fn indirect(name: &str, text: TextComponent) -> ComponentData {
    use foton_registry::{
        attribute::AttributeModifierOperation, equipment::EquipmentSlotGroup,
        jukebox_song::JukeboxSongValue, painting_variant::PaintingVariantValue, sound_events,
        trim_pattern::TrimPatternValue, vanilla_trim_materials, vanilla_trim_patterns,
    };
    let sound = || SoundEventHolder::registry(&sound_events::ITEM_GOAT_HORN_SOUND_0);
    match name {
        "instrument" => ComponentData::new(InstrumentComponent::new(RegistryHolder::direct(
            InstrumentValue::new(sound(), 1.0, 16.0, text).expect("instrument"),
        ))),
        "attribute_modifiers" => ComponentData::new(ItemAttributeModifiers {
            modifiers: vec![ItemAttributeModifierEntry {
                attribute: REGISTRY.attributes.iter().next().expect("attribute").1,
                id: Identifier::new("test", "modifier"),
                amount: 1.0,
                operation: AttributeModifierOperation::AddValue,
                slot: EquipmentSlotGroup::Any,
                display: ItemAttributeModifierDisplay::OverrideText(Box::new(text)),
            }],
        }),
        "trim" => ComponentData::new(ArmorTrim::new(
            RegistryHolder::reference(&vanilla_trim_materials::QUARTZ),
            RegistryHolder::direct(TrimPatternValue::new(
                Identifier::new("test", "pattern"),
                text,
                false,
            )),
        )),
        "jukebox_playable" => ComponentData::new(JukeboxPlayable::direct(JukeboxSongValue {
            sound_event: sound(),
            description: text,
            length_in_seconds: 1.0,
            comparator_output: 1,
        })),
        "painting/variant" | "painting/author" => {
            let (title, author) = if name == "painting/author" {
                (Some(TextComponent::plain("title")), Some(text))
            } else {
                (Some(text), None)
            };
            ComponentData::new(PaintingVariantComponent::direct(PaintingVariantValue {
                width: 1,
                height: 1,
                asset_id: Identifier::new("test", "painting"),
                title,
                author,
            }))
        }
        "provides_trim_material" | "trim/material" => {
            use foton_registry::trim_material::{
                MaterialAssetGroup, MaterialAssetInfo, TrimMaterialValue,
            };
            let material = RegistryHolder::direct(TrimMaterialValue::new(
                MaterialAssetGroup::new(
                    MaterialAssetInfo::new("a").expect("asset"),
                    FxHashMap::default(),
                ),
                text,
            ));
            if name == "trim/material" {
                ComponentData::new(ArmorTrim::new(
                    material,
                    RegistryHolder::reference(&vanilla_trim_patterns::SENTRY),
                ))
            } else {
                ComponentData::new(ProvidesTrimMaterial::new(material))
            }
        }
        _ => panic!("unknown fixture"),
    }
}
const INDIRECT: [&str; 8] = [
    "instrument",
    "attribute_modifiers",
    "trim",
    "jukebox_playable",
    "painting/variant",
    "painting/author",
    "provides_trim_material",
    "trim/material",
];
fn indirect_entry(name: &'static str) -> ComponentEntryRef {
    entry(match name {
        "painting/author" => "painting/variant",
        "trim/material" => "trim",
        _ => name,
    })
}

#[test]
fn brewing_union_indirect_text_is_canonical() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for name in INDIRECT {
        let encode = |reverse| {
            let mut patch = DataComponentPatch::new();
            assert!(patch.set_raw(
                indirect_entry(name).key.clone(),
                indirect(name, custom_text(reverse))
            ));
            encode_brewing_item(&ItemStack::from_raw_parts(&vanilla_items::STONE, 0, patch))
                .expect("encode")
        };
        let a = encode(false);
        let b = encode(true);
        if a != b {
            failures.push(name);
        }
        assert!(
            read_brewing_item(&a).is_some(),
            "{name} canonical roundtrip"
        );
        let component = indirect_entry(name);
        let mut ordinary = Vec::new();
        component
            .write_network(&indirect(name, custom_text(true)), &mut ordinary)
            .expect("ordinary text");
        let component_name = match name {
            "painting/author" => "painting/variant",
            "trim/material" => "trim",
            _ => name,
        };
        assert!(
            read_brewing_item(&envelope(component_name, &ordinary)).is_none(),
            "{name}: reject noncanonical custom NBT"
        );
    }
    assert!(
        failures.is_empty(),
        "noncanonical indirect text: {failures:?}"
    );
}

#[test]
fn brewing_union_indirect_text_tiny_cap_bounds_classification() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for name in INDIRECT {
        let mut text = TextComponent::plain("");
        text.children = vec![TextComponent::plain(""); 500_000];
        let value = indirect(name, text);
        let stats = allocation_counter::measure(|| {
            let classifications = child_classification_work::measure(|| {
                assert!(
                    indirect_entry(name)
                        .write_network_bounded(&value, 96, &mut sink())
                        .is_err()
                );
            });
            if classifications != 0 {
                failures.push((name, classifications));
            }
        });
        eprintln!("union indirect {name}: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            failures.push((name, usize::MAX));
        }
    }
    assert!(
        failures.is_empty(),
        "full classification under tiny cap: {failures:?}"
    );
}

fn unknown_lock(length: usize) -> LockCode {
    let ty = Box::leak(Box::new(DataComponentPredicateType::of::<DamagePredicate>(
        Identifier::new("test", "x".repeat(length)),
    )));
    let matchers = DataComponentMatchers::new(
        DataComponentExactPredicate::EMPTY,
        vec![DataComponentPredicateData::new(
            ty,
            DamagePredicate::new(IntBounds::ANY, IntBounds::ANY),
        )],
    )
    .expect("matchers");
    LockCode::new(ItemPredicate::new(None, IntBounds::ANY, matchers))
}

#[test]
fn brewing_union_snapshot_lock_unknown_discriminator_allocation() {
    init_vanilla_registry();
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.lock = unknown_lock((2 << 20) + 1);
    let stats = allocation_counter::measure(|| assert!(encode_brewing_snapshot(&value).is_none()));
    eprintln!("union snapshot discriminator: {stats:?}");
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn brewing_union_snapshot_lock_unknown_discriminator_rejects() {
    init_vanilla_registry();
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.lock = unknown_lock(3);
    assert!(
        encode_brewing_snapshot(&value).is_none(),
        "must not emit an unreadable persistent discriminator"
    );
}

#[test]
fn brewing_union_nested_transient_exact_matches_persistent_oracle() {
    use foton_registry::data_components::vanilla_components::CREATIVE_SLOT_LOCK;
    init_vanilla_registry();
    let inner = AdventureModePredicate::new(vec![BlockPredicate::new(
        None,
        None,
        None,
        matchers(vec![(entry("creative_slot_lock"), ComponentData::new(()))]),
    )])
    .expect("inner");
    let data = ComponentData::new(inner.clone());
    assert!(
        entry("can_break")
            .validate_persistent_encoding(&data)
            .is_ok()
    );
    for outer in ["can_break", "can_place_on"] {
        let value = AdventureModePredicate::new(vec![BlockPredicate::new(
            None,
            None,
            None,
            matchers(vec![(entry("can_break"), data.clone())]),
        )])
        .expect("outer");
        for count in [0, 1] {
            let mut patch = DataComponentPatch::new();
            patch.set(CREATIVE_SLOT_LOCK, ());
            assert!(patch.set_raw(entry(outer).key.clone(), ComponentData::new(value.clone())));
            let item = ItemStack::from_raw_parts(&vanilla_items::STONE, count, patch);
            let bytes = encode_brewing_item(&item).expect("network encoding");
            assert_eq!(
                read_brewing_item(&bytes),
                Some(item),
                "{outer} count {count}"
            );
        }
    }
    let lock = lock(vec![
        (entry("creative_slot_lock"), ComponentData::new(())),
        (entry("can_break"), data),
    ]);
    let mut bytes = Vec::new();
    lock.write_bounded(65536, &mut bytes)
        .expect("transient exact entries are omitted");
    assert!(LockCode::read(&mut Cursor::new(bytes.as_slice())).is_ok());
}

#[test]
fn brewing_union_tool_rejects_before_full_rule_validation() {
    init_vanilla_registry();
    let rule = ToolRule::override_speed(RegistryHolderSet::Direct(vec![]), 1.0);
    let mut tool = Tool {
        rules: vec![rule; 500_000],
        ..Tool::default()
    };
    let stats = allocation_counter::measure(|| {
        assert!(nbt_encode::write_bounded(&tool, 32, &mut sink()).is_err());
    });
    eprintln!("union tool preflight: {stats:?}");
    tool.rules.last_mut().expect("last rule").speed = Some(-1.0);
    let error = nbt_encode::write_bounded(&tool, 32, &mut sink()).expect_err("cannot fit");
    assert!(
        error.to_string().contains("budget"),
        "classified late rule before size rejection: {error}"
    );
    assert!(stats.bytes_max < 4096, "{stats:?}");
    tool.rules.last_mut().expect("last rule").speed = Some(1.0);
    let value = lock(vec![(entry("tool"), ComponentData::new(tool))]);
    let stats = allocation_counter::measure(|| {
        assert!(value.write_bounded(96, &mut sink()).is_err());
    });
    eprintln!("union tool exact lock: {stats:?}");
    assert!(stats.bytes_max < 4096, "{stats:?}");
}

#[test]
fn brewing_union_trim_preflight_bounds_sorting() {
    use foton_registry::data_components::components::ProvidesTrimMaterial;
    use foton_registry::trim_material::{MaterialAssetGroup, MaterialAssetInfo, TrimMaterialValue};
    init_vanilla_registry();
    let map = (0..12_000)
        .map(|i| {
            (
                Identifier::new("test", format!("{}_{i:05}", "x".repeat(1024))),
                MaterialAssetInfo::new("a").expect("asset"),
            )
        })
        .collect();
    let material = TrimMaterialValue::new(
        MaterialAssetGroup::new(MaterialAssetInfo::new("a").expect("base"), map),
        TextComponent::plain(""),
    );
    let value = ComponentData::new(ProvidesTrimMaterial::new(RegistryHolder::direct(
        material.clone(),
    )));
    let mut network_visits = 0;
    let mut persistent_visits = 0;
    let stats = allocation_counter::measure(|| {
        network_visits = canonical_override_work::measure(|| {
            assert!(
                entry("provides_trim_material")
                    .write_network_bounded(&value, 64, &mut sink())
                    .is_err()
            );
        });
        persistent_visits = canonical_override_work::measure(|| {
            assert!(nbt_encode::write_bounded(&material, 64, &mut sink()).is_err());
        });
    });
    eprintln!(
        "union trim tiny cap: network visits={network_visits}, persistent visits={persistent_visits}, {stats:?}"
    );
    assert_eq!(network_visits, 0, "network route scanned impossible map");
    assert_eq!(
        persistent_visits, 0,
        "persistent route scanned impossible map"
    );
    assert!(
        stats.bytes_max < 4096,
        "sorted an impossible map: {stats:?}"
    );
}

#[test]
fn brewing_union_recursive_exact_validation_is_linear() {
    init_vanilla_registry();
    let encoded = |depth| {
        let mut body = Vec::new();
        for _ in 0..depth {
            for n in [
                vanilla_items::STONE.id() as i32,
                1,
                1,
                0,
                entry("use_remainder").id() as i32,
            ] {
                VarInt(n).write(&mut body).expect("template");
            }
        }
        for n in [vanilla_items::STONE.id() as i32, 1, 0, 0] {
            VarInt(n).write(&mut body).expect("leaf");
        }
        let mut exact = vec![1, 0, 0, 0, 1];
        VarInt(entry("use_remainder").id() as i32)
            .write(&mut exact)
            .expect("entry");
        exact.extend(body);
        exact.push(0);
        envelope("can_break", &exact)
    };
    let small = encoded(8);
    let large = encoded(48);
    let measure =
        |bytes: &[u8]| allocation_counter::measure(|| assert!(read_brewing_item(bytes).is_some()));
    let small_stats = measure(&small);
    let large_stats = measure(&large);
    eprintln!("union recursive validation: small={small_stats:?} large={large_stats:?}");
    assert!(
        large_stats.count_total < small_stats.count_total * 9,
        "suffixes serialized repeatedly: small={small_stats:?}, large={large_stats:?}"
    );
}

#[test]
fn brewing_union_nested_exact_validation_is_linear() {
    init_vanilla_registry();
    let encoded = |depth| {
        let mut body = vec![1, 0, 0, 0, 1];
        VarInt(entry("damage").id() as i32)
            .write(&mut body)
            .expect("damage");
        body.extend([1, 0]);
        for _ in 0..depth {
            let mut parent = vec![1, 0, 0, 0, 1];
            VarInt(entry("can_break").id() as i32)
                .write(&mut parent)
                .expect("adventure");
            parent.extend(body);
            parent.push(0);
            body = parent;
        }
        envelope("can_break", &body)
    };
    let small = encoded(8);
    let large = encoded(48);
    let measure =
        |bytes: &[u8]| allocation_counter::measure(|| assert!(read_brewing_item(bytes).is_some()));
    let a = measure(&small);
    let b = measure(&large);
    eprintln!("union nested exact validation: small={a:?} large={b:?}");
    assert!(
        b.count_total < a.count_total * 9,
        "revalidated nested exact suffixes: {a:?} {b:?}"
    );
}

#[test]
fn brewing_union_any_discriminator_persistence_matches_registered_reader() {
    init_vanilla_registry();
    let mut mismatches = Vec::new();
    for component in
        (0..REGISTRY.data_components.len()).filter_map(|id| REGISTRY.data_components.by_id(id))
    {
        let matchers = DataComponentMatchers::new(
            DataComponentExactPredicate::EMPTY,
            vec![DataComponentPredicateData::any(component)],
        )
        .expect("matchers");
        let value = LockCode::new(ItemPredicate::new(None, IntBounds::ANY, matchers));
        let mut ordinary = Vec::new();
        value.write(&mut ordinary).expect("ordinary lock");
        let expected = LockCode::read(&mut Cursor::new(ordinary.as_slice())).is_ok();
        let mut bounded = Vec::new();
        let actual = value.write_bounded(65536, &mut bounded).is_ok();
        if actual != expected {
            mismatches.push(component.key.to_string());
        }
        if actual {
            assert!(LockCode::read(&mut Cursor::new(bounded.as_slice())).is_ok());
        }
    }
    assert!(
        mismatches.is_empty(),
        "discriminator oracle mismatch: {mismatches:?}"
    );
}

#[test]
fn brewing_union_indirect_text_in_recursive_templates_and_exact_locks() {
    init_vanilla_registry();
    for name in INDIRECT {
        let encode = |reverse| {
            let component = indirect_entry(name);
            let value = indirect(name, custom_text(reverse));
            let mut template = Vec::new();
            for n in [
                vanilla_items::STONE.id() as i32,
                1,
                1,
                0,
                component.id() as i32,
            ] {
                VarInt(n).write(&mut template).expect("template");
            }
            component
                .write_network(&value, &mut template)
                .expect("ordinary nested value");
            let nested = entry("use_remainder")
                .read_network(&mut Cursor::new(&template))
                .expect("network template");
            let mut patch = DataComponentPatch::new();
            assert!(patch.set_raw(entry("use_remainder").key.clone(), nested));
            let bytes =
                encode_brewing_item(&ItemStack::from_raw_parts(&vanilla_items::STONE, 1, patch))
                    .expect("recursive encoding");
            let mut lock_bytes = Vec::new();
            if component.validate_persistent_encoding(&value).is_ok() {
                lock(vec![(component, value)])
                    .write_bounded(65536, &mut lock_bytes)
                    .expect("exact lock");
                assert!(LockCode::read(&mut Cursor::new(lock_bytes.as_slice())).is_ok());
            }
            (bytes, lock_bytes)
        };
        let first = encode(false);
        assert_eq!(first, encode(true), "{name}: recursive canonical text");
        assert!(
            read_brewing_item(&first.0).is_some(),
            "{name}: recursive roundtrip"
        );
    }
}

#[test]
fn brewing_union_nested_decode_failure_restores_validation() {
    init_vanilla_registry();
    let mut invalid = vec![1, 0, 0, 0, 1];
    VarInt(entry("ominous_bottle_amplifier").id() as i32)
        .write(&mut invalid)
        .expect("entry");
    invalid.extend([5, 0]);
    let invalid = envelope("can_break", &invalid);
    let mut truncated = vec![1, 0, 0, 0, 1];
    VarInt(entry("can_break").id() as i32)
        .write(&mut truncated)
        .expect("entry");
    truncated.extend([1, 0, 0, 0, 1]);
    VarInt(entry("use_remainder").id() as i32)
        .write(&mut truncated)
        .expect("entry");
    let truncated = envelope("can_break", &truncated);
    for _ in 0..3 {
        assert!(read_brewing_item(&truncated).is_none());
        assert!(
            read_brewing_item(&invalid).is_none(),
            "a failed child must not defer later validation"
        );
    }
}
