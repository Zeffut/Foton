use super::*;
use foton_registry::codec_work;
use foton_registry::data_components::{DataComponentPatch, components::CustomModelData};
use foton_registry::item_predicate::{
    StatePropertiesPredicate, StatePropertyMatcher, StatePropertyValueMatcher,
};
use foton_registry::resolvable_profile::ResolvableProfile;
use foton_utils::serial::ReadFrom;
use simdnbt::borrow::read_tag;
use std::io::sink;

fn state_value(value: StatePropertyValueMatcher) -> AdventureModePredicate {
    AdventureModePredicate::new(vec![BlockPredicate::new(
        None,
        Some(
            StatePropertiesPredicate::new(vec![StatePropertyMatcher::new("x".into(), value)])
                .expect("state"),
        ),
        None,
        DataComponentMatchers::ANY,
    )])
    .expect("adventure")
}

#[test]
fn brewing_task14_registered_strings_reject_before_scanning() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for size in [32 * 1024 * 1024, 16_000] {
        for (name, id, variant) in [
            ("custom_model_data", 17, 0),
            ("can_break", 15, 1),
            ("can_place_on", 14, 1),
            ("can_break", 15, 2),
            ("can_place_on", 14, 3),
        ] {
            let text = "x".repeat(size);
            let data = if variant == 0 {
                ComponentData::new(CustomModelData::new(vec![], vec![], vec![text], vec![]))
            } else {
                ComponentData::new(state_value(match variant {
                    1 => StatePropertyValueMatcher::Exact(text),
                    2 => StatePropertyValueMatcher::Range {
                        min: Some(text),
                        max: None,
                    },
                    _ => StatePropertyValueMatcher::Range {
                        min: None,
                        max: Some(text),
                    },
                }))
            };
            assert_eq!(entry(name).id(), id);
            let mut patch = DataComponentPatch::new();
            assert!(patch.set_raw(entry(name).key.clone(), data.clone()));
            let item = ItemStack::from_raw_parts(&vanilla_items::STONE, 0, patch);
            for route in ["registered", "item"] {
                // The native item budget admits 16K; registered 16-byte allowance must reject it.
                if route == "item" && size == 16_000 {
                    continue;
                }
                let mut work = codec_work::Work::default();
                let stats = allocation_counter::measure(|| {
                    work = codec_work::measure(|| {
                        if route == "registered" {
                            assert!(
                                entry(name)
                                    .write_network_bounded(&data, 16, &mut sink())
                                    .is_err()
                            );
                        } else {
                            assert!(encode_brewing_item(&item).is_none());
                        }
                    });
                });
                assert!(
                    stats.bytes_total < 4096,
                    "rejected string allocated payload storage: {stats:?}"
                );
                eprintln!(
                    "task14 string {route} {name} variant={variant} size={size}: {work:?} {stats:?}"
                );
                if work.utf16 > usize::from(variant != 0) {
                    failures.push((name, variant, size, route, work.utf16));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "oversized preliminary scans: {failures:?}"
    );
}

fn profile_tag(map: bool, empty: bool) -> NbtTag {
    let mut profile = NbtCompound::new();
    if map {
        let mut groups = NbtCompound::new();
        groups.insert("a", NbtList::String(vec!["a".into()]));
        groups.insert("b", NbtList::String(vec!["x".repeat(1024).into(); 16]));
        profile.insert("properties", groups);
    } else {
        let mut property = NbtCompound::new();
        if !empty {
            property.insert("name", "x");
            property.insert("value", "x".repeat(1024));
        }
        profile.insert("properties", NbtList::Compound(vec![property; 17]));
    }
    NbtTag::Compound(profile)
}

#[test]
fn brewing_task14_persistent_profile_rejects_before_copy() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for (map, empty) in [(false, true), (false, false), (true, false)] {
        let tag = profile_tag(map, empty);
        let mut profile_bytes = Vec::new();
        tag.write(&mut profile_bytes);
        let borrowed =
            read_tag(&mut Cursor::new(profile_bytes.as_slice())).expect("borrowed profile");
        let stats = allocation_counter::measure(|| {
            use simdnbt::FromNbtTag as _;
            assert!(ResolvableProfile::from_nbt_tag(borrowed.as_tag()).is_none());
        });
        assert_eq!(
            stats.bytes_total, 0,
            "borrowed excessive properties allocated before rejection"
        );
        let mut exact = NbtCompound::new();
        exact.insert("minecraft:profile", tag.clone());
        let mut root = NbtCompound::new();
        root.insert("components", exact);
        let lock_tag = NbtTag::Compound(root);
        let snapshot = snapshot_with_lock_tag(lock_tag.clone());
        let mut lock_bytes = Vec::new();
        lock_tag.write(&mut lock_bytes);
        let item = envelope("lock", &lock_bytes);
        for route in ["registered", "lock", "item", "snapshot"] {
            let mut work = codec_work::Work::default();
            let stats = allocation_counter::measure(|| {
                work = codec_work::measure(|| match route {
                    "registered" => assert!(entry("profile").read_nbt_owned(&tag).is_none()),
                    "lock" => assert!(LockCode::read(&mut Cursor::new(&lock_bytes)).is_err()),
                    "item" => assert!(read_brewing_item(&item).is_none()),
                    _ => assert!(decode_brewing_snapshot(&snapshot).is_none()),
                });
            });
            assert!(
                stats.bytes_total < 150_000,
                "profile rejection allocation grew unexpectedly: {stats:?}"
            );
            eprintln!("task14 profile {route} map={map} empty={empty}: {work:?} {stats:?}");
            if work.profile_copies != 0 {
                failures.push((route, map, empty, work.profile_copies));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "profile copied before cardinality rejection: {failures:?}"
    );
}

#[test]
fn brewing_task14_primitive_profile_cardinality_precedes_registered_copy() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for properties in [
        NbtList::Short(vec![0; 17]),
        NbtList::Int(vec![0; 17]),
        NbtList::Long(vec![0; 17]),
        NbtList::Float(vec![0.0; 17]),
        NbtList::Double(vec![0.0; 17]),
    ] {
        let mut profile = NbtCompound::new();
        profile.insert("properties", properties);
        let tag = NbtTag::Compound(profile);
        let mut exact = NbtCompound::new();
        exact.insert("minecraft:profile", tag.clone());
        let mut root = NbtCompound::new();
        root.insert("components", exact);
        let lock_tag = NbtTag::Compound(root);
        let snapshot = snapshot_with_lock_tag(lock_tag.clone());
        let mut lock_bytes = Vec::new();
        lock_tag.write(&mut lock_bytes);
        let item = envelope("lock", &lock_bytes);
        for route in ["registered", "lock", "item", "snapshot"] {
            let mut work = codec_work::Work::default();
            let stats = allocation_counter::measure(|| {
                work = codec_work::measure(|| match route {
                    "registered" => assert!(entry("profile").read_nbt_owned(&tag).is_none()),
                    "lock" => assert!(LockCode::read(&mut Cursor::new(&lock_bytes)).is_err()),
                    "item" => assert!(read_brewing_item(&item).is_none()),
                    _ => assert!(decode_brewing_snapshot(&snapshot).is_none()),
                });
            });
            eprintln!("task14 primitive profile {route}: {work:?} {stats:?}");
            if work.profile_copies != 0 {
                failures.push(route);
            }
            assert!(stats.bytes_total < 30_000);
            if route == "registered" && stats.bytes_total != 0 {
                failures.push(route);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "primitive cardinality rejected after copy/allocation: {failures:?}"
    );
}

#[test]
fn brewing_task14_registered_profile_keeps_valid_and_empty_list_semantics() {
    use simdnbt::FromNbtTag as _;
    init_vanilla_registry();
    let mut property = NbtCompound::new();
    property.insert("name", "textures");
    property.insert("value", "payload");
    for properties in [
        NbtList::Compound(vec![property]),
        NbtList::Empty,
        NbtList::Short(vec![]),
        NbtList::Int(vec![]),
        NbtList::Long(vec![]),
        NbtList::Float(vec![]),
        NbtList::Double(vec![]),
    ] {
        let mut profile = NbtCompound::new();
        profile.insert("name", "Alex");
        profile.insert("properties", properties);
        let tag = NbtTag::Compound(profile);
        let mut bytes = Vec::new();
        tag.write(&mut bytes);
        let borrowed = read_tag(&mut Cursor::new(bytes.as_slice())).expect("borrowed profile");
        let expected = ResolvableProfile::from_nbt_tag(borrowed.as_tag())
            .expect("previous accepted semantics");
        let actual = entry("profile")
            .read_nbt_owned(&tag)
            .expect("registered profile");
        assert_eq!(actual.downcast_ref::<ResolvableProfile>(), Some(&expected));
    }
}
