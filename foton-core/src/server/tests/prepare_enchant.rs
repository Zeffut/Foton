use super::*;
use crate::event::menus::PrepareItemEnchantEvent as LegacyPrepareItemEnchantEvent;
use crate::event::{EnchantmentOffer, EventPriority, PrepareItemEnchantEvent};
use crate::inventory::{
    click::{Click, MouseButton},
    menu::{
        Menu,
        kinds::{EnchantmentKind, enchantment},
    },
};
use foton_registry::{item_stack::ItemStack, vanilla_enchantments};
use foton_utils::types::GameType;

fn assert_creative_costs(player: &Arc<Player>, mut menu: Menu) {
    assert!(player.set_game_mode(GameType::Creative));
    player.set_enchantment_seed(1234);
    player.give_experience_levels(10);
    menu.on_open(player);
    assert!(menu.set_enchantment_item(0, ItemStack::new(&vanilla_items::DIAMOND_SWORD), player));
    assert!(menu.click_menu_button(player, 0));
    assert_eq!(player.experience.lock().level(), 9);
    let guard = menu.behavior().lock_all_containers();
    assert!(
        menu.behavior().slots()[0]
            .get_item(&guard)
            .has_any_enchantments()
    );
    assert!(menu.behavior().slots()[1].get_item(&guard).is_empty());
}

fn assert_stage(phase: usize, menu: &mut Menu, player: &Player) {
    let state = menu
        .kind()
        .downcast_ref::<EnchantmentKind>()
        .expect("enchantment kind")
        .view_state();
    if phase == 0 {
        assert_eq!(state.costs, [0; 3]);
        assert!(state.offers.iter().all(Option::is_none));
        assert!(!menu.click_menu_button(player, 0));
        return;
    }
    assert_eq!(state.costs[0], if phase == 1 { 9 } else { 2 });
    assert_eq!(
        state.offers[0].expect("plugin's offer").enchantment,
        &vanilla_enchantments::UNBREAKING
    );
    if phase == 1 {
        assert_eq!(state.seed, i32::MIN + 7);
        let guard = menu.behavior().lock_all_containers();
        assert!(
            menu.behavior().slots()[0]
                .get_item(&guard)
                .is(&vanilla_items::DIAMOND_SWORD)
        );
        assert_eq!(menu.behavior().slots()[1].get_item(&guard).count(), 3);
    }
}

fn modify_offer(
    event: &mut PrepareItemEnchantEvent,
    phase: usize,
    player: &Player,
    position: BlockPos,
) {
    assert_eq!(event.player_id, player.uuid());
    assert_eq!(event.position, position);
    assert_eq!(event.bonus, 0);
    assert_eq!(event.state.seed, player.enchantment_seed());
    let inventory = player.inventory.try_lock();
    assert!(
        inventory.is_some(),
        "a JVM callback must be able to access player inventory"
    );
    if phase == 0 {
        assert!(!event.cancelled);
        assert!(event.state.offers.iter().any(Option::is_some));
        event.cancelled = true;
        return;
    }
    if phase == 1 {
        event.state.seed = i32::MIN + 7;
        event.items[0] = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        event.items[1] = ItemStack::with_count(&vanilla_items::LAPIS_LAZULI, 3);
    } else {
        assert!(
            event.cancelled,
            "Paper initially cancels non-enchantable items"
        );
        event.cancelled = false;
    }
    event.state.apply_offers([
        Some(EnchantmentOffer {
            enchantment: &vanilla_enchantments::UNBREAKING,
            level: if phase == 1 { 3 } else { 1 },
            cost: if phase == 1 { 9 } else { 2 },
        }),
        None,
        None,
    ]);
}

#[test]
fn prepare_enchant_cancellation_offers_items_and_unlocked_player_inventory() {
    foton_registry::init_vanilla_registry();
    let world = fresh_test_world("prepare_enchant_event");
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    runtime.block_on(async {
        let storage_root = test_storage_root("prepare-enchant-event");
        let server = test_server(
            Arc::clone(&world),
            PermissionSubjectIndex::new(),
            &storage_root,
        )
        .await
        .expect("test server");
        server.attach_worlds();
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Enchanter", 1)
            .server(&server)
            .build();
        let phase = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let phase_for_event = Arc::clone(&phase);
        let calls_for_event = Arc::clone(&calls);
        let player_for_event = Arc::clone(&player);
        let position = BlockPos::new(8, 64, 8);
        let owner = Identifier::new_static("test", "prepare_enchant");
        // The old module path must subscribe to the same concrete event emitted
        // by the real menu, exactly once per changed nonempty input.
        server.events.listen::<LegacyPrepareItemEnchantEvent, _>(
            owner.clone(),
            EventPriority::Normal,
            true,
            move |event| {
                calls_for_event.fetch_add(1, Ordering::SeqCst);
                modify_offer(
                    event,
                    phase_for_event.load(Ordering::SeqCst),
                    &player_for_event,
                    position,
                );
            },
        );
        for stage in 0..3 {
            phase.store(stage, Ordering::SeqCst);
            let mut menu = enchantment(Arc::clone(&player.inventory), 1, position, &world);
            menu.on_open(&player);
            assert_eq!(
                calls.load(Ordering::SeqCst),
                stage,
                "empty input does not fire"
            );
            *menu.behavior_mut().carried_mut() = ItemStack::new(if stage == 2 {
                &vanilla_items::STONE
            } else if stage == 1 {
                &vanilla_items::IRON_SWORD
            } else {
                &vanilla_items::DIAMOND_SWORD
            });
            menu.clicked(
                Click::Pickup {
                    slot: 0,
                    button: MouseButton::Left,
                },
                &player,
            );
            assert_stage(stage, &mut menu, &player);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        server.events.forget(&owner);
        assert_creative_costs(
            &player,
            enchantment(Arc::clone(&player.inventory), 1, position, &world),
        );
        drop(server);
        fs::remove_dir_all(&storage_root)
            .await
            .expect("remove test storage");
    });
}

#[test]
fn closing_a_preparing_enchant_view_does_not_close_its_queued_replacement() {
    foton_registry::init_vanilla_registry();
    let world = fresh_test_world("prepare_enchant_replacement");
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    runtime.block_on(async {
        let storage_root = test_storage_root("prepare-enchant-replacement");
        let server = test_server(
            Arc::clone(&world),
            PermissionSubjectIndex::new(),
            &storage_root,
        )
        .await
        .expect("test server");
        server.attach_worlds();
        let player = TestPlayerBuilder::new(Arc::clone(&world), "ReentrantEnchanter", 1)
            .server(&server)
            .build();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_event = Arc::clone(&calls);
        let player_for_event = Arc::clone(&player);
        let owner = Identifier::new_static("test", "prepare_enchant_replacement");
        server
            .events
            .on::<PrepareItemEnchantEvent, _>(owner.clone(), move |event| {
                let player = &player_for_event;
                let first = calls_for_event.fetch_add(1, Ordering::SeqCst) == 0;
                assert_eq!(
                    player.enchantment_title(event.menu_id).as_deref(),
                    Some(if first { "Original" } else { "Replacement" }),
                    "title remains available while the menu is detached"
                );
                if first {
                    player.open_menu(TextComponent::plain("Replacement"), |context| {
                        enchantment(
                            Arc::clone(&context.player.inventory),
                            context.container_id,
                            BlockPos::new(16, 64, 16),
                            context.world,
                        )
                    });
                }
                assert!(player.close_enchantment_view(event.menu_id));
            });
        player.open_menu(TextComponent::plain("Original"), |context| {
            enchantment(
                Arc::clone(&context.player.inventory),
                context.container_id,
                BlockPos::new(8, 64, 8),
                context.world,
            )
        });
        let original = player.enchantment_view().expect("original table").0;
        assert!(player.set_enchantment_item(
            original,
            0,
            ItemStack::new(&vanilla_items::DIAMOND_SWORD)
        ));
        let replacement = player
            .enchantment_view()
            .expect("replacement remains open")
            .0;
        assert_ne!(original, replacement);
        assert_eq!(
            player.enchantment_title(replacement).as_deref(),
            Some("Replacement")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(player.set_enchantment_item(
            replacement,
            0,
            ItemStack::new(&vanilla_items::DIAMOND_SWORD)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(
            player.enchantment_view().is_none(),
            "a matching queued close still executes"
        );
        server.events.forget(&owner);
        drop(server);
        fs::remove_dir_all(&storage_root)
            .await
            .expect("remove test storage");
    });
}
