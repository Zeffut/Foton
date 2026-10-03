use super::{MenuItemBatch, MenuItemBatchError, MenuItemBatchStatus, MenuSlotWrite};
use std::sync::Arc;
use std::sync::{
    Weak,
    atomic::{AtomicUsize, Ordering},
};

use foton_registry::{item_stack::ItemStack, vanilla_items, vanilla_menu_types};
use foton_utils::locks::IntoShared as _;

use crate::{
    inventory::{
        container::{Container as _, SimpleContainer},
        menu::{MenuBuilder, kinds::BasicKind},
    },
    test_support::{TestPlayerBuilder, fresh_test_world},
};

#[test]
fn borrowed_menu_read_refusal_preserves_live_and_dispatch_sources() {
    let world = fresh_test_world("borrowed_item_menu");
    let player = TestPlayerBuilder::new(world, "ItemReader", 1).build();
    player.set_client_loaded(true);
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set_opaque_nbt(Some("owned source requiring preflight".to_owned()));
    let container = SimpleContainer::from_items(vec![item; 9]).into_shared();
    let menu_container = Arc::clone(&container);
    player.open_menu("Borrowed source", move |context| {
        let mut builder = MenuBuilder::new(&vanilla_menu_types::GENERIC_9X1, context.container_id);
        builder.section(menu_container, 9);
        builder.player_inventory(&context.player.inventory);
        builder.build(BasicKind {})
    });
    let live_address = container
        .lock()
        .get_item(0)
        .opaque_nbt()
        .expect("source")
        .as_ptr();
    assert_eq!(
        player.with_open_container_item(0, |stack| {
            assert_eq!(
                stack.opaque_nbt().expect("borrowed source").as_ptr(),
                live_address,
                "read must not clone before the callback can reject it"
            );
            Err::<(), _>("source refused")
        }),
        Some(Err("source refused"))
    );
    assert_eq!(
        container.lock().get_item(0).opaque_nbt(),
        Some("owned source requiring preflight")
    );

    let menu = player
        .take_open_menu_for_callback(None)
        .ok()
        .expect("detach real menu");
    let dispatch_address = {
        let state = player.open_menu.lock();
        let super::OpenMenuReadSource::Snapshot(items) =
            &state.dispatch.as_ref().expect("dispatch").reads
        else {
            panic!("legacy callback uses snapshot");
        };
        items[0].opaque_nbt().expect("dispatch source").as_ptr()
    };
    assert_eq!(
        player.with_open_container_item(0, |stack| {
            assert_eq!(
                stack.opaque_nbt().expect("borrowed snapshot").as_ptr(),
                dispatch_address
            );
            Err::<(), _>("snapshot refused")
        }),
        Some(Err("snapshot refused"))
    );
    assert!(player.with_open_container_item(9, |_| ()).is_none());
    assert_eq!(
        player.with_open_container_item(0, |stack| stack.count),
        Some(1)
    );
    player.finish_open_menu_callback(menu);
    assert_eq!(
        player.with_open_container_item(0, |stack| stack.count),
        Some(1)
    );
    player.close_container();
}

struct TrackedBatch {
    writes: Vec<MenuSlotWrite>,
    size: Option<usize>,
    player: Weak<crate::player::Player>,
    drops: Arc<AtomicUsize>,
}
impl MenuItemBatch for TrackedBatch {
    fn writes(&self) -> &[MenuSlotWrite] {
        &self.writes
    }
    fn expected_size(&self) -> Option<usize> {
        self.size
    }
    fn take_writes(&mut self) -> Vec<MenuSlotWrite> {
        std::mem::take(&mut self.writes)
    }
}
impl Drop for TrackedBatch {
    fn drop(&mut self) {
        if let Some(player) = self.player.upgrade() {
            assert!(
                player.open_menu.try_lock().is_some(),
                "operation owner dropped under menu mutex"
            );
        }
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn full_menu_batch_replays_on_same_size_current_target_and_refuses_changed_size() {
    for target_size in [9, 18] {
        let world = fresh_test_world(if target_size == 9 {
            "full_item_replay_same"
        } else {
            "full_item_replay_changed"
        });
        let player = TestPlayerBuilder::new(world, "FullBatch", 3).build();
        player.set_client_loaded(true);
        let original = SimpleContainer::from_items(vec![ItemStack::new(&vanilla_items::STONE); 9])
            .into_shared();
        let source = Arc::clone(&original);
        player.open_menu("Original", move |context| {
            let mut builder =
                MenuBuilder::new(&vanilla_menu_types::GENERIC_9X1, context.container_id);
            builder.section(source, 9);
            builder.player_inventory(&context.player.inventory);
            builder.build(BasicKind {})
        });
        let menu = player
            .take_open_menu_for_callback(None)
            .ok()
            .expect("dispatch");
        assert!(player.set_open_container_item(0, ItemStack::new(&vanilla_items::DIAMOND)));
        let target =
            SimpleContainer::from_items(vec![ItemStack::new(&vanilla_items::STONE); target_size])
                .into_shared();
        let replacement = Arc::clone(&target);
        player.open_menu("Replacement", move |context| {
            let kind = if target_size == 9 {
                &vanilla_menu_types::GENERIC_9X1
            } else {
                &vanilla_menu_types::GENERIC_9X2
            };
            let mut builder = MenuBuilder::new(kind, context.container_id);
            builder.section(replacement, target_size);
            builder.player_inventory(&context.player.inventory);
            builder.build(BasicKind {})
        });
        let drops = Arc::new(AtomicUsize::new(0));
        let batch = Box::new(TrackedBatch {
            writes: (0..9)
                .map(|index| MenuSlotWrite {
                    index,
                    stack: ItemStack::new(&vanilla_items::DIAMOND),
                })
                .collect(),
            size: Some(9),
            player: Arc::downgrade(&player),
            drops: Arc::clone(&drops),
        });
        assert_eq!(
            player.set_open_container_items(batch),
            Ok(MenuItemBatchStatus::Queued)
        );
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        player.finish_open_menu_callback(menu);
        assert!(
            original.lock().get_item(0).is(&vanilla_items::DIAMOND),
            "first set precedes replacement"
        );
        let expected = if target_size == 9 {
            &*vanilla_items::DIAMOND
        } else {
            &*vanilla_items::STONE
        };
        assert!(
            target.lock().items().iter().all(|item| item.is(expected)),
            "full batch must apply only to a same-size current target"
        );
        assert_eq!(
            drops.load(Ordering::SeqCst),
            1,
            "replayed or rejected owner releases exactly once"
        );
        player.close_container();
    }
}

#[test]
fn menu_batches_validate_before_queue_and_keep_owners_until_replay_or_terminal_discard() {
    let world = fresh_test_world("owned_item_menu");
    let player = TestPlayerBuilder::new(world, "ItemBatch", 2).build();
    player.set_client_loaded(true);
    let storage =
        SimpleContainer::from_items(vec![ItemStack::new(&vanilla_items::STONE); 9]).into_shared();
    let menu_storage = Arc::clone(&storage);
    player.open_menu("Owned items", move |context| {
        let mut builder = MenuBuilder::new(&vanilla_menu_types::GENERIC_9X1, context.container_id);
        builder.section(menu_storage, 9);
        builder.player_inventory(&context.player.inventory);
        builder.build(BasicKind {})
    });
    let drops = Arc::new(AtomicUsize::new(0));
    let batch = |indices: Vec<usize>, size| {
        Box::new(TrackedBatch {
            writes: indices
                .into_iter()
                .map(|index| MenuSlotWrite {
                    index,
                    stack: ItemStack::new(&vanilla_items::DIAMOND),
                })
                .collect(),
            size,
            player: Arc::downgrade(&player),
            drops: Arc::clone(&drops),
        }) as Box<dyn MenuItemBatch>
    };
    assert_eq!(
        player.set_open_container_items(batch(vec![0, 9, 1], None)),
        Err(MenuItemBatchError::InvalidSlot)
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(
        storage
            .lock()
            .items()
            .iter()
            .all(|item| item.is(&vanilla_items::STONE))
    );
    let menu = player
        .take_open_menu_for_callback(None)
        .ok()
        .expect("legacy detach");
    assert_eq!(
        player.set_open_container_items(batch(vec![0, 0], None)),
        Err(MenuItemBatchError::DuplicateSlot)
    );
    assert!(
        player
            .open_menu
            .lock()
            .dispatch
            .as_ref()
            .expect("dispatch")
            .actions
            .is_empty()
    );
    assert_eq!(
        player.set_open_container_items(batch(vec![0, 1], None)),
        Ok(MenuItemBatchStatus::Queued)
    );
    assert_eq!(
        drops.load(Ordering::SeqCst),
        2,
        "queued operation must retain ownership"
    );
    assert_eq!(
        player.with_open_container_item(0, |item| item.item()),
        Some(&*vanilla_items::STONE)
    );
    player.finish_open_menu_callback(menu);
    assert_eq!(drops.load(Ordering::SeqCst), 3);
    assert!(storage.lock().get_item(0).is(&vanilla_items::DIAMOND));
    assert!(storage.lock().get_item(1).is(&vanilla_items::DIAMOND));
    assert_eq!(
        player.set_open_container_items(batch(vec![0], Some(18))),
        Err(MenuItemBatchError::SizeMismatch)
    );
    let menu = player
        .take_open_menu_for_callback(None)
        .ok()
        .expect("terminal detach");
    assert_eq!(
        player.set_open_container_items(batch(vec![2], None)),
        Ok(MenuItemBatchStatus::Queued)
    );
    let _ = player.remove_all_menus();
    assert_eq!(drops.load(Ordering::SeqCst), 4);
    player.finish_open_menu_callback(menu);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        5,
        "terminal discard releases outside menu lock"
    );
    assert!(storage.lock().get_item(2).is(&vanilla_items::STONE));
}
