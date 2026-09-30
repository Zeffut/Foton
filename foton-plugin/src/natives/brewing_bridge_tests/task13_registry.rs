use super::*;
use crate::natives::{BrewingPayloadWriter, MAX_BREWING_LOCK_BYTES};
use foton_registry::data_components::components::UseRemainder;
use foton_registry::data_components::vanilla_components::{
    CREATIVE_SLOT_LOCK, LOCK, USE_REMAINDER,
};
use foton_registry::{ItemStackTemplate, data_components::DataComponentPatch};
use std::{env, process::Command, thread};

#[test]
fn brewing_task13_filtered_optional_fields_roundtrip() {
    init_vanilla_registry();
    let mut rejected = 0;
    for recursive in [false, true] {
        let value = if recursive {
            let mut patch = DataComponentPatch::new();
            patch.set(CREATIVE_SLOT_LOCK, ());
            let mut template =
                ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
                    .expect("filtered template");
            for _ in 0..3 {
                let mut patch = DataComponentPatch::new();
                patch.set(USE_REMAINDER, UseRemainder::new(template));
                template =
                    ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
                        .expect("nested template");
            }
            lock(vec![(
                entry("use_remainder"),
                ComponentData::new(UseRemainder::new(template)),
            )])
        } else {
            lock(vec![(entry("creative_slot_lock"), ComponentData::new(()))])
        };
        let mut state = snapshot(vec![ItemStack::empty(); 5]);
        state.lock = value.clone();
        let bytes = encode_brewing_snapshot(&state).expect("snapshot getter");
        if let Some(decoded) = decode_brewing_snapshot(&bytes) {
            assert_eq!(
                encode_brewing_snapshot(&decoded).expect("snapshot reencode"),
                bytes
            );
        } else {
            rejected += 1;
            eprintln!("snapshot rejected recursive={recursive}");
        }
        let mut patch = DataComponentPatch::new();
        patch.set(LOCK, value);
        let item = ItemStack::from_raw_parts(&vanilla_items::STONE, 0, patch);
        let bytes = encode_brewing_item(&item).expect("item getter");
        if let Some(decoded) = read_brewing_item(&bytes) {
            assert_eq!(decoded.count, 0);
            assert_eq!(decoded.item, &*vanilla_items::STONE);
            assert_eq!(encode_brewing_item(&decoded).expect("item reencode"), bytes);
        } else {
            rejected += 1;
            eprintln!("item rejected recursive={recursive}");
        }
    }
    assert_eq!(rejected, 0, "setters must accept filtered getter output");
}

fn adventure_wire(depth: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..depth {
        bytes.extend([1, 0, 0, 0, 1]);
        VarInt(entry("can_break").id() as i32)
            .write(&mut bytes)
            .expect("id");
    }
    bytes.extend([1, 0, 0, 0, 0, 0]);
    bytes.resize(bytes.len() + depth, 0);
    bytes
}

#[test]
fn brewing_task13_adventure_recursion_stops_before_descending() {
    init_vanilla_registry();
    let bytes = adventure_wire(1000);
    // Generous host stack isolates the missing parser guard from a process abort.
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let mut cursor = Cursor::new(bytes.as_slice());
            let result =
                DecodeBudget::new(512 * 1024).decode(|| AdventureModePredicate::read(&mut cursor));
            assert!(result.is_err());
            assert!(
                cursor.position() < 4000,
                "recursive descent reached byte {} before rejection",
                cursor.position()
            );
            let valid = adventure_wire(8);
            assert!(
                DecodeBudget::new(512 * 1024)
                    .decode(|| AdventureModePredicate::read(&mut Cursor::new(&valid)))
                    .is_ok()
            );
        })
        .expect("worker")
        .join()
        .expect("guard assertions");
}

#[test]
fn brewing_task13_production_recursive_decoder_subprocess() {
    const CHILD: &str = "FOTON_TASK13_RECURSION_CHILD";
    if env::var_os(CHILD).is_none() {
        let result = Command::new(env::current_exe().expect("test executable"))
            .args(["--exact", "natives::brewing_bridge_tests::brewing_review_regressions::persistent_union::task13_registry::brewing_task13_production_recursive_decoder_subprocess", "--nocapture"])
            .env(CHILD, "1").output().expect("isolated decoder");
        assert!(
            result.status.success(),
            "isolated Brew decoder failed: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    init_vanilla_registry();
    assert!(read_brewing_item(&envelope("can_break", &adventure_wire(1000))).is_none());
    let bytes = envelope("can_break", &adventure_wire(8));
    let decoded = read_brewing_item(&bytes).expect("restored production decode scope");
    assert_eq!(
        encode_brewing_item(&decoded).expect("accepted reencode"),
        bytes
    );
}

fn leaf_values() -> Vec<(&'static str, ComponentData)> {
    use foton_registry::data_components::components::{
        BannerPatternLayers, Bees, JukeboxPlayable, PotDecorations, Recipes, SuspiciousStewEffects,
    };
    let song = REGISTRY
        .jukebox_songs
        .by_key(&Identifier::vanilla_static("cat"))
        .expect("song");
    vec![
        ("recipes", ComponentData::new(Recipes::empty())),
        (
            "recipes",
            ComponentData::new(Recipes::new(vec![Identifier::vanilla_static("bread")])),
        ),
        ("bees", ComponentData::new(Bees::empty())),
        (
            "banner_patterns",
            ComponentData::new(BannerPatternLayers::empty()),
        ),
        (
            "suspicious_stew_effects",
            ComponentData::new(SuspiciousStewEffects::empty()),
        ),
        (
            "jukebox_playable",
            ComponentData::new(JukeboxPlayable::new(song)),
        ),
        ("pot_decorations", ComponentData::new(PotDecorations::EMPTY)),
    ]
}

#[test]
fn brewing_task13_leaf_boundaries_through_exact_item() {
    init_vanilla_registry();
    for (name, value) in leaf_values() {
        for wrappers in [252, 253] {
            let mut body = vec![1, 0, 0, 0, 1];
            VarInt(entry("bundle_contents").id() as i32)
                .write(&mut body)
                .expect("bundle");
            body.push(1);
            for _ in 0..wrappers {
                for value in [
                    vanilla_items::STONE.id() as i32,
                    1,
                    1,
                    0,
                    entry("use_remainder").id() as i32,
                ] {
                    VarInt(value).write(&mut body).expect("wrapper");
                }
            }
            for value in [
                vanilla_items::STONE.id() as i32,
                1,
                1,
                0,
                entry(name).id() as i32,
            ] {
                VarInt(value).write(&mut body).expect("leaf patch");
            }
            entry(name).write_network(&value, &mut body).expect("leaf");
            body.push(0);
            let mut bytes = envelope("can_break", &body);
            bytes[0] = 0;
            let decoded = read_brewing_item(&bytes)
                .unwrap_or_else(|| panic!("accepted {name} at {wrappers} wrappers"));
            assert_eq!(decoded.count, 0);
            assert_eq!(decoded.item, &*vanilla_items::STONE);
            assert_eq!(encode_brewing_item(&decoded).expect("item reencode"), bytes);
        }
    }
}

#[test]
fn brewing_task13_leaf_boundaries_through_snapshot_lock() {
    init_vanilla_registry();
    for (name, value) in leaf_values() {
        let mut patch = NbtCompound::new();
        patch.insert(
            entry(name).key.to_string(),
            entry(name).write_nbt(&value).expect("leaf"),
        );
        let mut item = NbtCompound::new();
        item.insert("id", "minecraft:stone");
        item.insert("components", patch);
        for _ in 0..253 {
            let mut patch = NbtCompound::new();
            patch.insert("minecraft:use_remainder", item);
            item = NbtCompound::new();
            item.insert("id", "minecraft:stone");
            item.insert("components", patch);
        }
        let mut components = NbtCompound::new();
        components.insert("minecraft:bundle_contents", NbtList::Compound(vec![item]));
        let mut root = NbtCompound::new();
        root.insert("components", components);
        let mut bytes = Vec::new();
        NbtTag::Compound(root).write(&mut bytes);
        let value = DecodeBudget::new(64 * 1024 * 1024)
            .decode(|| LockCode::read(&mut Cursor::new(&bytes)))
            .expect("registered lock");
        let mut state = snapshot(vec![ItemStack::empty(); 5]);
        state.lock = value;
        let encoded =
            encode_brewing_snapshot(&state).unwrap_or_else(|| panic!("snapshot encode: {name}"));
        let read_allocation = allocation_counter::measure(|| {
            drop(
                DecodeBudget::new(512 * 1024)
                    .decode(|| LockCode::read(&mut Cursor::new(&bytes)))
                    .expect("diagnostic lock read"),
            );
        });
        let write_allocation = allocation_counter::measure(|| {
            let mut output = BrewingPayloadWriter::new(MAX_BREWING_LOCK_BYTES);
            state
                .lock
                .write_bounded(MAX_BREWING_LOCK_BYTES, &mut output)
                .expect("diagnostic lock write");
        });
        eprintln!("task13 phase {name}: read={read_allocation:?} write={write_allocation:?}");
        let allocation = allocation_counter::measure(|| {
            let decoded = decode_brewing_snapshot(&encoded)
                .unwrap_or_else(|| panic!("snapshot decode: {name}"));
            drop(decoded);
        });
        eprintln!("task13 deepest snapshot {name}: {allocation:?}");
        assert!(
            allocation.bytes_total < 512 * 1024 && allocation.bytes_max < 512 * 1024,
            "deepest snapshot allocation: {allocation:?}"
        );
        let decoded =
            decode_brewing_snapshot(&encoded).unwrap_or_else(|| panic!("snapshot decode: {name}"));
        assert_eq!(
            encode_brewing_snapshot(&decoded).expect("snapshot reencode"),
            encoded
        );
    }
}
