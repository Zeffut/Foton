use super::*;
use foton_registry::item_predicate::{
    StatePropertiesPredicate, StatePropertyMatcher, StatePropertyValueMatcher,
};
use foton_utils::serial::{ReadFrom, budget::DecodeBudget, nbt_encode};

fn state(reverse: bool) -> AdventureModePredicate {
    let mut properties = vec![
        StatePropertyMatcher::new(
            "facing".into(),
            StatePropertyValueMatcher::Exact("north".into()),
        ),
        StatePropertyMatcher::new(
            "waterlogged".into(),
            StatePropertyValueMatcher::Exact("false".into()),
        ),
    ];
    if reverse {
        properties.reverse();
    }
    AdventureModePredicate::new(vec![BlockPredicate::new(
        None,
        Some(StatePropertiesPredicate::new(properties).expect("state")),
        None,
        DataComponentMatchers::ANY,
    )])
    .expect("adventure")
}
#[test]
fn brewing_task12_persistent_state_maps_are_canonical() {
    init_vanilla_registry();
    for name in ["can_break", "can_place_on"] {
        let encode = |reverse| {
            let value = state(reverse);
            let mut direct = Vec::new();
            nbt_encode::write_bounded(&value, 65536, &mut direct).expect("persistent adventure");
            let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
            snapshot.lock = lock(vec![(entry(name), ComponentData::new(value))]);
            (
                direct,
                encode_brewing_snapshot(&snapshot).expect("snapshot"),
            )
        };
        let a = encode(false);
        let b = encode(true);
        assert_eq!(a, b, "{name}: persistent map depends on insertion order");
        let decoded = decode_brewing_snapshot(&b.1).expect("own canonical snapshot must decode");
        assert_eq!(encode_brewing_snapshot(&decoded).expect("reencode"), b.1);
        let mut ordinary_a = Vec::new();
        let mut ordinary_b = Vec::new();
        state(false).write(&mut ordinary_a).expect("network");
        state(true).write(&mut ordinary_b).expect("network");
        assert_ne!(
            ordinary_a, ordinary_b,
            "ordinary network list order remains meaningful"
        );
        let noncanonical = entry(name)
            .write_nbt(&ComponentData::new(state(true)))
            .expect("ordinary persistent value");
        let mut exact = NbtCompound::new();
        exact.insert(entry(name).key.to_string(), noncanonical);
        let mut root = NbtCompound::new();
        root.insert("components", exact);
        assert!(
            decode_brewing_snapshot(&snapshot_with_lock_tag(NbtTag::Compound(root))).is_none(),
            "permuted lock accepted"
        );
    }
}

fn persistent_chain(depth: usize, family: &str) -> NbtTag {
    let mut tag = NbtTag::Compound(NbtCompound::new());
    for _ in 0..depth {
        let mut components = NbtCompound::new();
        if family == "lock" {
            components.insert("minecraft:lock", tag);
        } else {
            let mut patch = NbtCompound::new();
            patch.insert("minecraft:can_break", tag);
            let mut item = NbtCompound::new();
            item.insert("id", "minecraft:stone");
            item.insert("components", patch);
            components.insert("minecraft:use_remainder", item);
        }
        let mut parent = NbtCompound::new();
        parent.insert("components", components);
        tag = NbtTag::Compound(parent);
    }
    tag
}
#[test]
fn brewing_task12_persistent_lock_decode_allocations_are_linear() {
    init_vanilla_registry();
    for family in ["adventure_template", "lock"] {
        let encoded = |depth| {
            let mut bytes = Vec::new();
            persistent_chain(depth, family).write(&mut bytes);
            bytes
        };
        let small = encoded(4);
        let large = encoded(16);
        let measure = |bytes: &[u8]| {
            allocation_counter::measure(|| {
                DecodeBudget::new(16 * 1024 * 1024)
                    .decode(|| LockCode::read(&mut Cursor::new(bytes)))
                    .expect("persistent lock");
            })
        };
        let a = measure(&small);
        let b = measure(&large);
        eprintln!("task12 persistent {family} depth4={a:?} depth16={b:?}");
        assert!(
            b.count_total < a.count_total * 6,
            "quadratic persistent allocations: {a:?} {b:?}"
        );
        assert!(
            b.bytes_total < a.bytes_total * 6,
            "quadratic persistent bytes: {a:?} {b:?}"
        );
    }
}
#[test]
fn brewing_task12_persistent_lock_uses_shared_linear_budget() {
    init_vanilla_registry();
    for family in ["lock", "adventure_template"] {
        let bytes = snapshot_with_lock_tag(persistent_chain(48, family));
        let decoded =
            decode_brewing_snapshot(&bytes).expect("48 descendants fit the single Brew allowance");
        assert_eq!(encode_brewing_snapshot(&decoded).expect("roundtrip"), bytes);
    }
    // A failed subtree must not leave deferred validation active for the next read.
    let mut bad = NbtCompound::new();
    bad.insert("minecraft:damage", -1);
    let mut leaf = NbtCompound::new();
    leaf.insert("components", bad);
    let mut invalid = NbtTag::Compound(leaf);
    for _ in 0..8 {
        let mut exact = NbtCompound::new();
        exact.insert("minecraft:lock", invalid);
        let mut parent = NbtCompound::new();
        parent.insert("components", exact);
        invalid = NbtTag::Compound(parent);
    }
    assert!(decode_brewing_snapshot(&snapshot_with_lock_tag(invalid)).is_none());
    let mut malformed = vec![1, 0, 0, 0, 1];
    VarInt(entry("max_stack_size").id() as i32)
        .write(&mut malformed)
        .expect("id");
    malformed.extend([0, 0]);
    assert!(read_brewing_item(&envelope("can_break", &malformed)).is_none());
}

#[test]
fn brewing_task12_zero_count_removal_depth_rejects_before_hash() {
    init_vanilla_registry();
    for (wrappers, accepted) in [(252, true), (253, false)] {
        let mut bundle = vec![1];
        for _ in 0..wrappers {
            for value in [
                vanilla_items::STONE.id() as i32,
                1,
                1,
                0,
                entry("use_remainder").id() as i32,
            ] {
                VarInt(value).write(&mut bundle).expect("wrapper");
            }
        }
        for value in [
            vanilla_items::STONE.id() as i32,
            1,
            0,
            1,
            entry("damage").id() as i32,
        ] {
            VarInt(value).write(&mut bundle).expect("removal");
        }
        let mut inner = vec![1, 0, 0, 0, 1];
        VarInt(entry("bundle_contents").id() as i32)
            .write(&mut inner)
            .expect("bundle id");
        inner.extend(bundle);
        inner.push(0);
        // Outer exact validation must validate the CAN_BREAK persistent root, including removal depth.
        let mut outer = vec![1, 0, 0, 0, 1];
        VarInt(entry("can_break").id() as i32)
            .write(&mut outer)
            .expect("adventure id");
        outer.extend(inner);
        outer.push(0);
        let mut bytes = envelope("can_break", &outer);
        bytes[0] = 0;
        let decoded = read_brewing_item(&bytes);
        assert_eq!(
            decoded.is_some(),
            accepted,
            "{wrappers} wrappers in zero-count raw item"
        );
        if let Some(item) = decoded {
            assert_eq!(item.count, 0);
            assert_eq!(item.item, &*vanilla_items::STONE);
            assert_eq!(encode_brewing_item(&item).expect("roundtrip"), bytes);
        }
    }
}

fn partial_chain(depth: usize) -> NbtTag {
    let mut tag = NbtTag::Compound(NbtCompound::new());
    for _ in 0..depth {
        let NbtTag::Compound(child) = tag else {
            panic!("item predicate");
        };
        let mut collection = NbtCompound::new();
        collection.insert("contains", NbtList::Compound(vec![child]));
        let mut bundle = NbtCompound::new();
        bundle.insert("items", collection);
        let mut predicates = NbtCompound::new();
        predicates.insert("minecraft:bundle_contents", bundle);
        let mut parent = NbtCompound::new();
        parent.insert("predicates", predicates);
        tag = NbtTag::Compound(parent);
    }
    tag
}
#[test]
fn brewing_task12_recursive_partial_lock_decoding_is_linear() {
    init_vanilla_registry();
    let encoded = |depth| {
        let mut bytes = Vec::new();
        partial_chain(depth).write(&mut bytes);
        bytes
    };
    let small = encoded(4);
    let large = encoded(16);
    let measure = |bytes: &[u8]| {
        allocation_counter::measure(|| {
            DecodeBudget::new(16 * 1024 * 1024)
                .decode(|| LockCode::read(&mut Cursor::new(bytes)))
                .expect("partial lock");
        })
    };
    let a = measure(&small);
    let b = measure(&large);
    eprintln!("task12 partial lock depth4={a:?} depth16={b:?}");
    assert!(
        b.count_total < a.count_total * 6,
        "partial suffix copies: {a:?} {b:?}"
    );
    assert!(
        b.bytes_total < a.bytes_total * 6,
        "partial suffix storage: {a:?} {b:?}"
    );
}

#[test]
fn brewing_task12_persistent_leaf_copy_accounting_is_preserved() {
    init_vanilla_registry();
    let mut text = NbtCompound::new();
    text.insert("text", "x".repeat(24_000));
    for _ in 0..64 {
        let mut parent = NbtCompound::new();
        parent.insert("text", "");
        parent.insert("extra", NbtList::Compound(vec![text]));
        text = parent;
    }
    let mut components = NbtCompound::new();
    components.insert("minecraft:custom_name", text.clone());
    let mut root = NbtCompound::new();
    root.insert("components", components);
    let exact = snapshot_with_lock_tag(NbtTag::Compound(root));
    let mut pages = NbtCompound::new();
    pages.insert("contains", NbtList::Compound(vec![text]));
    let mut book = NbtCompound::new();
    book.insert("pages", pages);
    let mut predicates = NbtCompound::new();
    predicates.insert("minecraft:written_book_content", book);
    let mut root = NbtCompound::new();
    root.insert("predicates", predicates);
    let partial = snapshot_with_lock_tag(NbtTag::Compound(root));
    for bytes in [exact, partial] {
        let stats = allocation_counter::measure(|| {
            assert!(
                decode_brewing_snapshot(&bytes).is_none(),
                "leaf conversion must fit the shared allowance"
            );
        });
        eprintln!("task12 retained leaf accounting {stats:?}");
        assert!(
            stats.bytes_max < 1024 * 1024,
            "unaccounted leaf suffix copies: {stats:?}"
        );
    }
}

#[path = "task13_registry.rs"]
mod task13_registry;
