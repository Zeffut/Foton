use super::*;
use foton_registry::data_components::components::InstrumentComponent;
use foton_registry::{
    RegistryHolder, instrument::InstrumentValue, sound_event::SoundEventHolder, sound_events,
};
use foton_utils::serial::{ReadFrom, budget::DecodeBudget};
use foton_utils::{nbt::compound_work, text};
use text_components::{
    custom::{CustomData as TextCustomData, Payload},
    interactivity::ClickEvent,
};

fn custom(payload: NbtCompound) -> TextComponent {
    let mut text = TextComponent::plain("x");
    text.interactions.click = Some(ClickEvent::Custom(TextCustomData {
        id: "test:click".into(),
        payload: Payload::Nbt(NbtTag::Compound(payload).into()),
    }));
    text
}
fn lock_tag(name: &str, value: NbtTag) -> NbtTag {
    let mut exact = NbtCompound::new();
    exact.insert(format!("minecraft:{name}"), value);
    let mut lock = NbtCompound::new();
    lock.insert("components", exact);
    NbtTag::Compound(lock)
}
fn snapshot_name(tag: &NbtTag) -> Vec<u8> {
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.custom_name = Some(TextComponent::plain("old"));
    let mut bytes = encode_brewing_snapshot(&value).expect("snapshot");
    let offset = custom_name_length_offset(&bytes);
    let mut replacement = Vec::new();
    tag.write(&mut replacement);
    replace_snapshot_blob(&mut bytes, offset, &replacement);
    bytes
}
#[test]
fn brewing_task13_raw_text_rejects_duplicate_and_malformed_mutf8() {
    init_vanilla_registry();
    let mut duplicate = NbtCompound::new();
    duplicate.insert("a", 1);
    duplicate.insert("a", 1);
    let mut bad_name = NbtCompound::new();
    bad_name.insert(simdnbt::Mutf8String::from_vec(vec![0xff]), 1);
    let mut bad_string = NbtCompound::new();
    bad_string.insert(
        "a",
        NbtTag::String(simdnbt::Mutf8String::from_vec(vec![0xff])),
    );
    let mut failures = Vec::new();
    for (label, payload) in [
        ("duplicate", duplicate),
        ("name", bad_name),
        ("string", bad_string),
    ] {
        let text = custom(payload);
        let tag = text.to_codec_nbt();
        let mut body = Vec::new();
        tag.write(&mut body);
        let snapshot = snapshot_name(&tag);
        let direct = envelope("custom_name", &body);
        // A direct instrument holder's prefix, followed by its description text.
        let instrument = InstrumentComponent::new(RegistryHolder::direct(
            InstrumentValue::new(
                SoundEventHolder::registry(&sound_events::ITEM_GOAT_HORN_SOUND_0),
                1.0,
                16.0,
                text,
            )
            .expect("instrument"),
        ));
        let mut indirect = Vec::new();
        instrument
            .write(&mut indirect)
            .expect("ordinary instrument");
        let indirect = envelope("instrument", &indirect);
        let stats = allocation_counter::measure(|| {
            if decode_brewing_snapshot(&snapshot).is_some() {
                failures.push(format!("snapshot accepts {label}"));
            }
            if read_brewing_item(&direct).is_some() {
                failures.push(format!("direct accepts {label}"));
            }
            if read_brewing_item(&indirect).is_some() {
                failures.push(format!("indirect accepts {label}"));
            }
        });
        assert!(stats.bytes_max < 1024 * 1024, "{stats:?}");
    }
    assert!(failures.is_empty(), "{failures:?}");
}
fn text_chain(depth: usize) -> NbtTag {
    let mut child = NbtCompound::new();
    child.insert("text", "leaf");
    for _ in 0..depth {
        let mut parent = NbtCompound::new();
        parent.insert("text", "");
        parent.insert("extra", NbtList::Compound(vec![child]));
        child = parent;
    }
    NbtTag::Compound(child)
}
fn translation_chain(depth: usize) -> NbtTag {
    let mut child = NbtCompound::new();
    child.insert("text", "leaf");
    for _ in 0..depth {
        let mut parent = NbtCompound::new();
        parent.insert("translate", "test.key");
        parent.insert("with", NbtList::Compound(vec![child]));
        child = parent;
    }
    NbtTag::Compound(child)
}
#[test]
fn brewing_task13_persistent_text_decode_is_linear() {
    init_vanilla_registry();
    for (kind, chain) in [
        ("extra", text_chain as fn(usize) -> NbtTag),
        ("translation", translation_chain),
    ] {
        let wire = |depth| {
            let mut bytes = Vec::new();
            lock_tag("custom_name", chain(depth)).write(&mut bytes);
            bytes
        };
        let a = wire(8);
        let b = wire(32);
        let measure = |bytes: &[u8]| {
            allocation_counter::measure(|| {
                DecodeBudget::new(16 * 1024 * 1024)
                    .decode(|| LockCode::read(&mut Cursor::new(bytes)))
                    .expect("lock text");
            })
        };
        let a = measure(&a);
        let b = measure(&b);
        eprintln!("task13 text {kind} depth8={a:?} depth32={b:?}");
        assert!(
            b.count_total < a.count_total * 6,
            "text suffix copies: {a:?} {b:?}"
        );
        assert!(
            b.bytes_total < a.bytes_total * 6,
            "text suffix bytes: {a:?} {b:?}"
        );
    }
}
fn snbt_lock(snbt: String) -> NbtTag {
    let mut block = NbtCompound::new();
    block.insert("nbt", snbt);
    lock_tag("can_break", NbtTag::Compound(block))
}
#[test]
fn brewing_task13_snbt_list_decode_is_linear() {
    init_vanilla_registry();
    let wire = |depth| {
        let tag = snbt_lock(format!("{{x:{}1{}}}", "[".repeat(depth), "]".repeat(depth)));
        let snapshot = snapshot_with_lock_tag(tag.clone());
        let decoded = decode_brewing_snapshot(&snapshot).expect("accepted snapshot SNBT");
        assert_eq!(
            encode_brewing_snapshot(&decoded).expect("reencode"),
            snapshot
        );
        let mut bytes = Vec::new();
        tag.write(&mut bytes);
        bytes
    };
    let a = wire(8);
    let b = wire(16);
    let measure = |bytes: &[u8]| {
        allocation_counter::measure(|| {
            DecodeBudget::new(512 * 1024)
                .decode(|| LockCode::read(&mut Cursor::new(bytes)))
                .expect("snbt lock");
        })
    };
    let a = measure(&a);
    let b = measure(&b);
    eprintln!("task13 snbt lists depth8={a:?} depth16={b:?}");
    assert!(
        b.count_total * 10 < a.count_total * 26,
        "SNBT suffix copies: {a:?} {b:?}"
    );
    assert!(
        b.bytes_total * 10 < a.bytes_total * 26,
        "SNBT suffix bytes: {a:?} {b:?}"
    );
}

#[test]
fn brewing_task13_snbt_maps_use_actual_snapshot_route() {
    init_vanilla_registry();
    let mut stats = Vec::new();
    for count in [32usize, 128] {
        let snbt = format!(
            "{{{}}}",
            (0..count)
                .map(|i| format!("a{i:04}:0"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let bytes = snapshot_with_lock_tag(snbt_lock(snbt));
        let mut comparisons = 0;
        let measured = allocation_counter::measure(|| {
            comparisons = compound_work::measure(|| {
                let decoded = decode_brewing_snapshot(&bytes).expect("accepted snapshot map");
                assert_eq!(encode_brewing_snapshot(&decoded).expect("reencode"), bytes);
            });
        });
        eprintln!("task13 SNBT snapshot keys={count} comparisons={comparisons} {measured:?}");
        assert!(
            comparisons < count * count.ilog2() as usize * 3,
            "snapshot SNBT duplicate comparisons: {comparisons}"
        );
        assert!(measured.bytes_max < 1024 * 1024, "{measured:?}");
        stats.push(measured);
    }
    assert!(stats[1].count_total < stats[0].count_total * 6);
    assert!(stats[1].bytes_total < stats[0].bytes_total * 6);
}

#[test]
fn brewing_task13_accepted_text_routes_scale_and_roundtrip() {
    init_vanilla_registry();
    for (kind, chain) in [
        ("extra", text_chain as fn(usize) -> NbtTag),
        ("translation", translation_chain),
    ] {
        for route in ["snapshot", "direct", "indirect", "lock"] {
            let fixture = |depth| {
                let text = text::from_nbt(&chain(depth)).expect("fixture text");
                if route == "snapshot" || route == "lock" {
                    let mut value = snapshot(vec![ItemStack::empty(); 5]);
                    if route == "snapshot" {
                        value.custom_name = Some(text);
                    } else {
                        value.lock = lock(vec![(entry("custom_name"), ComponentData::new(text))]);
                    }
                    let bytes = encode_brewing_snapshot(&value).expect("fixture snapshot");
                    return bytes;
                }
                let mut body = Vec::new();
                if route == "direct" {
                    entry("custom_name")
                        .write_network_bounded(&ComponentData::new(text), 1024 * 1024, &mut body)
                        .expect("fixture direct text");
                    return envelope("custom_name", &body);
                }
                let instrument = InstrumentComponent::new(RegistryHolder::direct(
                    InstrumentValue::new(
                        SoundEventHolder::registry(&sound_events::ITEM_GOAT_HORN_SOUND_0),
                        1.0,
                        16.0,
                        text,
                    )
                    .expect("instrument"),
                ));
                entry("instrument")
                    .write_network_bounded(&ComponentData::new(instrument), 1024 * 1024, &mut body)
                    .expect("fixture instrument");
                envelope("instrument", &body)
            };
            let small = fixture(4);
            let large = fixture(8);
            let measure = |bytes: &[u8]| {
                allocation_counter::measure(|| {
                    if route == "snapshot" || route == "lock" {
                        let value = decode_brewing_snapshot(bytes).unwrap_or_else(|| {
                            panic!("accepted text snapshot {route} bytes={}", bytes.len())
                        });
                        assert_eq!(encode_brewing_snapshot(&value).expect("reencode"), bytes);
                    } else {
                        let value = read_brewing_item(bytes).expect("accepted text item");
                        assert_eq!(encode_brewing_item(&value).expect("reencode"), bytes);
                    }
                })
            };
            let a = measure(&small);
            let b = measure(&large);
            eprintln!("task13 text {kind} {route} depth4={a:?} depth8={b:?}");
            assert!(b.count_total < a.count_total * 3, "{route}: {a:?} {b:?}");
            assert!(b.bytes_total < a.bytes_total * 3, "{route}: {a:?} {b:?}");
        }
    }
}

#[test]
fn brewing_task13_raw_text_rejects_noncanonical_utf_aliases() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for (label, raw) in [
        ("raw NUL", vec![0]),
        ("overlong two", vec![0xc1, 0x81]),
        ("overlong three", vec![0xe0, 0x81, 0x81]),
    ] {
        for key in [false, true] {
            let mut payload = NbtCompound::new();
            if key {
                payload.insert(simdnbt::Mutf8String::from_vec(raw.clone()), 1);
            } else {
                payload.insert(
                    "a",
                    NbtTag::String(simdnbt::Mutf8String::from_vec(raw.clone())),
                );
            }
            let text = custom(payload);
            let tag = text.to_codec_nbt();
            let mut body = Vec::new();
            tag.write(&mut body);
            if decode_brewing_snapshot(&snapshot_name(&tag)).is_some() {
                failures.push(format!("snapshot {label} key={key}"));
            }
            if read_brewing_item(&envelope("custom_name", &body)).is_some() {
                failures.push(format!("direct {label} key={key}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn brewing_task13_raw_text_keeps_java_utf16_units() {
    init_vanilla_registry();
    // writeUTF emits UTF-16 code units, so lone surrogates remain valid NBT data.
    for raw in [
        vec![0xc0, 0x80],
        vec![0xed, 0xa0, 0x80],
        vec![0xed, 0xb0, 0x80],
        vec![0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80],
    ] {
        let mut payload = NbtCompound::new();
        payload.insert(
            simdnbt::Mutf8String::from_vec(raw.clone()),
            NbtTag::String(simdnbt::Mutf8String::from_vec(raw)),
        );
        let text = custom(payload);
        let tag = text.to_codec_nbt();
        let bytes = snapshot_name(&tag);
        let decoded = decode_brewing_snapshot(&bytes).expect("canonical Java UTF-16");
        assert_eq!(encode_brewing_snapshot(&decoded).expect("reencode"), bytes);
    }
}
