//! Tests for the bundle's weight rule, insertion order and selection.

use std::sync::Arc;

use foton_registry::data_components::BundleContents;
use foton_registry::data_components::vanilla_components::BUNDLE_CONTENTS;
use foton_registry::item_stack::ItemStack;
use foton_registry::items::ItemRef;
use foton_registry::{ItemStackTemplate, init_vanilla_registry, vanilla_items};

use crate::behavior::init_behaviors;
use crate::behavior::items::{BundleItem, MutableBundleContents, can_item_be_in_bundle};
use crate::inventory::click::{Click, MouseButton};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::menu::kinds::BasicKind;
use crate::inventory::menu::{Menu, MenuBuilder};
use crate::player::Player;
use crate::test_support::{TestPlayerBuilder, fresh_test_world};
use foton_registry::vanilla_menu_types;
use foton_utils::locks::{IntoShared as _, Shared};

fn ready() {
    init_vanilla_registry();
    init_behaviors();
}

fn stack(item: ItemRef, count: i32) -> ItemStack {
    ItemStack::with_count(item, count)
}

fn empty_bundle() -> MutableBundleContents {
    MutableBundleContents::new(&BundleContents::empty())
}

fn contents_of(contents: &BundleContents) -> Vec<(String, i32)> {
    contents
        .items()
        .iter()
        .map(|item| (item.item().key.to_string(), item.count()))
        .collect()
}

#[test]
fn a_bundle_holds_one_full_stack_of_a_sixty_four_stackable_item() {
    ready();
    let mut contents = empty_bundle();

    let mut stone = stack(&vanilla_items::STONE, 64);
    assert_eq!(contents.try_insert(&mut stone), 64);
    assert!(stone.is_empty(), "the whole stack went in");

    let mut more = stack(&vanilla_items::STONE, 1);
    assert_eq!(
        contents.try_insert(&mut more),
        0,
        "a full bundle takes nothing more"
    );
    assert_eq!(more.count(), 1, "a refused insertion returns everything");
}

#[test]
fn a_partial_insertion_takes_only_what_fits_and_leaves_the_rest() {
    ready();
    let mut contents = empty_bundle();

    let mut first = stack(&vanilla_items::DIAMOND, 40);
    assert_eq!(contents.try_insert(&mut first), 40);

    let mut second = stack(&vanilla_items::STONE, 64);
    assert_eq!(
        contents.try_insert(&mut second),
        24,
        "only the remaining 24/64 of the bundle's capacity is free"
    );
    assert_eq!(second.count(), 40, "the rest stays in the player's hand");
}

#[test]
fn a_sixteen_stackable_item_weighs_four_times_as_much() {
    ready();
    let mut contents = empty_bundle();

    // Snowballs stack to 16, so each one is 1/16 of a bundle.
    let mut snowballs = stack(&vanilla_items::SNOWBALL, 16);
    assert_eq!(contents.try_insert(&mut snowballs), 16);

    let mut stone = stack(&vanilla_items::STONE, 1);
    assert_eq!(contents.try_insert(&mut stone), 0);
}

#[test]
fn an_unstackable_item_fills_the_bundle_on_its_own() {
    ready();
    let mut contents = empty_bundle();

    let mut sword = stack(&vanilla_items::DIAMOND_SWORD, 1);
    assert_eq!(contents.try_insert(&mut sword), 1);

    let mut stone = stack(&vanilla_items::STONE, 1);
    assert_eq!(
        contents.try_insert(&mut stone),
        0,
        "a single-stack item weighs a whole bundle"
    );
}

#[test]
fn the_last_stack_inserted_is_the_first_one_removed() {
    ready();
    let mut contents = empty_bundle();

    let mut stone = stack(&vanilla_items::STONE, 16);
    let mut diamonds = stack(&vanilla_items::DIAMOND, 16);
    contents.try_insert(&mut stone);
    contents.try_insert(&mut diamonds);

    let removed = contents.remove_one().expect("the bundle is not empty");
    assert!(
        removed.is(&vanilla_items::DIAMOND),
        "insertion pushes to the front, so extraction is last in first out"
    );
    let removed = contents.remove_one().expect("one stack is left");
    assert!(removed.is(&vanilla_items::STONE));
    assert!(contents.remove_one().is_none());
}

#[test]
fn matching_stacks_merge_and_move_to_the_front() {
    ready();
    let mut contents = empty_bundle();

    let mut diamonds = stack(&vanilla_items::DIAMOND, 8);
    let mut stone = stack(&vanilla_items::STONE, 8);
    let mut more_diamonds = stack(&vanilla_items::DIAMOND, 8);
    contents.try_insert(&mut diamonds);
    contents.try_insert(&mut stone);
    contents.try_insert(&mut more_diamonds);

    let frozen = contents.to_immutable();
    assert_eq!(frozen.size(), 2, "the two diamond stacks merged");
    let front = &frozen.items()[0];
    assert_eq!(front.item(), &*vanilla_items::DIAMOND);
    assert_eq!(front.count(), 16);
}

#[test]
fn removing_takes_the_selected_stack_and_then_clears_the_selection() {
    ready();
    let mut contents = empty_bundle();

    let mut stone = stack(&vanilla_items::STONE, 8);
    let mut diamonds = stack(&vanilla_items::DIAMOND, 8);
    contents.try_insert(&mut stone);
    contents.try_insert(&mut diamonds);

    // Index 1 is the stone: the diamonds went in later and sit at the front.
    contents.toggle_selected_item(1);
    let removed = contents.remove_one().expect("the selected stack");
    assert!(removed.is(&vanilla_items::STONE));

    assert_eq!(
        contents.to_immutable().selected_item_index(),
        BundleContents::NO_SELECTED_ITEM_INDEX,
        "removing clears the selection"
    );
    let removed = contents.remove_one().expect("the remaining stack");
    assert!(removed.is(&vanilla_items::DIAMOND));
}

#[test]
fn selecting_the_same_index_twice_clears_the_selection() {
    ready();
    let mut contents = empty_bundle();
    let mut stone = stack(&vanilla_items::STONE, 8);
    let mut diamonds = stack(&vanilla_items::DIAMOND, 8);
    contents.try_insert(&mut stone);
    contents.try_insert(&mut diamonds);

    contents.toggle_selected_item(1);
    assert_eq!(contents.to_immutable().selected_item_index(), 1);

    contents.toggle_selected_item(1);
    assert_eq!(
        contents.to_immutable().selected_item_index(),
        BundleContents::NO_SELECTED_ITEM_INDEX
    );
}

#[test]
fn an_out_of_range_selection_clears_instead_of_pointing_nowhere() {
    ready();
    let mut contents = empty_bundle();
    let mut stone = stack(&vanilla_items::STONE, 8);
    contents.try_insert(&mut stone);

    contents.toggle_selected_item(7);
    assert_eq!(
        contents.to_immutable().selected_item_index(),
        BundleContents::NO_SELECTED_ITEM_INDEX
    );

    contents.toggle_selected_item(-4);
    assert_eq!(
        contents.to_immutable().selected_item_index(),
        BundleContents::NO_SELECTED_ITEM_INDEX
    );
}

#[test]
fn a_shulker_box_never_goes_into_a_bundle() {
    ready();

    assert!(!can_item_be_in_bundle(&stack(
        &vanilla_items::SHULKER_BOX,
        1
    )));
    assert!(!can_item_be_in_bundle(&stack(
        &vanilla_items::RED_SHULKER_BOX,
        1
    )));
    assert!(can_item_be_in_bundle(&stack(&vanilla_items::CHEST, 1)));
    assert!(!can_item_be_in_bundle(&ItemStack::empty()));

    let mut contents = empty_bundle();
    let mut shulker = stack(&vanilla_items::SHULKER_BOX, 1);
    assert_eq!(contents.try_insert(&mut shulker), 0);
    assert_eq!(shulker.count(), 1);
}

#[test]
fn a_nested_bundle_pays_a_surcharge_on_top_of_what_it_holds() {
    ready();

    let mut inner = empty_bundle();
    let mut stone = stack(&vanilla_items::STONE, 32);
    inner.try_insert(&mut stone);

    let mut nested = stack(&vanilla_items::BUNDLE, 1);
    nested.set(BUNDLE_CONTENTS, inner.to_immutable());

    let mut outer = empty_bundle();
    assert_eq!(outer.try_insert(&mut nested), 1);

    // The inner bundle is half full and costs another 1/16 to carry, leaving
    // 7/16 -- that is 28 more stone at 1/64 each, and no more.
    let mut filler = stack(&vanilla_items::STONE, 64);
    assert_eq!(outer.try_insert(&mut filler), 28);
    assert_eq!(filler.count(), 36);
}

#[test]
fn two_bundles_holding_the_same_items_stack_whatever_is_selected() {
    ready();

    let items = vec![ItemStackTemplate::new(&vanilla_items::STONE)];
    let unselected = BundleContents::new(items.clone());
    let selected = BundleContents::with_selected_item(items, 0);

    assert_eq!(
        selected.selected_item_index(),
        0,
        "the selection is remembered"
    );
    assert_eq!(
        unselected, selected,
        "but it is not part of the component's identity"
    );

    let mut first = stack(&vanilla_items::BUNDLE, 1);
    first.set(BUNDLE_CONTENTS, unselected);
    let mut second = stack(&vanilla_items::BUNDLE, 1);
    second.set(BUNDLE_CONTENTS, selected);
    assert!(ItemStack::is_same_item_same_components(&first, &second));
}

#[test]
fn contents_survive_a_round_trip_through_the_component() {
    ready();

    let mut contents = empty_bundle();
    let mut stone = stack(&vanilla_items::STONE, 8);
    let mut diamonds = stack(&vanilla_items::DIAMOND, 3);
    contents.try_insert(&mut stone);
    contents.try_insert(&mut diamonds);
    contents.toggle_selected_item(1);

    let frozen = contents.to_immutable();
    let refrozen = MutableBundleContents::new(&frozen).to_immutable();

    assert_eq!(contents_of(&frozen), contents_of(&refrozen));
    assert_eq!(refrozen.selected_item_index(), 1);
    assert_eq!(refrozen.weight().ok(), frozen.weight().ok());
}

/// Vanilla parity: the nine slots of a `GENERIC_9X1` chest row, plus the
/// player's own thirty-six, which every menu of that type carries.
const CHEST_ROW_SLOTS: usize = 45;

/// Builds a chest-row menu whose first slot holds `first_slot`, with a player
/// to click it.
fn one_slot_menu(
    world_name: &'static str,
    first_slot: ItemStack,
) -> (Menu, Arc<Player>, Shared<SimpleContainer>) {
    let world = fresh_test_world(world_name);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "BundleTester", 1).build();

    let container = SimpleContainer::new(CHEST_ROW_SLOTS).into_shared();
    container.lock().set_item(0, first_slot);

    let mut builder = MenuBuilder::new(&vanilla_menu_types::GENERIC_9X1, 1);
    builder.section(container.clone(), CHEST_ROW_SLOTS);
    (builder.build(BasicKind {}), player, container)
}

fn bundle_holding(items: &[(ItemRef, i32)]) -> ItemStack {
    ready();
    let mut contents = empty_bundle();
    for (item, count) in items {
        let mut stack = stack(item, *count);
        contents.try_insert(&mut stack);
    }
    let mut bundle = stack(&vanilla_items::BUNDLE, 1);
    bundle.set(BUNDLE_CONTENTS, contents.to_immutable());
    bundle
}

#[test]
fn right_clicking_a_bundle_with_an_empty_cursor_pulls_one_stack_out() {
    ready();
    let (mut menu, player, container) = one_slot_menu(
        "bundle_extract",
        bundle_holding(&[(&vanilla_items::STONE, 8)]),
    );

    menu.clicked(
        Click::Pickup {
            slot: 0,
            button: MouseButton::Right,
        },
        &player,
    );

    let carried = menu.behavior().carried().clone();
    assert!(carried.is(&vanilla_items::STONE), "the stack came out");
    assert_eq!(carried.count(), 8);

    let bundle = container.lock().get_item(0).clone();
    let contents = bundle.get(BUNDLE_CONTENTS).expect("still a bundle");
    assert!(contents.is_empty(), "and the bundle kept nothing back");
}

#[test]
fn left_clicking_a_carried_bundle_onto_a_slot_pulls_the_slot_in() {
    ready();
    let (mut menu, player, container) =
        one_slot_menu("bundle_insert", stack(&vanilla_items::STONE, 32));
    *menu.behavior_mut().carried_mut() = bundle_holding(&[]);

    menu.clicked(
        Click::Pickup {
            slot: 0,
            button: MouseButton::Left,
        },
        &player,
    );

    assert!(
        container.lock().get_item(0).is_empty(),
        "the whole stack was transferred"
    );
    let carried = menu.behavior().carried().clone();
    let contents = carried.get(BUNDLE_CONTENTS).expect("still a bundle");
    assert_eq!(contents.items().len(), 1);
    assert_eq!(contents.items()[0].count(), 32);
}

#[test]
fn picking_a_bundle_up_clears_the_selection_it_was_left_with() {
    ready();
    let (mut menu, player, _container) = one_slot_menu(
        "bundle_selection_reset",
        bundle_holding(&[(&vanilla_items::STONE, 8), (&vanilla_items::DIAMOND, 8)]),
    );

    menu.set_selected_bundle_item_index(0, 1);

    // Vanilla's `overrideOtherStackedOnMe` clears the selection on this click
    // and then declines it, so the ordinary pickup still runs.
    menu.clicked(
        Click::Pickup {
            slot: 0,
            button: MouseButton::Left,
        },
        &player,
    );

    let carried = menu.behavior().carried().clone();
    let contents = carried
        .get(BUNDLE_CONTENTS)
        .expect("the bundle was picked up");
    assert_eq!(
        contents.selected_item_index(),
        BundleContents::NO_SELECTED_ITEM_INDEX,
        "an in-place selection change survives a declined click"
    );
}

#[test]
fn the_selection_packet_points_the_next_extraction_at_one_stack() {
    ready();
    let (mut menu, player, _container) = one_slot_menu(
        "bundle_selection_extract",
        bundle_holding(&[(&vanilla_items::STONE, 8), (&vanilla_items::DIAMOND, 8)]),
    );

    // Index 1 is the stone: the diamonds went in later and sit at the front.
    menu.set_selected_bundle_item_index(0, 1);
    menu.clicked(
        Click::Pickup {
            slot: 0,
            button: MouseButton::Right,
        },
        &player,
    );

    assert!(
        menu.behavior().carried().is(&vanilla_items::STONE),
        "the selected stack came out, not the front one"
    );
}

/// Keeps `BundleItem`'s static entry point exercised alongside the menu path.
#[test]
fn toggling_the_selection_on_a_stack_without_contents_does_nothing() {
    ready();
    let mut plain = stack(&vanilla_items::STONE, 1);
    BundleItem::toggle_selected_item(&mut plain, 0);
    assert!(plain.get(BUNDLE_CONTENTS).is_none());
}
