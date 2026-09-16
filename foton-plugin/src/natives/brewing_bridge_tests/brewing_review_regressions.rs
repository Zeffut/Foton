use super::*;
use foton_registry::data_component_predicate::DataComponentExactPredicate;
use foton_registry::data_components::components::{
    BeehiveOccupant, Bees, BlockEntityData, CustomData, EntityData, ItemEnchantments,
};
use foton_registry::data_components::vanilla_components::{CAN_BREAK, CAN_PLACE_ON, ENCHANTMENTS};
use foton_registry::data_components::{ComponentData, ComponentEntryRef};
use foton_registry::item_predicate::{AdventureModePredicate, BlockPredicate, NbtPredicate};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use std::io::Cursor;

fn entry(name: &'static str) -> ComponentEntryRef {
    REGISTRY
        .data_components
        .by_key(&Identifier::vanilla_static(name))
        .expect("component")
}
fn envelope(name: &'static str, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for n in [
        1,
        vanilla_items::STONE.id() as i32,
        1,
        0,
        REGISTRY
            .data_components
            .id_from_key(&entry(name).key)
            .expect("id") as i32,
    ] {
        VarInt(n).write(&mut bytes).expect("envelope");
    }
    bytes.extend_from_slice(body);
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    bytes
}
fn matchers(values: Vec<(ComponentEntryRef, ComponentData)>) -> DataComponentMatchers {
    DataComponentMatchers::new(
        DataComponentExactPredicate::new(values).expect("exact"),
        vec![],
    )
    .expect("matchers")
}
fn lock(values: Vec<(ComponentEntryRef, ComponentData)>) -> LockCode {
    LockCode::new(ItemPredicate::new(None, IntBounds::ANY, matchers(values)))
}
fn snapshot_with_lock_tag(tag: NbtTag) -> Vec<u8> {
    let mut value = snapshot(vec![ItemStack::empty(); 5]);
    value.custom_name = None;
    let mut bytes = encode_brewing_snapshot(&value).expect("snapshot");
    let offset = custom_name_length_offset(&bytes) + 4;
    let mut replacement = Vec::new();
    tag.write(&mut replacement);
    replace_snapshot_blob(&mut bytes, offset, &replacement);
    bytes
}

#[path = "brewing_review_bounds.rs"]
mod bounds;
#[path = "brewing_review_canonical.rs"]
mod canonical;
#[path = "brewing_rereview.rs"]
mod rereview;
#[path = "brewing_rereview_errors.rs"]
mod rereview_errors;
#[path = "brewing_review_validation.rs"]
mod validation;

#[path = "brewing_union.rs"]
mod union;

#[path = "brewing_persistent_union.rs"]
mod persistent_union;

#[path = "task13_text.rs"]
mod task13_text;

#[path = "task13_writers.rs"]
mod task13_writers;

#[path = "task14_text.rs"]
mod task14_text;

#[path = "task14_registry.rs"]
mod task14_registry;
