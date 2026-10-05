use super::*;
use crate::event::{EventPriority, InventoryCloseEvent};
use crate::inventory::{
    container::{Container, SimpleContainer},
    menu::{MenuBuilder, kinds::BasicKind},
};
use foton_registry::{data_components::vanilla_components::GLIDER, vanilla_menu_types};
use foton_utils::locks::IntoShared as _;

#[test]
fn close_listener_reads_current_inserted_and_removed_top_items_once() {
    foton_registry::init_vanilla_registry();
    let world = fresh_test_world("close_current_contents");
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let storage = test_storage_root("close-current-contents");
        let server = test_server(Arc::clone(&world), PermissionSubjectIndex::new(), &storage)
            .await
            .expect("server");
        server.attach_worlds();
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Closer", 1)
            .server(&server)
            .build();
        let phase = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_player = Arc::clone(&player);
        let callback_phase = Arc::clone(&phase);
        let callback_calls = Arc::clone(&calls);
        server.events.listen::<InventoryCloseEvent, _>(
            Identifier::new_static("test", "close_contents"),
            EventPriority::Normal,
            true,
            move |_| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(callback_player.open_container_top_slot_count(), Some(9));
                let item = callback_player
                    .open_container_item(0)
                    .expect("close snapshot slot");
                if callback_phase.load(Ordering::SeqCst) == 0 {
                    assert!(item.is(&vanilla_items::DIAMOND));
                    assert_eq!(item.count(), 3);
                    assert!(item.has(GLIDER));
                } else {
                    assert!(item.is_empty(), "removed initial item must not reappear");
                }
            },
        );
        for stage in 0..2 {
            phase.store(stage, Ordering::SeqCst);
            let contents = SimpleContainer::new(9).into_shared();
            if stage == 1 {
                contents
                    .lock()
                    .set_item(0, ItemStack::new(&vanilla_items::STONE));
            }
            let inventory = Arc::clone(&player.inventory);
            player.open_menu("Close contents", move |context| {
                let mut builder =
                    MenuBuilder::new(&vanilla_menu_types::GENERIC_9X1, context.container_id);
                builder.section(contents, 9);
                builder.player_inventory(&inventory);
                builder.build(BasicKind {})
            });
            let mut changed = ItemStack::with_count(&vanilla_items::DIAMOND, 3);
            changed.set(GLIDER, ());
            assert!(player.set_open_container_item(
                0,
                if stage == 0 {
                    changed
                } else {
                    ItemStack::empty()
                }
            ));
            player.do_close_container();
            assert_eq!(calls.load(Ordering::SeqCst), stage + 1);
        }
    });
}
