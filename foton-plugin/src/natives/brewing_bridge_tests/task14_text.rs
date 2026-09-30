use super::*;
use foton_registry::data_components::components::InstrumentComponent;
use foton_registry::{
    RegistryHolder, instrument::InstrumentValue, sound_event::SoundEventHolder, sound_events,
};
use foton_utils::serial::{ReadFrom, budget::DecodeBudget, nbt_preflight};
use foton_utils::text::decode_work;
use text_components::{
    custom::{CustomData as TextCustomData, Payload},
    interactivity::ClickEvent,
};

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
fn fallback_chain(depth: usize) -> NbtTag {
    let mut child = NbtCompound::new();
    child.insert("type", "x".repeat(1024));
    for _ in 0..depth {
        let mut parent = NbtCompound::new();
        parent.insert("selector", "s");
        parent.insert("nbt", "p");
        parent.insert("separator", child);
        child = parent;
    }
    NbtTag::Compound(child)
}
#[test]
fn brewing_task14_fallback_chain_visits_and_allocations_are_linear() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for depth in [0, 4, 8, 10] {
        let tag = fallback_chain(depth);
        let mut body = Vec::new();
        tag.write(&mut body);
        let item = envelope("custom_name", &body);
        let snapshot = snapshot_name(&tag);
        for (route, bytes) in [("item", &item), ("snapshot", &snapshot)] {
            let mut visits = 0;
            let stats = allocation_counter::measure(|| {
                visits = decode_work::measure(|| {
                    let accepted = if route == "item" {
                        read_brewing_item(bytes).is_some()
                    } else {
                        decode_brewing_snapshot(bytes).is_some()
                    };
                    assert!(!accepted, "malformed fallback {route}");
                });
            });
            eprintln!(
                "task14 fallback route={route} depth={depth} nbt_bytes={} visits={visits} {stats:?}",
                body.len()
            );
            if visits != depth + 1 || stats.bytes_total > 32_000 + depth as u64 * 8000 {
                failures.push(format!("{route} C{depth}: visits={visits} {stats:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
#[test]
fn brewing_task14_text_preflight_exhaustion_precedes_conversion() {
    init_vanilla_registry();
    let mut bytes = Vec::new();
    let tag = fallback_chain(10);
    tag.write(&mut bytes);
    let admitted = |quota| {
        DecodeBudget::new(quota)
            .decode(|| nbt_preflight::check(&Cursor::new(bytes.as_slice()), true))
            .is_ok()
    };
    let mut low = 0;
    let mut high = 512 * 1024;
    assert!(admitted(high));
    while low < high {
        let mid = usize::midpoint(low, high);
        if admitted(mid) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    let charged = low;
    let visits = decode_work::measure(|| {
        assert!(
            DecodeBudget::new(charged - 1)
                .decode(|| TextComponent::read(&mut Cursor::new(bytes.as_slice())))
                .is_err()
        );
    });
    assert_eq!(visits, 0);
    let item = envelope("custom_name", &bytes);
    let snapshot = snapshot_name(&tag);
    for (route, input) in [("item", &item), ("snapshot", &snapshot)] {
        let visits = decode_work::measure(|| {
            DecodeBudget::new(charged - 1)
                .decode(|| {
                    assert!(if route == "item" {
                        read_brewing_item(input).is_none()
                    } else {
                        decode_brewing_snapshot(input).is_none()
                    });
                    Ok(())
                })
                .expect("outer shared budget");
        });
        assert_eq!(visits, 0, "{route} exhausted before conversion");
    }
    let mut visits = 0;
    let stats = allocation_counter::measure(|| {
        visits = decode_work::measure(|| {
            assert!(
                DecodeBudget::new(charged)
                    .decode(|| TextComponent::read(&mut Cursor::new(bytes.as_slice())))
                    .is_err()
            );
        });
    });
    eprintln!("task14 prepaid C10 conversion charged={charged} visits={visits} {stats:?}");
    assert_eq!(visits, 11);
    assert!(
        stats.bytes_total <= charged as u64,
        "prepaid conversion allowance: {stats:?}"
    );
}
fn text_with_payload(payload: NbtCompound) -> TextComponent {
    let mut text = TextComponent::plain("x");
    text.interactions.click = Some(ClickEvent::Custom(TextCustomData {
        id: "test:click".into(),
        payload: Payload::Nbt(NbtTag::Compound(payload).into()),
    }));
    text
}

fn check_nan_case(
    kind: &str,
    tag: &NbtTag,
    accepted: bool,
    list: bool,
    failures: &mut Vec<String>,
) {
    let value = match (tag, list) {
        (NbtTag::Float(v), true) => NbtTag::List(NbtList::Float(vec![*v])),
        (NbtTag::Double(v), true) => NbtTag::List(NbtList::Double(vec![*v])),
        _ => tag.clone(),
    };
    let mut payload = NbtCompound::new();
    payload.insert("x", value);
    let mut raw = Vec::new();
    NbtTag::Compound(payload.clone()).write(&mut raw);
    let custom_data = envelope("custom_data", &raw);
    let text = text_with_payload(payload);
    let text_tag = text.to_codec_nbt();
    let snapshot = snapshot_name(&text_tag);
    let mut text_body = Vec::new();
    text_tag.write(&mut text_body);
    let direct = envelope("custom_name", &text_body);
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
    instrument.write(&mut indirect).expect("instrument prefix");
    // The ordinary description writer also canonicalizes raw NBT. Install
    // the independent simdnbt wire fixture after the real holder prefix.
    indirect.truncate(indirect.len() - text_body.len());
    indirect.extend_from_slice(&text_body);
    let indirect = envelope("instrument", &indirect);
    for (route, bytes) in [
        ("custom_data", &custom_data),
        ("direct", &direct),
        ("indirect", &indirect),
        ("snapshot", &snapshot),
    ] {
        let result = if route == "snapshot" {
            decode_brewing_snapshot(bytes).map(|v| encode_brewing_snapshot(&v).expect("reencode"))
        } else {
            read_brewing_item(bytes).map(|v| encode_brewing_item(&v).expect("reencode"))
        };
        if result.is_some() != accepted {
            failures.push(format!(
                "{kind} list={list} {route}: accepted={}",
                result.is_some()
            ));
        }
        if accepted {
            assert_eq!(result.as_ref(), Some(bytes), "{kind} list={list} {route}");
        }
    }
}

#[test]
fn brewing_task14_raw_nbt_nan_binary_canonicality_all_routes() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for (kind, tag, accepted) in [
        (
            "float alternative 1",
            NbtTag::Float(f32::from_bits(0x7fc0_0001)),
            false,
        ),
        (
            "float alternative 2",
            NbtTag::Float(f32::from_bits(0xff80_0001)),
            false,
        ),
        (
            "double alternative 1",
            NbtTag::Double(f64::from_bits(0x7ff8_0000_0000_0001)),
            false,
        ),
        (
            "double alternative 2",
            NbtTag::Double(f64::from_bits(0xfff0_0000_0000_0001)),
            false,
        ),
        (
            "float canonical",
            NbtTag::Float(f32::from_bits(0x7fc0_0000)),
            true,
        ),
        (
            "double canonical",
            NbtTag::Double(f64::from_bits(0x7ff8_0000_0000_0000)),
            true,
        ),
        (
            "float finite",
            NbtTag::Float(f32::from_bits(0xc2f6_8000)),
            true,
        ),
        (
            "double finite",
            NbtTag::Double(f64::from_bits(0xc05e_dd2f_1a9f_be77)),
            true,
        ),
        ("float subnormal", NbtTag::Float(f32::from_bits(1)), true),
        ("double subnormal", NbtTag::Double(f64::from_bits(1)), true),
        ("float positive zero", NbtTag::Float(0.0), true),
        ("float negative zero", NbtTag::Float(-0.0), true),
        ("double positive zero", NbtTag::Double(0.0), true),
        ("double negative zero", NbtTag::Double(-0.0), true),
        ("float infinity", NbtTag::Float(f32::INFINITY), true),
        (
            "float negative infinity",
            NbtTag::Float(f32::NEG_INFINITY),
            true,
        ),
        ("double infinity", NbtTag::Double(f64::INFINITY), true),
        (
            "double negative infinity",
            NbtTag::Double(f64::NEG_INFINITY),
            true,
        ),
    ] {
        for list in [false, true] {
            check_nan_case(kind, &tag, accepted, list, &mut failures);
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
