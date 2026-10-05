use super::*;
use foton_core::block_entity::BlockEntity as _;
use foton_registry::{
    data_components::vanilla_components::{CUSTOM_NAME, GLIDER},
    item_stack::ItemStack,
    vanilla_blocks,
};
use simdnbt::borrow as bridge_nbt_borrow;
use simdnbt::owned as bridge_owned;
use std::io as bridge_io;
use std::ptr as bridge_ptr;
use std::{num::NonZeroUsize, sync::Arc};
use text_components::TextComponent;

fn load_record(jukebox: &JukeboxBlockEntity, item: &ItemStack) {
    let mut data = bridge_owned::NbtCompound::new();
    data.insert("RecordItem", item.to_nbt_tag_ref());
    let mut bytes = Vec::new();
    data.write(&mut bytes);
    let parsed = bridge_nbt_borrow::read_compound(&mut bridge_io::Cursor::new(bytes.as_slice()))
        .expect("record NBT");
    jukebox.load_additional(&parsed);
}

fn deep_stack() -> ItemStack {
    let mut text = TextComponent::plain("leaf");
    for _ in 0..128 {
        let mut parent = TextComponent::plain("parent");
        parent.children.push(text);
        text = parent;
    }
    let mut stack = ItemStack::new(&vanilla_items::WRITABLE_BOOK);
    stack.set(CUSTOM_NAME, text);
    stack
}

#[test]
fn lectern_and_jukebox_borrowed_capture_refuse_depth_and_recover() {
    let world = super::super::test_world::world();
    let lectern = LecternBlockEntity::new(
        Arc::downgrade(world),
        BlockPos::new(3, 70, 3),
        vanilla_blocks::LECTERN.default_state(),
    );
    let jukebox = JukeboxBlockEntity::new(
        Arc::downgrade(world),
        BlockPos::new(4, 70, 3),
        vanilla_blocks::JUKEBOX.default_state(),
    );
    let store = super::super::SnapshotStore::new(NonZeroUsize::new(8).expect("capacity"));
    lectern.set_book(deep_stack());
    load_record(&jukebox, &deep_stack());
    assert!(
        jukebox.read_item(|item| item.has(CUSTOM_NAME)),
        "real block-entity NBT ingress retains deep text"
    );
    let lectern_source = lectern.with_book(bridge_ptr::from_ref);
    let jukebox_source = jukebox.read_item(bridge_ptr::from_ref);
    assert!(matches!(
        lectern.with_book(|item| {
            assert_eq!(bridge_ptr::from_ref(item), lectern_source);
            store.capture(item)
        }),
        Err(ItemBridgeError::Depth(_))
    ));
    assert!(matches!(
        jukebox.read_item(|item| {
            assert_eq!(bridge_ptr::from_ref(item), jukebox_source);
            store.capture(item)
        }),
        Err(ItemBridgeError::Depth(_))
    ));
    assert!(lectern.with_book(|item| item.has(CUSTOM_NAME)));
    assert!(jukebox.read_item(|item| item.has(CUSTOM_NAME)));
    let mut book = ItemStack::new(&vanilla_items::WRITABLE_BOOK);
    book.set(GLIDER, ());
    lectern.set_book(book.clone());
    load_record(&jukebox, &book);
    assert_eq!(
        lectern
            .with_book(|item| store.capture(item))
            .expect("lectern recovery")
            .stack(),
        &book
    );
    assert_eq!(
        jukebox
            .read_item(|item| store.capture(item))
            .expect("jukebox recovery")
            .stack(),
        &book
    );
}
