use super::*;
use foton_utils::nbt::parse_snbt;
use std::collections::BTreeSet;

fn persistent_accepts(entry: ComponentEntryRef, data: &ComponentData) -> bool {
    // Use the registered writer/reader directly: the oracle must not share the
    // source validator under test.
    entry
        .write_nbt(data)
        .is_ok_and(|tag| entry.read_nbt_owned(&tag).is_some())
}

fn accepts_valid_exact(entry: ComponentEntryRef, data: ComponentData) -> bool {
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(
        CAN_BREAK,
        AdventureModePredicate::new(vec![BlockPredicate::new(
            None,
            None,
            None,
            matchers(vec![(entry, data)]),
        )])
        .expect("adventure"),
    );
    let bytes = encode_brewing_item(&item).expect("bounded output");
    read_brewing_item(&bytes).is_some()
}

#[test]
fn brewing_review_invalid_persistent_values_cannot_enter_exact_predicates() {
    init_vanilla_registry();
    let enchantment = REGISTRY
        .enchantments
        .iter()
        .next()
        .expect("enchantment")
        .1
        .id() as i32;
    let mut zero_level = Vec::new();
    for n in [1, enchantment, 0] {
        VarInt(n).write(&mut zero_level).expect("level");
    }
    for (name, body, valid) in [
        ("ominous_bottle_amplifier", vec![5], false),
        ("ominous_bottle_amplifier", vec![4], true),
        ("enchantments", zero_level, false),
    ] {
        let data = entry(name)
            .read_network(&mut Cursor::new(body.as_slice()))
            .expect("network value");
        assert_eq!(
            persistent_accepts(entry(name), &data),
            valid,
            "persistent oracle"
        );
        for component in ["can_break", "can_place_on"] {
            let mut adventure = vec![1, 0, 0, 0, 1];
            VarInt(
                REGISTRY
                    .data_components
                    .id_from_key(&entry(name).key)
                    .expect("id") as i32,
            )
            .write(&mut adventure)
            .expect("id");
            adventure.extend_from_slice(&body);
            adventure.push(0);
            assert_eq!(
                read_brewing_item(&envelope(component, &adventure)).is_some(),
                valid,
                "{component}/{name}"
            );
        }
    }
}

#[test]
fn brewing_review_exact_validation_matches_persistent_codec_for_registered_defaults() {
    init_vanilla_registry();
    let mut covered = BTreeSet::new();
    let mut failures = Vec::new();
    for (_, item) in REGISTRY.items.iter() {
        for key in item.components.keys() {
            let entry = REGISTRY.data_components.by_key(key).expect("entry");
            if !entry.is_persistent() || covered.contains(&key.to_string()) {
                continue;
            }
            let data = item.components.get_raw(key).expect("value");
            if !persistent_accepts(entry, data) {
                continue;
            }
            if !accepts_valid_exact(entry, data.clone()) {
                failures.push(key.to_string());
            }
            covered.insert(key.to_string());
        }
    }
    let samples = [
        "{}",
        "[]",
        "0",
        "1",
        "true",
        "\"x\"",
        "\"minecraft:stone\"",
        "{title: {raw: \"title\"}, author: \"author\"}",
        "{id: \"minecraft:stone\"}",
        "{id: \"minecraft:chest\"}",
        "{loot_table: \"minecraft:empty\"}",
        "{material: \"minecraft:iron\", pattern: \"minecraft:sentry\"}",
    ]
    .map(|s| parse_snbt(s).expect("fixture NBT"));
    let mut missing = Vec::new();
    for component in
        (0..REGISTRY.data_components.len()).filter_map(|id| REGISTRY.data_components.by_id(id))
    {
        if !component.is_persistent() || covered.contains(&component.key.to_string()) {
            continue;
        }
        let from_tags = samples.iter().find_map(|tag| component.read_nbt_owned(tag));
        let value = from_tags.or_else(|| {
            [0, 1].into_iter().find_map(|first| {
                let mut bytes = vec![0; 256];
                bytes[0] = first;
                let data = component
                    .read_network(&mut Cursor::new(bytes.as_slice()))
                    .ok()?;
                if !persistent_accepts(component, &data) {
                    return None;
                }
                Some(data)
            })
        });
        let Some(data) = value else {
            missing.push(component.key.to_string());
            continue;
        };
        assert!(
            persistent_accepts(component, &data),
            "fixture oracle {}",
            component.key
        );
        if !accepts_valid_exact(component, data) {
            failures.push(component.key.to_string());
        }
        covered.insert(component.key.to_string());
    }
    eprintln!(
        "Persistent codecs covered: {}, missing: {missing:?}",
        covered.len()
    );
    assert!(
        missing.is_empty(),
        "missing persistent fixtures: {missing:?}"
    );
    assert!(
        failures.is_empty(),
        "valid persistent defaults rejected: {failures:?}"
    );
}

#[test]
fn brewing_review_exact_validation_matches_persistent_numeric_boundaries() {
    init_vanilla_registry();
    let mut cases = Vec::new();
    for value in [-1, 0, 1, 4, 5, 99, 100, i32::MAX] {
        for name in [
            "max_stack_size",
            "max_damage",
            "damage",
            "repair_cost",
            "ominous_bottle_amplifier",
        ] {
            let mut body = Vec::new();
            VarInt(value).write(&mut body).expect("integer");
            cases.push((name, body));
        }
        let mut body = vec![0];
        VarInt(value).write(&mut body).expect("duration");
        cases.push(("swing_animation", body));
        let mut body = vec![0];
        1_f32.write(&mut body).expect("speed");
        VarInt(value).write(&mut body).expect("damage");
        body.push(1);
        cases.push(("tool", body));
    }
    for value in [
        f32::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        f32::MIN_POSITIVE,
        1.0,
        2.0,
        65.0,
        f32::INFINITY,
        f32::NAN,
    ] {
        for name in [
            "minimum_attack_charge",
            "potion_duration_scale",
            "use_cooldown",
            "use_effects",
            "weapon",
            "attack_range",
            "tool",
        ] {
            let mut body = Vec::new();
            match name {
                "use_effects" => {
                    body.extend_from_slice(&[1, 1]);
                    value.write(&mut body).expect("speed");
                }
                "weapon" => {
                    body.push(1);
                    value.write(&mut body).expect("seconds");
                }
                "attack_range" => {
                    for _ in 0..6 {
                        value.write(&mut body).expect("range");
                    }
                }
                "tool" => {
                    body.extend_from_slice(&[1, 1, 1]);
                    value.write(&mut body).expect("speed");
                    body.push(0);
                    1_f32.write(&mut body).expect("default speed");
                    body.extend_from_slice(&[1, 1]);
                }
                _ => {
                    value.write(&mut body).expect("float");
                    if name == "use_cooldown" {
                        body.push(0);
                    }
                }
            }
            cases.push((name, body));
        }
    }
    let mut failures = Vec::new();
    for (name, body) in cases {
        let data = entry(name)
            .read_network(&mut Cursor::new(body.as_slice()))
            .expect("network accepts boundary");
        let expected = persistent_accepts(entry(name), &data);
        let mut adventure = vec![1, 0, 0, 0, 1];
        VarInt(entry(name).id() as i32)
            .write(&mut adventure)
            .expect("id");
        adventure.extend_from_slice(&body);
        adventure.push(0);
        let accepted = read_brewing_item(&envelope("can_break", &adventure)).is_some();
        if accepted != expected {
            failures.push((name, body, expected));
        }
    }
    assert!(
        failures.is_empty(),
        "persistent/network validation differs: {failures:?}"
    );
}
