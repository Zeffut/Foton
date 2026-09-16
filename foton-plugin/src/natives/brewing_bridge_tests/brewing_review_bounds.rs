use super::*;
use foton_utils::{
    nbt::sort_work,
    serial::{WriteTo, nbt_stream},
};
use simdnbt::owned::read_tag;
use std::{io, thread::Builder};

#[test]
fn brewing_review_primitive_arrays_cannot_expand_past_quota() {
    init_vanilla_registry();
    for (name, array) in [
        ("flags", NbtTag::ByteArray(vec![1; 27000])),
        ("colors", NbtTag::IntArray(vec![1; 27000])),
        ("floats", NbtTag::LongArray(vec![1; 27000])),
    ] {
        let mut model = NbtCompound::new();
        model.insert(name, array);
        let mut components = NbtCompound::new();
        components.insert("minecraft:custom_model_data", model);
        let mut predicate = NbtCompound::new();
        predicate.insert("components", components);
        let bytes = snapshot_with_lock_tag(NbtTag::Compound(predicate));
        let stats =
            allocation_counter::measure(|| assert!(decode_brewing_snapshot(&bytes).is_none()));
        eprintln!("array {name}: {stats:?}");
        assert!(stats.bytes_max < 1 << 20, "{name}: {stats:?}");
    }
}

#[test]
fn brewing_review_subquota_counts_reject_before_reserving() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for (name, prefix, count) in [
        ("custom_model_data", vec![0, 0], 20000),
        ("tool", vec![], 1000),
        ("attribute_modifiers", vec![], 1000),
        ("enchantments", vec![], 1000),
        ("tooltip_display", vec![0], 1000),
        ("bees", vec![], 1000),
        ("potion_contents", vec![0, 0], 1000),
        ("banner_patterns", vec![], 1000),
        ("blocks_attacks", vec![0; 8], 1000),
        ("death_protection", vec![], 1000),
        ("consumable", vec![0, 0, 0, 0, 0, 1, 0], 1000),
    ] {
        let mut body = prefix;
        VarInt(count).write(&mut body).expect("count");
        let mut bytes = envelope(name, &body);
        bytes.truncate(bytes.len() - 4);
        let stats = allocation_counter::measure(|| assert!(read_brewing_item(&bytes).is_none()));
        eprintln!("tiny {name}: {stats:?}");
        if stats.bytes_max > 8192 {
            failures.push((name, stats.bytes_max));
        }
    }
    let mut body = Vec::new();
    VarInt(128).write(&mut body).expect("lore count");
    body.extend_from_slice(&[8, 0, 0]); // One complete empty name, then EOF.
    let mut bytes = envelope("lore", &body);
    bytes.truncate(bytes.len() - 4);
    let stats = allocation_counter::measure(|| assert!(read_brewing_item(&bytes).is_none()));
    if stats.bytes_max > 8192 {
        failures.push(("lore", stats.bytes_max));
    }
    assert!(failures.is_empty(), "premature reservations: {failures:?}");
}

#[test]
fn brewing_review_encoder_errors_are_bounded() {
    init_vanilla_registry();
    let mut value = ItemEnchantments::empty();
    value
        .levels
        .insert(Identifier::new("test", "x".repeat(2 << 20)), 1);
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(ENCHANTMENTS, value);
    let stats = allocation_counter::measure(|| assert!(encode_brewing_item(&item).is_none()));
    eprintln!("unknown enchantment error: {stats:?}");
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn brewing_review_indirect_nbt_stops_at_callers_output_budget() {
    init_vanilla_registry();
    let mut compound = NbtCompound::new();
    compound.insert("first", NbtTag::ByteArray(vec![0; 100]));
    // Reaching this malformed suffix proves an unlimited preliminary traversal occurred.
    compound.insert("late", "x".repeat(65536));
    let custom = CustomData::try_from_compound(compound).expect("custom");
    let entity = EntityData::new(
        REGISTRY
            .entity_types
            .by_key(&Identifier::vanilla_static("pig"))
            .expect("pig"),
        custom.clone(),
    );
    let block = BlockEntityData::new(
        REGISTRY
            .block_entity_types
            .by_key(&Identifier::vanilla_static("chest"))
            .expect("chest"),
        custom.clone(),
    );
    for (name, value) in [
        ("entity_data", ComponentData::new(entity.clone())),
        ("block_entity_data", ComponentData::new(block)),
        (
            "bees",
            ComponentData::new(Bees::new(vec![BeehiveOccupant::new(entity, 0, 0)])),
        ),
    ] {
        let error = entry(name)
            .write_network_bounded(&value, 32, &mut io::sink())
            .expect_err("tiny budget");
        assert_eq!(
            error.to_string(),
            "serialization budget exceeded",
            "{name} inspected suffix past budget"
        );
    }
}

#[test]
fn brewing_review_large_snbt_predicate_has_bounded_sorting_work() {
    init_vanilla_registry();
    let mut compound = NbtCompound::new();
    for i in (0..6000).rev() {
        compound.insert(format!("k{i:04}"), 1_i8);
    }
    let adventure = AdventureModePredicate::new(vec![BlockPredicate::new(
        None,
        None,
        Some(NbtPredicate::new(compound).expect("NBT predicate")),
        DataComponentMatchers::ANY,
    )])
    .expect("adventure");
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.lock = lock(vec![(entry("can_break"), ComponentData::new(adventure))]);
    let mut comparisons = 0;
    let stats = allocation_counter::measure(|| {
        comparisons = sort_work::measure(|| assert!(encode_brewing_snapshot(&value).is_some()));
    });
    eprintln!("6000 SNBT fields: {comparisons} comparisons, {stats:?}");
    assert!(comparisons > 6000, "SNBT sorting route was not observed");
    assert!(
        comparisons < 6 * 6000 * 6000usize.ilog2() as usize,
        "quadratic SNBT ordering: {comparisons} comparisons"
    );
    assert!(stats.bytes_max < 1 << 20, "sorting workspace: {stats:?}");
}

#[test]
fn brewing_review_nbt_depth_counts_each_container_once() {
    // A dedicated stack also accommodates the reference reader's recursive implementation.
    Builder::new()
        .stack_size(16 << 20)
        .spawn(|| {
            for shape in 0..3 {
                for depth in [300, 511, 512, 513] {
                    let mut value = NbtTag::Compound(NbtCompound::new());
                    for level in 1..depth {
                        if shape == 0 || (shape == 2 && level % 2 == 0) {
                            let mut compound = NbtCompound::new();
                            compound.insert("x", value);
                            value = NbtTag::Compound(compound);
                        } else {
                            value = NbtTag::List(NbtList::from(vec![value]));
                        }
                    }
                    let mut reference = Vec::new();
                    value.write(&mut reference);
                    let accepted = read_tag(&mut Cursor::new(reference.as_slice())).is_ok();
                    let mut actual = Vec::new();
                    let result = WriteTo::write(&value, &mut actual);
                    assert_eq!(result.is_ok(), accepted, "shape={shape} depth={depth}");
                    if accepted {
                        assert_eq!(actual, reference);
                    }
                    let bounded = nbt_stream::wire_size(&value, MAX_BREWING_ITEM_BYTES);
                    assert_eq!(
                        bounded.is_ok(),
                        accepted,
                        "bounded shape={shape} depth={depth}"
                    );
                }
            }
        })
        .expect("worker")
        .join()
        .expect("depth assertions");
}

#[test]
fn brewing_review_unknown_holder_tag_errors_are_bounded() {
    use foton_registry::data_components::components::Repairable;
    use foton_registry::data_components::vanilla_components::REPAIRABLE;
    init_vanilla_registry();
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(
        REPAIRABLE,
        Repairable::new(RegistryHolderSet::Tag(Identifier::new(
            "test",
            "x".repeat(2 << 20),
        ))),
    );
    let stats = allocation_counter::measure(|| assert!(encode_brewing_item(&item).is_none()));
    assert!(stats.bytes_max < 1 << 20, "unknown holder tag: {stats:?}");
}
