use super::*;
use crate::data_components::PersistentValidationScope;
use crate::item_predicate::LockCode;
use crate::{REGISTRY, RegistryExt, init_vanilla_registry};
use foton_utils::{
    Identifier,
    serial::{ReadFrom, budget::DecodeBudget},
};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use std::{cell::Cell, io::Cursor};

thread_local! { static VISITS: Cell<usize> = const { Cell::new(0) }; }
pub(super) fn record_visit() {
    VISITS.with(|count| count.set(count.get() + 1));
}

fn chain(depth: usize, template: bool) -> NbtTag {
    let mut tag = NbtTag::Compound(NbtCompound::new());
    for _ in 0..depth {
        let mut exact = NbtCompound::new();
        if template {
            let mut patch = NbtCompound::new();
            patch.insert("minecraft:can_break", tag);
            let mut item = NbtCompound::new();
            item.insert("id", "minecraft:stone");
            item.insert("components", patch);
            exact.insert("minecraft:use_remainder", item);
        } else {
            exact.insert("minecraft:lock", tag);
        }
        let mut parent = NbtCompound::new();
        parent.insert("components", exact);
        tag = NbtTag::Compound(parent);
    }
    tag
}

#[test]
fn brewing_task12_persistent_lock_visits_each_recursive_descendant_once() {
    init_vanilla_registry();
    for template in [false, true] {
        for depth in [8, 48] {
            let tag = chain(depth, template);
            let mut bytes = Vec::new();
            tag.write(&mut bytes);
            VISITS.with(|count| count.set(0));
            let lock = DecodeBudget::new(512 * 1024)
                .decode(|| LockCode::read(&mut Cursor::new(bytes.as_slice())))
                .expect("lock under shared quota");
            let visits = VISITS.with(Cell::get);
            eprintln!("task12 persistent visits template={template} depth={depth}: {visits}");
            assert_eq!(visits, depth * if template { 2 } else { 1 });
            let mut encoded = Vec::new();
            lock.write_bounded(65536, &mut encoded)
                .expect("canonical lock");
            assert_eq!(encoded, bytes);
            // The public registered owned adapter also retains a single shared allowance.
            let entry = REGISTRY
                .data_components
                .by_key(&Identifier::vanilla_static("lock"))
                .expect("lock entry");
            let decoded = DecodeBudget::new(512 * 1024)
                .decode(|| {
                    entry
                        .read_nbt_owned(&tag)
                        .ok_or_else(|| Error::other("registered lock"))
                })
                .expect("registered persistent route");
            assert_eq!(decoded.downcast_ref::<LockCode>(), Some(&lock));
        }
    }
}

fn item(count: i32, id: &str) -> NbtCompound {
    let mut item = NbtCompound::new();
    item.insert("id", id);
    item.insert("count", count);
    item
}

fn compare(name: &'static str, tag: NbtTag) {
    let entry = REGISTRY
        .data_components
        .by_key(&Identifier::new("minecraft", name))
        .expect("component");
    let mut bytes = Vec::new();
    tag.write(&mut bytes);
    let borrowed =
        simdnbt::borrow::read_tag(&mut Cursor::new(bytes.as_slice())).expect("fixture NBT");
    let expected = DecodeBudget::new(512 * 1024)
        .decode(|| Ok(entry.read_nbt(borrowed.as_tag())))
        .expect("scope");
    let actual = DecodeBudget::new(512 * 1024)
        .decode(|| {
            let _scope = PersistentValidationScope::enter();
            Ok(entry.read_nbt_owned(&tag))
        })
        .expect("scope");
    assert_eq!(actual, expected, "{name}: {tag:?}");
}

#[test]
fn brewing_task12_owned_recursive_readers_match_all_five_registered_families() {
    init_vanilla_registry();
    for name in ["use_remainder", "sulfur_cube_content"] {
        compare(name, NbtTag::String("minecraft:stone".into()));
        for count in [-1, 0, 1, 99, 100] {
            compare(name, NbtTag::Compound(item(count, "minecraft:stone")));
        }
        compare(name, NbtTag::Compound(item(1, "minecraft:air")));
        compare(name, NbtTag::Compound(item(1, "test:missing")));
    }
    for name in ["bundle_contents", "charged_projectiles"] {
        compare(
            name,
            NbtTag::List(NbtList::String(vec!["minecraft:stone".into()])),
        );
        for count in [0, 1, 100] {
            compare(
                name,
                NbtTag::List(NbtList::Compound(vec![item(count, "minecraft:stone")])),
            );
        }
        compare(name, NbtTag::List(NbtList::Byte(vec![])));
        compare(name, NbtTag::List(NbtList::Byte(vec![1])));
        compare(name, NbtTag::List(NbtList::Empty));
    }
    for slot in [-1, 0, 2, 255, 256] {
        let mut entry = NbtCompound::new();
        entry.insert("slot", slot);
        entry.insert("item", item(1, "minecraft:stone"));
        compare("container", NbtTag::List(NbtList::Compound(vec![entry])));
    }
    compare("container", NbtTag::List(NbtList::Int(vec![])));
    compare("container", NbtTag::List(NbtList::Int(vec![1])));
    compare("container", NbtTag::List(NbtList::Empty));
    compare(
        "charged_projectiles",
        NbtTag::List(NbtList::String(vec!["stone".into(); 1025])),
    );
}
