use super::*;
use foton_registry::data_components::components::{Filterable, ItemLore, WrittenBookContent};
use foton_registry::data_components::vanilla_components::{CUSTOM_DATA, ENTITY_DATA, PROFILE};
use foton_registry::resolvable_profile::{
    PartialProfile, PlayerSkinPatch, ProfileProperty, ResolvableProfile,
};
use foton_utils::hash::HashComponent;
use foton_utils::{nbt::normalization_work, serial::text_stream::child_classification_work};
use std::{io, thread::Builder, time::Instant};

fn exact_bytes(component: &'static str, name: &'static str, body: &[u8]) -> Vec<u8> {
    let mut adventure = vec![1, 0, 0, 0, 1];
    VarInt(entry(name).id() as i32)
        .write(&mut adventure)
        .expect("id");
    adventure.extend_from_slice(body);
    adventure.push(0);
    envelope(component, &adventure)
}
fn oracle(name: &'static str, data: &ComponentData) -> bool {
    entry(name)
        .write_nbt(data)
        .is_ok_and(|tag| entry(name).read_nbt_owned(&tag).is_some())
}
fn book(raw: TextComponent, filtered: Option<TextComponent>) -> WrittenBookContent {
    WrittenBookContent::new(
        Filterable::pass_through("title".into()),
        "author".into(),
        0,
        vec![Filterable::new(raw, filtered)],
        false,
    )
    .expect("network book")
}

#[test]
fn brewing_rereview_written_book_exact_json_utf16_boundaries() {
    init_vanilla_registry();
    // JSON overhead is measured independently so escaping and non-BMP units are covered.
    for seed in ["a", "😀", "\n", "\"", "\\", "\0"] {
        let overhead = serde_json::to_string(&TextComponent::plain(""))
            .expect("JSON")
            .encode_utf16()
            .count();
        let unit = serde_json::to_string(&TextComponent::plain(seed))
            .expect("JSON")
            .encode_utf16()
            .count()
            - overhead;
        let count = (WrittenBookContent::PAGE_LENGTH - overhead) / unit;
        let padding = WrittenBookContent::PAGE_LENGTH - overhead - unit;
        let pages = [
            seed.repeat(count),
            seed.repeat(count + 1),
            "a".repeat(32_768),
            format!("{seed}{}", "a".repeat(padding)),
            format!("{seed}{}", "a".repeat(padding + 1)),
        ];
        for (case, content) in pages.into_iter().enumerate() {
            for filtered in [false, true] {
                let page = TextComponent::plain(content.clone());
                // Non-ASCII cases above the NBT string ceiling are not network-reachable.
                let value = if filtered {
                    book(TextComponent::plain("ok"), Some(page))
                } else {
                    book(page, None)
                };
                let data = ComponentData::new(value.clone());
                let expected = oracle("written_book_content", &data);
                let mut body = Vec::new();
                if value.write(&mut body).is_err() {
                    continue;
                }
                for component in ["can_break", "can_place_on"] {
                    let decoded =
                        read_brewing_item(&exact_bytes(component, "written_book_content", &body));
                    assert_eq!(
                        decoded.is_some(),
                        expected,
                        "seed={seed:?} case={case} filtered={filtered} {component}"
                    );
                    if let Some(item) = decoded {
                        // Hashing the accepted exact predicate must never hit its invariant panic.
                        if component == "can_break" {
                            item.get(CAN_BREAK).expect("predicate").compute_hash();
                        } else {
                            item.get(CAN_PLACE_ON).expect("predicate").compute_hash();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn brewing_rereview_potion_hidden_depth_matches_persistent_reader() {
    Builder::new()
        .stack_size(32 << 20)
        .spawn(|| {
            init_vanilla_registry();
            for hidden in [508, 509, 510, 511] {
                // Potion flags, one effect, registered effect ID; each details node is six bytes.
                let mut body = vec![0, 0, 1];
                VarInt(REGISTRY.mob_effects.iter().next().expect("effect").1.id() as i32)
                    .write(&mut body)
                    .expect("effect id");
                for level in 0..=hidden {
                    body.extend_from_slice(&[0, 0, 0, 1, 1, u8::from(level < hidden)]);
                }
                body.push(0); // absent custom name
                let data = entry("potion_contents")
                    .read_network(&mut Cursor::new(&body))
                    .expect("network potion");
                let expected = oracle("potion_contents", &data);
                eprintln!("potion hidden={hidden}: persistent={expected}");
                for component in ["can_break", "can_place_on"] {
                    let actual =
                        read_brewing_item(&exact_bytes(component, "potion_contents", &body));
                    assert_eq!(actual.is_some(), expected, "{component} hidden={hidden}");
                }
            }
        })
        .expect("worker")
        .join()
        .expect("depth checks");
}

fn nested_compound(reverse: bool) -> NbtCompound {
    let mut inner = NbtCompound::new();
    for (k, v) in if reverse {
        [("b", 2), ("a", 1)]
    } else {
        [("a", 1), ("b", 2)]
    } {
        inner.insert(k, v);
    }
    let mut outer = NbtCompound::new();
    if reverse {
        outer.insert("z", 3);
    }
    outer.insert("nested", NbtList::Compound(vec![inner]));
    if !reverse {
        outer.insert("z", 3);
    }
    outer
}

#[test]
fn brewing_rereview_compounds_are_recursively_canonical() {
    init_vanilla_registry();
    let encode = |reverse| {
        let custom = CustomData::try_from_compound(nested_compound(reverse)).expect("custom");
        let pig = REGISTRY
            .entity_types
            .by_key(&Identifier::vanilla_static("pig"))
            .expect("pig");
        let entity = EntityData::new(pig, custom.clone());
        let mut item = ItemStack::new(&vanilla_items::STONE);
        item.set(CUSTOM_DATA, custom.clone());
        item.set(ENTITY_DATA, entity.clone());
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = lock(vec![
            (entry("custom_data"), ComponentData::new(custom)),
            (entry("entity_data"), ComponentData::new(entity)),
        ]);
        (
            encode_brewing_item(&item).expect("item"),
            encode_brewing_snapshot(&value).expect("lock"),
        )
    };
    let canonical = encode(false);
    assert_eq!(canonical, encode(true), "recursive unordered compounds");
    assert!(read_brewing_item(&canonical.0).is_some());
    assert!(decode_brewing_snapshot(&canonical.1).is_some());
    for name in [
        "custom_data",
        "bucket_entity_data",
        "entity_data",
        "block_entity_data",
    ] {
        let mut tag = nested_compound(true);
        if name == "entity_data" {
            tag.insert("id", "minecraft:pig");
        }
        if name == "block_entity_data" {
            tag.insert("id", "minecraft:chest");
        }
        let persistent_tag = NbtTag::Compound(tag);
        let mut body = Vec::new();
        if name == "entity_data" {
            VarInt(
                REGISTRY
                    .entity_types
                    .by_key(&Identifier::vanilla_static("pig"))
                    .expect("pig")
                    .id() as i32,
            )
            .write(&mut body)
            .expect("id");
        }
        if name == "block_entity_data" {
            VarInt(
                REGISTRY
                    .block_entity_types
                    .by_key(&Identifier::vanilla_static("chest"))
                    .expect("chest")
                    .id() as i32,
            )
            .write(&mut body)
            .expect("id");
        }
        NbtTag::Compound(nested_compound(true)).write(&mut body);
        assert!(
            read_brewing_item(&envelope(name, &body)).is_none(),
            "permuted {name}"
        );
        for component in ["can_break", "can_place_on"] {
            assert!(
                read_brewing_item(&exact_bytes(component, name, &body)).is_none(),
                "exact permuted {name}"
            );
        }
        let mut components = NbtCompound::new();
        components.insert(entry(name).key.to_string(), persistent_tag);
        let mut predicate = NbtCompound::new();
        predicate.insert("components", components);
        assert!(
            decode_brewing_snapshot(&snapshot_with_lock_tag(NbtTag::Compound(predicate))).is_none(),
            "permuted lock {name}"
        );
    }
}

fn profile(reverse: bool, swap_within_group: bool) -> ResolvableProfile {
    let a1 = ProfileProperty::new("a".into(), "A1".into(), None).expect("property");
    let a2 = ProfileProperty::new("a".into(), "A2".into(), None).expect("property");
    let b = ProfileProperty::new("b".into(), "B".into(), None).expect("property");
    let mut properties = if swap_within_group {
        vec![a2, a1]
    } else {
        vec![a1, a2]
    };
    if reverse {
        properties.insert(0, b);
    } else {
        properties.push(b);
    }
    ResolvableProfile::static_partial(
        PartialProfile::new(None, None, properties).expect("partial"),
        PlayerSkinPatch::default(),
    )
}
#[test]
fn brewing_rereview_profile_groups_are_canonical_but_values_are_ordered() {
    init_vanilla_registry();
    let encode = |reverse, swap| {
        let profile = profile(reverse, swap);
        let mut item = ItemStack::new(&vanilla_items::STONE);
        item.set(PROFILE, profile.clone());
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = lock(vec![(entry("profile"), ComponentData::new(profile))]);
        (
            encode_brewing_item(&item).expect("item"),
            encode_brewing_snapshot(&value).expect("snapshot"),
        )
    };
    assert_eq!(profile(false, false), profile(true, false));
    let canonical = encode(false, false);
    assert_eq!(canonical, encode(true, false), "profile group order");
    assert_ne!(
        canonical,
        encode(false, true),
        "same-name order must remain significant"
    );
    assert!(read_brewing_item(&canonical.0).is_some());
    assert!(decode_brewing_snapshot(&canonical.1).is_some());
    let mut body = Vec::new();
    profile(true, false)
        .write(&mut body)
        .expect("ordinary profile");
    assert!(
        read_brewing_item(&envelope("profile", &body)).is_none(),
        "permuted network groups"
    );
    let mut components = NbtCompound::new();
    components.insert(
        "minecraft:profile",
        entry("profile")
            .write_nbt(&ComponentData::new(profile(true, false)))
            .expect("profile tag"),
    );
    let mut predicate = NbtCompound::new();
    predicate.insert("components", components);
    assert!(
        decode_brewing_snapshot(&snapshot_with_lock_tag(NbtTag::Compound(predicate))).is_none(),
        "permuted persistent groups"
    );
}

#[test]
fn brewing_rereview_nested_preflights_stop_before_malicious_suffix() {
    init_vanilla_registry();
    let mut late = TextComponent::plain("");
    late.format.font = Some("x".repeat(65536).into());
    let lore =
        ItemLore::new(vec![TextComponent::plain("x".repeat(26)), late.clone()]).expect("lore");
    let pages = WrittenBookContent::new(
        Filterable::pass_through(String::new()),
        String::new(),
        0,
        vec![Filterable::new(
            TextComponent::plain("x".repeat(20)),
            Some(late.clone()),
        )],
        false,
    )
    .expect("book");
    for (name, value) in [
        ("lore", ComponentData::new(lore)),
        ("written_book_content", ComponentData::new(pages)),
        ("custom_name", ComponentData::new(late)),
    ] {
        let cap = if name == "custom_name" { 16 } else { 32 };
        let error = entry(name)
            .write_network_bounded(&value, cap, &mut io::sink())
            .expect_err("budget");
        assert_eq!(
            error.to_string(),
            "serialization budget exceeded",
            "{name} scanned suffix"
        );
    }
}

#[test]
fn brewing_rereview_snbt_preflight_respects_remaining_allowance() {
    init_vanilla_registry();
    let mut compound = NbtCompound::new();
    compound.insert("a", NbtTag::ByteArray(vec![0; 100]));
    compound.insert("z", "x".repeat(30000));
    let adventure = AdventureModePredicate::new(vec![BlockPredicate::new(
        None,
        None,
        Some(NbtPredicate::new(compound).expect("predicate")),
        DataComponentMatchers::ANY,
    )])
    .expect("adventure");
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.lock = lock(vec![(entry("can_break"), ComponentData::new(adventure))]);
    let stats = allocation_counter::measure(|| {
        assert!(value.lock.write_bounded(80, &mut io::sink()).is_err());
    });
    eprintln!("SNBT tiny cap scratch: {stats:?}");
    assert!(
        stats.bytes_total < 8192,
        "SNBT scanned/decoded late suffix: {stats:?}"
    );
}

#[test]
fn brewing_rereview_text_list_work_is_bounded_by_remaining_output() {
    init_vanilla_registry();
    let mut text = TextComponent::plain("");
    text.children = vec![TextComponent::plain(""); 500_000];
    let value = ComponentData::new(text);
    let classifications = child_classification_work::measure(|| {
        assert!(
            entry("custom_name")
                .write_network_bounded(&value, 32, &mut io::sink())
                .is_err()
        );
    });
    assert_eq!(
        classifications, 0,
        "tiny cap classified attacker-sized child list"
    );
}

#[test]
fn brewing_rereview_flat_compound_normalization_work() {
    init_vanilla_registry();
    for count in [50usize, 500] {
        let mut compound = NbtCompound::new();
        for i in 0..count {
            compound.insert(format!("k{i:05}"), 1_i8);
        }
        let mut body = Vec::new();
        NbtTag::Compound(compound).write(&mut body);
        let bytes = envelope("custom_data", &body);
        let start = Instant::now();
        let comparisons = normalization_work::measure(|| {
            assert!(read_brewing_item(&bytes).is_some());
        });
        eprintln!(
            "task14 flat Brew normalization count={count} comparisons={comparisons} elapsed={:?}",
            start.elapsed()
        );
        assert!(comparisons > count, "normalization route must be observed");
        assert!(
            comparisons < 3 * count * count.ilog2() as usize,
            "quadratic normalization: {comparisons}"
        );
    }
}

#[test]
fn brewing_rereview_nested_compounds_from_text_and_merged_custom_data() {
    use foton_registry::data_components::vanilla_components::CUSTOM_NAME;
    use text_components::{
        custom::{CustomData as TextData, Payload},
        interactivity::ClickEvent,
    };
    init_vanilla_registry();
    let encode = |reverse| {
        let mut custom = CustomData::default();
        for (key, value) in nested_compound(reverse) {
            let mut piece = NbtCompound::new();
            piece.insert(key, value);
            custom = custom.merged_with(&CustomData::try_from_compound(piece).expect("piece"));
        }
        let mut text = TextComponent::plain("hover");
        text.interactions.click = Some(ClickEvent::Custom(TextData {
            id: "test:payload".into(),
            payload: Payload::Nbt(NbtTag::Compound(nested_compound(reverse)).into()),
        }));
        let mut item = ItemStack::new(&vanilla_items::STONE);
        item.set(CUSTOM_DATA, custom.clone());
        item.set(CUSTOM_NAME, text.clone());
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = lock(vec![
            (entry("custom_data"), ComponentData::new(custom)),
            (entry("custom_name"), ComponentData::new(text)),
        ]);
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

#[test]
fn brewing_rereview_normalization_keeps_last_duplicate_and_rejects_invalid_overwritten_data() {
    init_vanilla_registry();
    for invalid in [false, true] {
        let mut compound = NbtCompound::new();
        if invalid {
            compound.insert(
                "value",
                NbtTag::String(simdnbt::Mutf8Str::from_slice(&[0xff]).to_owned()),
            );
        } else {
            compound.insert("value", 1);
        }
        compound.insert("value", 2);
        let mut body = Vec::new();
        NbtTag::Compound(compound).write(&mut body);
        let result = entry("custom_data").read_network(&mut Cursor::new(&body));
        assert_eq!(result.is_ok(), !invalid);
        if let Ok(data) = result {
            assert_eq!(
                data.downcast_ref::<CustomData>()
                    .expect("custom")
                    .as_compound()
                    .int("value"),
                Some(2)
            );
        }
        assert!(read_brewing_item(&envelope("custom_data", &body)).is_none());
    }
}
