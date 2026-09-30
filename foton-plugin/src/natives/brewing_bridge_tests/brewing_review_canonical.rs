use super::*;

#[test]
fn brewing_review_exact_matchers_are_canonical_in_both_formats() {
    init_vanilla_registry();
    let values = |reverse| {
        let mut values = vec![
            (entry("damage"), ComponentData::new(3_i32)),
            (entry("repair_cost"), ComponentData::new(2_i32)),
        ];
        if reverse {
            values.reverse();
        }
        values
    };
    for component in [CAN_BREAK, CAN_PLACE_ON] {
        let encode = |reverse| {
            let mut item = ItemStack::new(&vanilla_items::STONE);
            item.set(
                component.clone(),
                AdventureModePredicate::new(vec![BlockPredicate::new(
                    None,
                    None,
                    None,
                    matchers(values(reverse)),
                )])
                .expect("adventure"),
            );
            encode_brewing_item(&item).expect("item")
        };
        let canonical = encode(false);
        assert_eq!(canonical, encode(true), "exact network order");
        assert!(read_brewing_item(&canonical).is_some());
        // Encode an explicitly reversed wire matcher with the ordinary codec.
        let adventure = AdventureModePredicate::new(vec![BlockPredicate::new(
            None,
            None,
            None,
            matchers(values(true)),
        )])
        .expect("adventure");
        let mut body = Vec::new();
        adventure.write(&mut body).expect("ordinary");
        assert!(
            read_brewing_item(&envelope(
                if component.key() == CAN_BREAK.key() {
                    "can_break"
                } else {
                    "can_place_on"
                },
                &body
            ))
            .is_none()
        );
    }
    let encode = |reverse| {
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = lock(values(reverse));
        encode_brewing_snapshot(&value).expect("snapshot")
    };
    assert_eq!(encode(false), encode(true), "exact persistent order");
    let mut fields = NbtCompound::new();
    fields.insert("minecraft:repair_cost", 2);
    fields.insert("minecraft:damage", 3);
    let mut predicate = NbtCompound::new();
    predicate.insert("components", fields);
    assert!(
        decode_brewing_snapshot(&snapshot_with_lock_tag(NbtTag::Compound(predicate))).is_none()
    );
}

#[test]
fn brewing_review_enchantment_maps_ignore_capacity_and_insertion_history() {
    init_vanilla_registry();
    let make = |capacity, reverse| {
        let mut value = ItemEnchantments::empty();
        value.levels.reserve(capacity);
        let mut keys: Vec<_> = REGISTRY.enchantments.iter().take(15).collect();
        if reverse {
            keys.reverse();
        }
        for enchantment in keys {
            value.set(enchantment.1.key.clone(), 1);
        }
        value
    };
    let encode = |value: ItemEnchantments| {
        let mut item = ItemStack::new(&vanilla_items::STONE);
        item.set(ENCHANTMENTS, value.clone());
        let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
        snapshot.lock = lock(vec![(entry("enchantments"), ComponentData::new(value))]);
        (
            encode_brewing_item(&item).expect("item"),
            encode_brewing_snapshot(&snapshot).expect("snapshot"),
        )
    };
    let canonical = encode(make(0, false));
    for capacity in [0, 64, 512, 4096] {
        assert_eq!(
            canonical,
            encode(make(capacity, true)),
            "capacity {capacity}"
        );
    }
    assert!(read_brewing_item(&canonical.0).is_some());
    assert!(decode_brewing_snapshot(&canonical.1).is_some());
}

#[test]
fn brewing_review_snapshot_names_require_canonical_bytes() {
    init_vanilla_registry();
    for bold in [None, Some(2_i8)] {
        let mut name = NbtCompound::new();
        name.insert("text", "x");
        if let Some(bold) = bold {
            name.insert("bold", bold);
        }
        let mut tag = Vec::new();
        NbtTag::Compound(name).write(&mut tag);
        let mut bytes =
            encode_brewing_snapshot(&snapshot(vec![ItemStack::empty(); 5])).expect("snapshot");
        let offset = custom_name_length_offset(&bytes);
        replace_snapshot_blob(&mut bytes, offset, &tag);
        assert!(
            decode_brewing_snapshot(&bytes).is_none(),
            "noncanonical name {bold:?}"
        );
    }
}

#[test]
fn brewing_review_trim_override_maps_are_canonical() {
    use foton_registry::data_components::components::{ArmorTrim, ProvidesTrimMaterial};
    use foton_registry::data_components::vanilla_components::{PROVIDES_TRIM_MATERIAL, TRIM};
    use foton_registry::trim_material::{MaterialAssetGroup, MaterialAssetInfo, TrimMaterialValue};
    use foton_registry::{RegistryHolder, vanilla_trim_patterns};
    init_vanilla_registry();
    let encode = |capacity, reverse| {
        let mut overrides = rustc_hash::FxHashMap::default();
        overrides.reserve(capacity);
        let mut keys: Vec<_> = (0..15).collect();
        if reverse {
            keys.reverse();
        }
        for i in keys {
            overrides.insert(
                Identifier::new("test", format!("asset_{i}")),
                MaterialAssetInfo::new(format!("suffix_{i}")).expect("asset"),
            );
        }
        let material = TrimMaterialValue::new(
            MaterialAssetGroup::new(
                MaterialAssetInfo::new("base".to_owned()).expect("base"),
                overrides,
            ),
            TextComponent::plain("trim"),
        );
        let mut item = ItemStack::new(&vanilla_items::STONE);
        item.set(
            TRIM,
            ArmorTrim::new(
                RegistryHolder::Direct(material.clone()),
                RegistryHolder::Reference(&vanilla_trim_patterns::SENTRY),
            ),
        );
        item.set(
            PROVIDES_TRIM_MATERIAL,
            ProvidesTrimMaterial::new(RegistryHolder::Direct(material.clone())),
        );
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = lock(vec![(
            entry("provides_trim_material"),
            ComponentData::new(ProvidesTrimMaterial::new(RegistryHolder::Direct(material))),
        )]);
        (
            encode_brewing_item(&item).expect("item"),
            encode_brewing_snapshot(&value).expect("snapshot"),
        )
    };
    let canonical = encode(0, false);
    for capacity in [0, 64, 512] {
        assert_eq!(canonical, encode(capacity, true));
    }
    assert!(read_brewing_item(&canonical.0).is_some());
    assert!(decode_brewing_snapshot(&canonical.1).is_some());
}

#[test]
fn brewing_review_partial_predicate_maps_are_canonical() {
    use foton_registry::data_component_predicate::DataComponentPredicateData;
    init_vanilla_registry();
    let make = |reverse| {
        let mut partial = vec![
            DataComponentPredicateData::any(entry("damage")),
            DataComponentPredicateData::any(entry("repair_cost")),
        ];
        if reverse {
            partial.reverse();
        }
        DataComponentMatchers::new(DataComponentExactPredicate::EMPTY, partial).expect("partial")
    };
    let encode = |reverse| {
        let mut item = ItemStack::new(&vanilla_items::STONE);
        item.set(
            CAN_BREAK,
            AdventureModePredicate::new(vec![BlockPredicate::new(None, None, None, make(reverse))])
                .expect("adventure"),
        );
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = LockCode::new(ItemPredicate::new(None, IntBounds::ANY, make(reverse)));
        (
            encode_brewing_item(&item).expect("item"),
            encode_brewing_snapshot(&value).expect("snapshot"),
        )
    };
    let canonical = encode(false);
    assert_eq!(canonical, encode(true));
    assert!(read_brewing_item(&canonical.0).is_some());
    assert!(decode_brewing_snapshot(&canonical.1).is_some());
}
