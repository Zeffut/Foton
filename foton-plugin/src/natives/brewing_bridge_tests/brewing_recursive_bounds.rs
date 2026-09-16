use super::*;
use foton_registry::data_component_predicate::{
    DataComponentExactPredicate, DataComponentMatchers,
};
use foton_registry::data_components::components::BundleContents;
use foton_registry::data_components::vanilla_components::BUNDLE_CONTENTS;
use foton_registry::data_components::{ComponentData, DataComponentPatch};
use foton_registry::item_predicate::{IntBounds, ItemPredicate};
use foton_utils::serial::ReadFrom as _;
use simdnbt::owned::{NbtTag, read_tag};
use std::io::Cursor;

#[test]
fn rejected_lore_streams_nested_text_without_owned_nbt() {
    init_vanilla_registry();
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(
        LORE,
        ItemLore::new(vec![TextComponent::plain("x".repeat(2 << 20))]).expect("one line"),
    );
    let stats = allocation_counter::measure(|| {
        assert!(encode_brewing_item(&item).is_none());
    });
    eprintln!("oversized lore: {stats:?}");
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn rejected_exact_lock_lore_streams_without_cloning_the_value() {
    init_vanilla_registry();
    let lore = ItemLore::new(
        (0..64)
            .map(|_| TextComponent::plain("x".repeat(32768)))
            .collect(),
    )
    .expect("valid lore");
    let entry = REGISTRY
        .data_components
        .by_key(&Identifier::vanilla_static("lore"))
        .expect("lore registered");
    let exact = DataComponentExactPredicate::new(vec![(entry, ComponentData::new(lore))])
        .expect("persistent lore");
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.lock = LockCode::new(ItemPredicate::new(
        None,
        IntBounds::ANY,
        DataComponentMatchers::new(exact, vec![]).expect("exact matcher"),
    ));
    let stats = allocation_counter::measure(|| {
        assert!(encode_brewing_snapshot(&value).is_none());
    });
    eprintln!("oversized exact lore: {stats:?}");
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn rejected_book_streams_nested_text_without_owned_nbt() {
    init_vanilla_registry();
    let mut item = ItemStack::new(&vanilla_items::WRITTEN_BOOK);
    item.set(
        WRITTEN_BOOK_CONTENT,
        WrittenBookContent::new(
            Filterable::pass_through("title".to_owned()),
            "author".to_owned(),
            0,
            vec![Filterable::pass_through(TextComponent::plain(
                "x".repeat(2 << 20),
            ))],
            false,
        )
        .expect("book"),
    );
    let stats = allocation_counter::measure(|| {
        assert!(encode_brewing_item(&item).is_none());
    });
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn rejected_exact_lock_recursive_lore_streams_without_cloning_templates() {
    init_vanilla_registry();
    let mut patch = DataComponentPatch::new();
    patch.set(
        LORE,
        ItemLore::new(
            (0..64)
                .map(|_| TextComponent::plain("x".repeat(32768)))
                .collect(),
        )
        .expect("lore"),
    );
    let template = ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
        .expect("template");
    let entry = REGISTRY
        .data_components
        .by_key(&Identifier::vanilla_static("bundle_contents"))
        .expect("bundle");
    let exact = DataComponentExactPredicate::new(vec![(
        entry,
        ComponentData::new(BundleContents::new(vec![template])),
    )])
    .expect("exact");
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.lock = LockCode::new(ItemPredicate::new(
        None,
        IntBounds::ANY,
        DataComponentMatchers::new(exact, vec![]).expect("matcher"),
    ));
    let stats = allocation_counter::measure(|| {
        assert!(encode_brewing_snapshot(&value).is_none());
    });
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
}

#[test]
fn rejected_exact_lock_composite_lists_stream_without_owned_nbt() {
    use foton_registry::data_components::components::CustomModelData;
    init_vanilla_registry();
    let book = WrittenBookContent::new(
        Filterable::pass_through("title".to_owned()),
        "author".to_owned(),
        0,
        (0..80)
            .map(|_| Filterable::pass_through(TextComponent::plain("x".repeat(30000))))
            .collect(),
        false,
    )
    .expect("book");
    let mut exceeded = Vec::new();
    for (name, data) in [
        ("written_book_content", ComponentData::new(book)),
        (
            "firework_explosion",
            ComponentData::new(FireworkExplosion::new(
                FireworkExplosionShape::Star,
                vec![0; 300_000],
                vec![],
                false,
                false,
            )),
        ),
        (
            "custom_model_data",
            ComponentData::new(CustomModelData::new(
                vec![0.; 300_000],
                vec![],
                vec![],
                vec![],
            )),
        ),
    ] {
        let entry = REGISTRY
            .data_components
            .by_key(&Identifier::vanilla_static(name))
            .expect("component");
        let exact = DataComponentExactPredicate::new(vec![(entry, data)]).expect("exact");
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = LockCode::new(ItemPredicate::new(
            None,
            IntBounds::ANY,
            DataComponentMatchers::new(exact, vec![]).expect("matcher"),
        ));
        let stats =
            allocation_counter::measure(|| assert!(encode_brewing_snapshot(&value).is_none()));
        eprintln!("exact {name}: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            exceeded.push((name, stats));
        }
    }
    assert!(exceeded.is_empty(), "{exceeded:?}");
}

#[test]
fn exact_lock_lore_preserves_persistent_shapes() {
    init_vanilla_registry();
    let entry = REGISTRY
        .data_components
        .by_key(&Identifier::vanilla_static("lore"))
        .expect("lore registered");
    let mut styled = TextComponent::plain("styled");
    styled.format.bold = Some(true);
    for lines in [
        vec![],
        vec![TextComponent::plain("plain")],
        vec![TextComponent::plain("plain"), styled],
    ] {
        let exact = DataComponentExactPredicate::new(vec![(
            entry,
            ComponentData::new(ItemLore::new(lines).expect("lore")),
        )])
        .expect("persistent lore");
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = LockCode::new(ItemPredicate::new(
            None,
            IntBounds::ANY,
            DataComponentMatchers::new(exact, vec![]).expect("matcher"),
        ));
        let mut expected = Vec::new();
        value
            .lock
            .write(&mut expected)
            .expect("ordinary persistent encoding");
        let mut bounded = Vec::new();
        value
            .lock
            .write_bounded(65536, &mut bounded)
            .expect("bounded encoding");
        assert_eq!(bounded, expected, "bounded persistent codec shape changed");
        let bytes = encode_brewing_snapshot(&value).expect("snapshot");
        let decoded = decode_brewing_snapshot(&bytes).expect("decode");
        assert_eq!(decoded.lock, value.lock);
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one parity test covers the persistent wire shape of all exact composite codecs"
)]
fn bounded_exact_composites_preserve_their_persistent_wire_shape() {
    use foton_registry::data_components::components::{
        ChargedProjectiles, CustomModelData, SulfurCubeContent, UseRemainder, WritableBookContent,
    };
    init_vanilla_registry();
    let explosion = || {
        FireworkExplosion::new(
            FireworkExplosionShape::Star,
            vec![0x0012_3456],
            vec![0x00ab_cdef],
            true,
            true,
        )
    };
    for (name, value) in [
        (
            "written_book_content",
            ComponentData::new(
                WrittenBookContent::new(
                    Filterable::new("raw".to_owned(), Some("filtered".to_owned())),
                    "author".to_owned(),
                    2,
                    vec![Filterable::new(
                        TextComponent::plain("page"),
                        Some(TextComponent::plain("filtered page")),
                    )],
                    true,
                )
                .expect("book"),
            ),
        ),
        (
            "writable_book_content",
            ComponentData::new(
                WritableBookContent::new(vec![Filterable::new(
                    "raw".to_owned(),
                    Some("filtered".to_owned()),
                )])
                .expect("book"),
            ),
        ),
        ("firework_explosion", ComponentData::new(explosion())),
        (
            "fireworks",
            ComponentData::new(Fireworks::new(128, vec![explosion()]).expect("fireworks")),
        ),
        (
            "custom_model_data",
            ComponentData::new(CustomModelData::new(
                vec![1.5],
                vec![true, false],
                vec!["text".to_owned()],
                vec![0x0012_3456],
            )),
        ),
        (
            "bundle_contents",
            ComponentData::new(BundleContents::new(vec![populated_template(false)])),
        ),
        (
            "charged_projectiles",
            ComponentData::new(
                ChargedProjectiles::new(vec![populated_template(false)]).expect("projectiles"),
            ),
        ),
        (
            "use_remainder",
            ComponentData::new(UseRemainder::new(populated_template(false))),
        ),
        (
            "sulfur_cube_content",
            ComponentData::new(SulfurCubeContent::new(populated_template(false))),
        ),
        (
            "container",
            ComponentData::new(
                ItemContainerContents::new(vec![None, Some(populated_template(false))])
                    .expect("container"),
            ),
        ),
    ] {
        let entry = REGISTRY
            .data_components
            .by_key(&Identifier::vanilla_static(name))
            .expect("component");
        let exact = DataComponentExactPredicate::new(vec![(entry, value)]).expect("exact");
        let lock = LockCode::new(ItemPredicate::new(
            None,
            IntBounds::ANY,
            DataComponentMatchers::new(exact, vec![]).expect("matcher"),
        ));
        let mut expected = Vec::new();
        lock.write(&mut expected).expect("persistent encoding");
        let mut actual = Vec::new();
        lock.write_bounded(65536, &mut actual)
            .expect("bounded encoding");
        if matches!(
            name,
            "bundle_contents"
                | "charged_projectiles"
                | "use_remainder"
                | "sulfur_cube_content"
                | "container"
        ) {
            // Persistent template patches now have a prescribed lexical order.
            // Compare ordinary bytes after applying only that intentional change.
            let tag = read_tag(&mut Cursor::new(expected.as_slice())).expect("ordinary NBT");
            let mut canonical = Vec::new();
            canonical_template_fields(tag).write(&mut canonical);
            assert_eq!(actual, canonical, "{name}");
        } else {
            assert_eq!(actual, expected, "{name}");
        }
        assert_eq!(
            LockCode::read(&mut Cursor::new(actual.as_slice())).expect("decode"),
            lock
        );
    }
}

#[test]
fn large_streamed_text_uses_capped_geometric_growth() {
    init_vanilla_registry();
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(CUSTOM_NAME, TextComponent::plain("x".repeat(60000)));
    let mut encoded = None;
    let stats = allocation_counter::measure(|| {
        encoded = encode_brewing_item(&item);
    });
    let encoded = encoded.expect("valid large text");
    eprintln!(
        "large streamed text: {stats:?}, capacity={}",
        encoded.capacity()
    );
    assert!(encoded.capacity() <= MAX_BREWING_ITEM_BYTES);
    assert!(stats.bytes_max < 1 << 20, "{stats:?}");
    assert!(
        stats.count_total <= 32,
        "streaming must need logarithmic growth: {stats:?}"
    );
    assert_eq!(
        read_brewing_item(&encoded)
            .expect("decode")
            .get(CUSTOM_NAME),
        item.get(CUSTOM_NAME)
    );
}

pub(super) fn populated_template(reverse: bool) -> ItemStackTemplate {
    let mut patch = DataComponentPatch::new();
    if reverse {
        // Grow, then clear the map to retain a different construction capacity.
        for name in [
            "lore",
            "custom_name",
            "item_name",
            "damage",
            "max_damage",
            "unbreakable",
            "glider",
            "repair_cost",
            "rarity",
            "max_stack_size",
            "enchantments",
            "dyed_color",
        ] {
            assert!(patch.remove_raw(Identifier::vanilla_static(name)));
        }
        for name in [
            "lore",
            "custom_name",
            "item_name",
            "damage",
            "max_damage",
            "unbreakable",
            "glider",
            "repair_cost",
            "rarity",
            "max_stack_size",
            "enchantments",
            "dyed_color",
        ] {
            patch.clear_key(&Identifier::vanilla_static(name));
        }
        patch.remove(ITEM_NAME);
        patch.remove(POTION_CONTENTS);
        patch.set(
            LORE,
            ItemLore::new(vec![TextComponent::plain("lore")]).expect("lore"),
        );
        patch.set(CUSTOM_NAME, TextComponent::plain("name"));
    } else {
        patch.set(CUSTOM_NAME, TextComponent::plain("name"));
        patch.set(
            LORE,
            ItemLore::new(vec![TextComponent::plain("lore")]).expect("lore"),
        );
        patch.remove(POTION_CONTENTS);
        patch.remove(ITEM_NAME);
    }
    ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch).expect("template")
}

#[test]
fn recursive_patches_are_canonical_across_construction_histories() {
    fn patch(cursor: &mut Cursor<&[u8]>) {
        let added = VarInt::read(cursor).expect("added").0;
        let removed = VarInt::read(cursor).expect("removed").0;
        for (count, has_value) in [(added, true), (removed, false)] {
            let mut previous = None;
            for _ in 0..count {
                let id = VarInt::read(cursor).expect("component id").0;
                assert!(
                    previous.is_none_or(|prev| prev < id),
                    "nested IDs are not sorted"
                );
                previous = Some(id);
                if has_value {
                    let entry = REGISTRY.data_components.by_id(id as usize).expect("entry");
                    if entry.key == Identifier::vanilla_static("bundle_contents") {
                        assert_eq!(VarInt::read(cursor).expect("one child").0, 1);
                        VarInt::read(cursor).expect("item id");
                        VarInt::read(cursor).expect("item count");
                        patch(cursor);
                    } else {
                        entry.read_network(cursor).expect("component");
                    }
                }
            }
        }
    }

    init_vanilla_registry();
    let encode = |reverse| {
        let mut patch = DataComponentPatch::new();
        patch.set(
            BUNDLE_CONTENTS,
            BundleContents::new(vec![populated_template(reverse)]),
        );
        patch.set(CUSTOM_NAME, TextComponent::plain("outer"));
        let template =
            ItemStackTemplate::try_with_count_and_patch(&vanilla_items::BUNDLE, 1, patch)
                .expect("outer template");
        let mut item = ItemStack::new(&vanilla_items::BUNDLE);
        item.set(BUNDLE_CONTENTS, BundleContents::new(vec![template]));
        encode_brewing_item(&item).expect("encode")
    };
    let first = encode(false);
    let second = encode(true);
    assert_eq!(
        first, second,
        "nested encoding must not depend on map capacity or insertion order"
    );

    let mut cursor = Cursor::new(first.as_slice());
    VarInt::read(&mut cursor).expect("count");
    VarInt::read(&mut cursor).expect("id");
    patch(&mut cursor);
    let decoded = read_brewing_item(&first).expect("decode");
    assert_eq!(encode_brewing_item(&decoded).expect("reencode"), first);
}

#[test]
fn nested_malicious_counts_share_one_decode_allocation_budget() {
    init_vanilla_registry();
    let bundle_id = REGISTRY
        .data_components
        .id_from_key(&Identifier::vanilla_static("bundle_contents"))
        .expect("bundle component") as i32;
    for (declared_count, leading_child) in
        [(4096, false), (4681, false), (4096, true), (4681, true)]
    {
        let mut bytes = Vec::new();
        for value in [1, vanilla_items::BUNDLE.id() as i32, 1, 0, bundle_id] {
            VarInt(value).write(&mut bytes).expect("envelope");
        }
        for _ in 0..12 {
            VarInt(declared_count).write(&mut bytes).expect("count");
            if leading_child {
                for value in [vanilla_items::STONE.id() as i32, 1, 0, 0] {
                    VarInt(value)
                        .write(&mut bytes)
                        .expect("completed first child");
                }
            }
            for value in [vanilla_items::BUNDLE.id() as i32, 1, 1, 0, bundle_id] {
                VarInt(value).write(&mut bytes).expect("first-child chain");
            }
        }
        // A terminal invalid item followed by padding makes every ancestor count
        // fit the same remaining suffix before any ancestor can finish a child.
        VarInt(declared_count)
            .write(&mut bytes)
            .expect("last count");
        VarInt(i32::MAX).write(&mut bytes).expect("invalid item");
        bytes.resize(bytes.len() + 20000, 0);
        let stats = allocation_counter::measure(|| {
            assert!(read_brewing_item(&bytes).is_none());
        });
        eprintln!("nested counts={declared_count}, leading_child={leading_child}: {stats:?}");
        assert!(
            stats.bytes_max <= 1 << 20,
            "shared recursive budget exceeded: {stats:?}"
        );
    }
}

fn canonical_template_fields(tag: NbtTag) -> NbtTag {
    use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
    match tag {
        NbtTag::Compound(value) => {
            let template = value.get("id").is_some() && value.get("components").is_some();
            let mut result = NbtCompound::new();
            for (key, value) in value.iter() {
                let mut child = canonical_template_fields(value.clone());
                if template && key.to_str() == "components" {
                    let entries = child.compound().expect("patch map");
                    let mut fields: Vec<_> = entries.iter().collect();
                    fields.sort_by(|(a, _), (b, _)| a.to_str().cmp(&b.to_str()));
                    let mut sorted = NbtCompound::new();
                    for (key, value) in fields {
                        sorted.insert(key.to_owned(), value.clone());
                    }
                    child = NbtTag::Compound(sorted);
                }
                result.insert(key.to_owned(), child);
            }
            NbtTag::Compound(result)
        }
        NbtTag::List(NbtList::Compound(values)) => NbtTag::List(NbtList::Compound(
            values
                .into_iter()
                .map(
                    |value| match canonical_template_fields(NbtTag::Compound(value)) {
                        NbtTag::Compound(value) => value,
                        _ => unreachable!("compound stays a compound"),
                    },
                )
                .collect(),
        )),
        value => value,
    }
}
