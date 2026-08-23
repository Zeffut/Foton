use std::sync::Arc;

use glam::DVec3;
use steel_registry::data_components::vanilla_components::BANNER_PATTERNS;
use steel_registry::item_stack::ItemStack;
use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos, Downcast as _};

use super::{LoomKind, loom};
use crate::behavior::init_behaviors;
use crate::entity::Entity as _;
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::lock::ContainerId;
use crate::inventory::menu::Menu;
use crate::inventory::slots::{
    LOOM_BANNER, LOOM_DYE, LOOM_PATTERN, PATTERN_NOT_SET, ResultHandler,
};
use crate::player::Player;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

const LOOM_POS: BlockPos = BlockPos::new(0, 64, 0);

fn test_loom(key: &'static str) -> (Arc<World>, Arc<Player>, Menu) {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(LOOM_POS));
    assert!(world.set_block(
        LOOM_POS,
        vanilla_blocks::LOOM.default_state(),
        UpdateFlags::UPDATE_ALL,
    ));

    let player = TestPlayerBuilder::new(Arc::clone(&world), "LoomTester", 1).build();
    player.base().set_position_local(DVec3::new(0.5, 64.0, 0.5));

    let menu = loom(Arc::clone(&player.inventory), 1, LOOM_POS);
    (world, player, menu)
}

/// Fills the loom's three input slots and recomputes.
fn load(menu: &mut Menu, player: &Player, banner: ItemStack, dye: ItemStack, pattern: ItemStack) {
    let input_id = input_container_id(menu);
    let mut guard = menu.behavior().lock_all_containers();
    {
        let container = guard
            .get_typed_mut::<SimpleContainer>(input_id)
            .expect("the input container should be locked");
        container.set_item(LOOM_BANNER, banner);
        container.set_item(LOOM_DYE, dye);
        container.set_item(LOOM_PATTERN, pattern);
    }
    menu.slots_changed(&mut guard, player);
}

fn input_container_id(menu: &Menu) -> ContainerId {
    let kind = menu
        .kind()
        .downcast_ref::<LoomKind>()
        .expect("the builder should make a loom menu");
    ResultHandler::dependencies(&kind.handler)[0].container_id()
}

fn result(menu: &Menu) -> ItemStack {
    let kind = menu
        .kind()
        .downcast_ref::<LoomKind>()
        .expect("the builder should make a loom menu");
    let result_id = ResultHandler::result_container(&kind.handler).container_id();
    let guard = menu.behavior().lock_all_containers();
    guard
        .get(result_id)
        .expect("the result container should be locked")
        .get_item(0)
        .clone()
}

fn layer_count(stack: &ItemStack) -> usize {
    stack
        .get(BANNER_PATTERNS)
        .map_or(0, |layers| layers.layers().len())
}

#[test]
fn a_banner_and_a_dye_alone_make_nothing_until_a_pattern_is_picked() {
    let (_world, player, mut menu) = test_loom("loom_menu_needs_a_pattern");

    load(
        &mut menu,
        &player,
        ItemStack::new(&vanilla_items::WHITE_BANNER),
        ItemStack::new(&vanilla_items::RED_DYE),
        ItemStack::empty(),
    );

    // Several patterns need no item, so none of them is picked for the player.
    assert!(result(&menu).is_empty());
}

#[test]
fn picking_a_pattern_stamps_a_layer_onto_the_banner() {
    let (_world, player, mut menu) = test_loom("loom_menu_pick_pattern");
    load(
        &mut menu,
        &player,
        ItemStack::new(&vanilla_items::WHITE_BANNER),
        ItemStack::new(&vanilla_items::RED_DYE),
        ItemStack::empty(),
    );

    assert!(menu.click_menu_button(&player, 0));

    let stamped = result(&menu);
    assert!(stamped.is(&vanilla_items::WHITE_BANNER));
    assert_eq!(layer_count(&stamped), 1);
}

/// A pattern item offering exactly one pattern picks it for the player, which
/// is what makes a banner-pattern item work without a click.
#[test]
fn a_pattern_item_needs_no_click() {
    let (_world, player, mut menu) = test_loom("loom_menu_pattern_item");

    load(
        &mut menu,
        &player,
        ItemStack::new(&vanilla_items::WHITE_BANNER),
        ItemStack::new(&vanilla_items::RED_DYE),
        ItemStack::new(&vanilla_items::FLOWER_BANNER_PATTERN),
    );

    assert_eq!(layer_count(&result(&menu)), 1);
}

#[test]
fn an_index_the_loom_does_not_offer_is_refused() {
    let (_world, player, mut menu) = test_loom("loom_menu_bad_index");
    load(
        &mut menu,
        &player,
        ItemStack::new(&vanilla_items::WHITE_BANNER),
        ItemStack::new(&vanilla_items::RED_DYE),
        ItemStack::new(&vanilla_items::FLOWER_BANNER_PATTERN),
    );

    assert!(!menu.click_menu_button(&player, 40));
}

/// Swapping the pattern item out from under a choice unselects it rather than
/// leaving the loom pointing at a pattern that is no longer on offer.
#[test]
fn changing_the_pattern_item_drops_a_selection_it_no_longer_offers() {
    let (_world, player, mut menu) = test_loom("loom_menu_swap_pattern");
    load(
        &mut menu,
        &player,
        ItemStack::new(&vanilla_items::WHITE_BANNER),
        ItemStack::new(&vanilla_items::RED_DYE),
        ItemStack::empty(),
    );
    assert!(menu.click_menu_button(&player, 4));

    load(
        &mut menu,
        &player,
        ItemStack::new(&vanilla_items::WHITE_BANNER),
        ItemStack::new(&vanilla_items::RED_DYE),
        ItemStack::new(&vanilla_items::FLOWER_BANNER_PATTERN),
    );

    // The one pattern the item offers is picked instead, not index 4.
    let kind = menu
        .kind()
        .downcast_ref::<LoomKind>()
        .expect("the builder should make a loom menu");
    assert_eq!(kind.handler.selected(), 0);
}

#[test]
fn an_empty_dye_slot_makes_nothing() {
    let (_world, player, mut menu) = test_loom("loom_menu_no_dye");

    load(
        &mut menu,
        &player,
        ItemStack::new(&vanilla_items::WHITE_BANNER),
        ItemStack::empty(),
        ItemStack::new(&vanilla_items::FLOWER_BANNER_PATTERN),
    );

    assert!(result(&menu).is_empty());
    let kind = menu
        .kind()
        .downcast_ref::<LoomKind>()
        .expect("the builder should make a loom menu");
    assert_eq!(kind.handler.selected(), PATTERN_NOT_SET);
}

/// Six layers is the limit, and a full banner offers nothing rather than
/// silently dropping the seventh.
#[test]
fn a_banner_with_six_layers_takes_no_more() {
    let (_world, player, mut menu) = test_loom("loom_menu_full_banner");
    let mut banner = ItemStack::new(&vanilla_items::WHITE_BANNER);

    for _ in 0..6 {
        load(
            &mut menu,
            &player,
            banner.clone(),
            ItemStack::new(&vanilla_items::RED_DYE),
            ItemStack::new(&vanilla_items::FLOWER_BANNER_PATTERN),
        );
        banner = result(&menu);
        assert!(!banner.is_empty(), "the loom should still be stamping");
    }
    assert_eq!(layer_count(&banner), 6);

    load(
        &mut menu,
        &player,
        banner,
        ItemStack::new(&vanilla_items::RED_DYE),
        ItemStack::new(&vanilla_items::FLOWER_BANNER_PATTERN),
    );

    assert!(result(&menu).is_empty());
}
