use std::sync::Arc;

use glam::DVec3;
use steel_registry::item_stack::ItemStack;
use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos, Downcast as _};

use super::{CrafterKind, crafter};
use crate::behavior::init_behaviors;
use crate::block_entity::entities::{CRAFTER_SLOTS, CrafterBlockEntity};
use crate::block_entity::init_block_entities;
use crate::entity::Entity as _;
use crate::inventory::click::Click;
use crate::inventory::container::Container as _;
use crate::inventory::menu::Menu;
use crate::player::Player;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use crate::world::{LevelReader as _, World};

/// The first hotbar square in a crafter menu: nine grid slots, then the
/// twenty-seven main inventory slots, then the hotbar.
const FIRST_HOTBAR_SLOT: usize = CRAFTER_SLOTS + 27;

/// Runs `f` on the crafter at the world origin.
fn with_crafter<T>(world: &Arc<World>, f: impl FnOnce(&CrafterBlockEntity) -> T) -> T {
    let block_entity = world
        .get_block_entity(BlockPos::new(0, 64, 0))
        .expect("the crafter should have a block entity");
    let entity = block_entity
        .downcast_ref::<CrafterBlockEntity>()
        .expect("the block entity should be a crafter");
    f(entity)
}

fn test_crafter(key: &'static str) -> (Arc<World>, Arc<Player>, Menu) {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    let pos = BlockPos::new(0, 64, 0);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos,
        vanilla_blocks::CRAFTER.default_state(),
        UpdateFlags::UPDATE_ALL,
    ));
    let block_entity = world
        .get_block_entity(pos)
        .expect("the crafter should have a block entity");
    let entity = block_entity
        .downcast_ref::<CrafterBlockEntity>()
        .expect("the block entity should be a crafter");
    let container = entity.container_ref();
    let data = entity.data();

    let player = TestPlayerBuilder::new(Arc::clone(&world), "CrafterTester", 1).build();
    player.base().set_position_local(DVec3::new(0.5, 64.0, 0.5));

    let menu = crafter(Arc::clone(&player.inventory), 1, container, data);
    (world, player, menu)
}

/// The whole point of the menu: a player loads the grid by shift-clicking.
#[test]
fn a_shift_click_moves_an_item_into_the_grid() {
    let (world, player, mut menu) = test_crafter("crafter_menu_quick_move");
    player
        .inventory
        .lock()
        .set_item(0, ItemStack::new(&vanilla_items::OAK_LOG));

    menu.clicked(
        Click::QuickMove {
            slot: FIRST_HOTBAR_SLOT,
        },
        &player,
    );

    assert!(player.inventory.lock().get_item(0).is_empty());
    assert!(with_crafter(&world, |crafter| crafter.get_item(0)).is(&vanilla_items::OAK_LOG));
}

/// The preview is what the client draws in the output square; it is not a slot
/// anything can be taken from, so nothing else would notice if it stopped
/// updating.
#[test]
fn the_preview_shows_what_the_grid_would_make() {
    let (_world, player, mut menu) = test_crafter("crafter_menu_preview");
    player
        .inventory
        .lock()
        .set_item(0, ItemStack::new(&vanilla_items::OAK_LOG));

    menu.clicked(
        Click::QuickMove {
            slot: FIRST_HOTBAR_SLOT,
        },
        &player,
    );

    let kind = menu
        .kind()
        .downcast_ref::<CrafterKind>()
        .expect("the builder should make a crafter menu");
    let preview = kind.preview.clone();
    let guard = menu.behavior().lock_all_containers();
    let shown = guard
        .get(preview.container_id())
        .expect("the preview should be locked with the rest")
        .get_item(0)
        .clone();
    assert!(shown.is(&vanilla_items::OAK_PLANKS));
}

/// Switching a slot off is the one thing this menu does that no other does,
/// and it arrives as its own packet rather than as a click.
#[test]
fn a_slot_can_be_switched_off_and_back_on() {
    let (world, player, mut menu) = test_crafter("crafter_menu_slot_state");

    menu.set_slot_state(&player, 4, false);
    assert!(with_crafter(&world, |crafter| crafter.is_slot_disabled(4)));

    menu.set_slot_state(&player, 4, true);
    assert!(!with_crafter(&world, |crafter| crafter.is_slot_disabled(4)));
}

/// A slot with something in it stays on, or a player could hide an ingredient
/// the recipe is already using.
#[test]
fn a_filled_slot_refuses_to_be_switched_off() {
    let (world, player, mut menu) = test_crafter("crafter_menu_slot_state_filled");
    player
        .inventory
        .lock()
        .set_item(0, ItemStack::new(&vanilla_items::OAK_LOG));
    menu.clicked(
        Click::QuickMove {
            slot: FIRST_HOTBAR_SLOT,
        },
        &player,
    );

    menu.set_slot_state(&player, 0, false);

    assert!(!with_crafter(&world, |crafter| crafter.is_slot_disabled(0)));
}
