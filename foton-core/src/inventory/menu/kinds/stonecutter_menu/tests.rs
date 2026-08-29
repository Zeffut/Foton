use std::sync::Arc;

use super::{StonecutterKind, stonecutter};
use crate::{
    behavior::init_behaviors,
    entity::Entity as _,
    inventory::{
        click::{Click, MouseButton},
        lock::ContainerId,
        menu::Menu,
    },
    player::Player,
    test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk},
    world::World,
};
use foton_registry::{
    REGISTRY, init_vanilla_registry, item_stack::ItemStack, vanilla_blocks, vanilla_items,
};
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, ChunkPos, Downcast as _};
use glam::DVec3;

/// The input slot; the result is slot 1 and the player inventory follows.
const INPUT_SLOT: usize = 0;
const RESULT_SLOT: usize = 1;

/// Builds a world with a stonecutter and a player standing beside it.
fn stonecutter_world(name: &'static str) -> (Arc<World>, Arc<Player>, BlockPos) {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(name);
    let pos = BlockPos::new(0, 64, 0);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos,
        vanilla_blocks::STONECUTTER.default_state(),
        UpdateFlags::UPDATE_ALL,
    ));
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Cutter", 1).build();
    player.base().set_position_local(DVec3::new(0.5, 64.0, 0.5));
    (world, player, pos)
}

/// Returns the ids of the menu's own two containers.
fn container_ids(menu: &Menu) -> (ContainerId, ContainerId) {
    let kind = menu
        .kind()
        .downcast_ref::<StonecutterKind>()
        .expect("the stonecutter builder makes a stonecutter menu");
    (kind.input_id(), kind.result_id())
}

/// Puts `stack` into the stonecutter's input slot by carrying it and clicking.
fn place_input(menu: &mut Menu, player: &Arc<Player>, stack: ItemStack) {
    *menu.behavior_mut().carried_mut() = stack;
    menu.clicked(
        Click::Pickup {
            slot: INPUT_SLOT,
            button: MouseButton::Left,
        },
        player,
    );
}

/// Reads what is in one of the menu's containers.
fn item_in(menu: &Menu, container: ContainerId) -> ItemStack {
    menu.behavior()
        .lock_all_containers()
        .get(container)
        .expect("container is registered with the menu")
        .get_item(0)
        .clone()
}

/// The extracted data really does carry stonecutting recipes.
///
/// If this fails, everything below is testing an empty list rather than a
/// stonecutter -- the build script skipped these recipes entirely until now.
#[test]
fn andesite_has_stonecutting_recipes() {
    init_vanilla_registry();
    let recipes = REGISTRY
        .recipes
        .stonecutting_recipes_for(&ItemStack::new(&vanilla_items::ANDESITE));
    assert!(
        recipes.len() >= 3,
        "andesite should cut into at least a slab, stairs and a wall, got {}",
        recipes.len()
    );
}

/// Nothing comes out until the player picks a cut.
///
/// Vanilla parity: `setupRecipeList` resets the selection to -1, and an
/// unselected stonecutter shows an empty result even though the input matches
/// a dozen recipes.
#[test]
fn an_unselected_stonecutter_makes_nothing() {
    let (_world, player, pos) = stonecutter_world("stonecutter_unselected");
    let mut menu = stonecutter(Arc::clone(&player.inventory), 1, pos);
    let (input, result) = container_ids(&menu);

    place_input(
        &mut menu,
        &player,
        ItemStack::with_count(&vanilla_items::ANDESITE, 4),
    );

    assert!(!item_in(&menu, input).is_empty(), "the input went in");
    assert!(
        item_in(&menu, result).is_empty(),
        "no cut was chosen, so there should be nothing to take"
    );
}

/// Choosing a cut fills the result, and taking it spends one input.
#[test]
fn choosing_a_cut_fills_the_result_and_taking_it_spends_one() {
    let (_world, player, pos) = stonecutter_world("stonecutter_cut");
    let mut menu = stonecutter(Arc::clone(&player.inventory), 1, pos);
    let (input, result) = container_ids(&menu);

    place_input(
        &mut menu,
        &player,
        ItemStack::with_count(&vanilla_items::ANDESITE, 4),
    );

    let expected = REGISTRY
        .recipes
        .stonecutting_recipes_for(&ItemStack::new(&vanilla_items::ANDESITE))
        .first()
        .expect("andesite cuts into something")
        .result
        .to_item_stack();

    assert!(
        menu.click_menu_button(&player, 0),
        "the first cut is selectable"
    );

    let cut = item_in(&menu, result);
    assert!(
        cut.is(expected.item()),
        "the result should be the first cut, got {:?}",
        cut.item().key
    );

    menu.clicked(
        Click::Pickup {
            slot: RESULT_SLOT,
            button: MouseButton::Left,
        },
        &player,
    );

    assert_eq!(
        item_in(&menu, input).count(),
        3,
        "one block is spent per cut taken"
    );
    assert!(
        !item_in(&menu, result).is_empty(),
        "the result is rebuilt so the player can keep cutting"
    );
}

/// A button beyond the recipe list is ignored rather than clearing the choice.
///
/// Vanilla parity: `isValidRecipeIndex`. It matters because a click can race a
/// slot change, and a stale index must not wipe what the player picked.
#[test]
fn an_out_of_range_button_leaves_the_choice_alone() {
    let (_world, player, pos) = stonecutter_world("stonecutter_bad_button");
    let mut menu = stonecutter(Arc::clone(&player.inventory), 1, pos);
    let (_input, result) = container_ids(&menu);

    place_input(
        &mut menu,
        &player,
        ItemStack::with_count(&vanilla_items::ANDESITE, 4),
    );
    menu.click_menu_button(&player, 0);
    let chosen = item_in(&menu, result);
    assert!(!chosen.is_empty());

    menu.click_menu_button(&player, 9999);

    assert!(
        item_in(&menu, result).is(chosen.item()),
        "an impossible button should not change what is on offer"
    );
}

/// An input nothing cuts offers nothing.
#[test]
fn an_input_with_no_cuts_offers_none() {
    let (_world, player, pos) = stonecutter_world("stonecutter_no_recipe");
    let mut menu = stonecutter(Arc::clone(&player.inventory), 1, pos);
    let (_input, result) = container_ids(&menu);

    place_input(
        &mut menu,
        &player,
        ItemStack::with_count(&vanilla_items::DIAMOND, 4),
    );
    menu.click_menu_button(&player, 0);

    assert!(
        item_in(&menu, result).is_empty(),
        "a diamond does not go in a stonecutter"
    );
}
