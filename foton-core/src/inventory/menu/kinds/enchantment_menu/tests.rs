use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_registry::{init_vanilla_registry, vanilla_items};
use foton_utils::BlockPos;
use foton_utils::random::Random as _;

use super::{EnchantmentKind, SLOT_ITEM, SLOT_LAPIS, enchantment};
use crate::player::Player;
use crate::player::player_inventory::MenuOpenContext;
use crate::test_support::{TestPlayerBuilder, fresh_test_world};

#[test]
fn offer_seed_addition_wraps_as_a_java_int_before_widening() {
    init_vanilla_registry();
    let item = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
    let (rolled, mut random) = EnchantmentKind::roll_offer(i32::MAX, 2, 30, &item);
    let (wrapped, mut expected) = EnchantmentKind::roll_offer(i32::MIN + 1, 0, 30, &item);
    assert_eq!(rolled, wrapped);
    assert_eq!(random.next_i32(), expected.next_i32());
}

#[test]
fn an_expired_view_cannot_read_write_or_close_a_reopened_table() {
    init_vanilla_registry();
    let world = fresh_test_world("enchantment_expired_view");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "ViewTester", 1).build();
    let open = |context: MenuOpenContext<'_>| {
        enchantment(
            Arc::clone(&context.player.inventory),
            context.container_id,
            BlockPos::new(0, 64, 0),
            context.world,
        )
    };
    player.open_menu(text_components::TextComponent::plain("First"), open);
    let (old, _, _, state) = player.enchantment_view().expect("first table");
    player.open_menu(text_components::TextComponent::plain("Second"), open);
    let (current, _, _, _) = player.enchantment_view().expect("second table");
    assert_ne!(old, current);
    assert!(player.enchantment_title(old).is_none());
    assert_eq!(player.enchantment_title(current).as_deref(), Some("Second"));
    assert!(player.enchantment_item(old, 0).is_none());
    assert!(!player.set_enchantment_view(old, state));
    assert!(!player.set_enchantment_item(old, 0, ItemStack::new(&vanilla_items::DIAMOND_SWORD)));
    assert!(!player.close_enchantment_view(old));
    assert_eq!(
        player.enchantment_view().expect("second table remains").0,
        current
    );
    assert!(
        player
            .enchantment_item(current, 0)
            .expect("current item")
            .is_empty()
    );
    assert!(player.set_enchantment_item(current, 0, ItemStack::new(&vanilla_items::DIAMOND_SWORD)));
    assert!(
        player
            .enchantment_view()
            .expect("updated offers")
            .3
            .offers
            .iter()
            .any(Option::is_some)
    );
    assert!(player.close_enchantment_view(current));
    assert!(player.enchantment_view().is_none());
}

/// The two slot rules vanilla puts on the enchanting table.
///
/// Both were missing, and each cost something different: with no `mayPlace`
/// on the currency slot, three of any item bought an enchantment and lapis
/// left the economy entirely; with no ceiling on the item slot, a
/// shift-clicked stack of sixty-four books went in and came back as one.
///
/// The ceiling is asserted on *both* slots on purpose. Capping the section
/// rather than the slot is an easy mistake and a quiet one -- the lapis slot
/// still looks right, and only the three-lapis offer silently becomes
/// unpayable. A test that checked the item slot alone would not catch it.
#[test]
fn the_table_takes_lapis_only_and_one_item_at_a_time() {
    init_vanilla_registry();
    let world = fresh_test_world("enchantment_slot_rules");
    let player: Arc<Player> =
        TestPlayerBuilder::new(Arc::clone(&world), "EnchantTester", 1).build();

    let menu = enchantment(
        Arc::clone(&player.inventory),
        1,
        BlockPos::new(0, 64, 0),
        &world,
    );
    let slots = menu.behavior().slots();

    // Vanilla: `mayPlace` returns `itemStack.is(Items.LAPIS_LAZULI)`.
    assert!(
        slots[SLOT_LAPIS].may_place(&ItemStack::new(&vanilla_items::LAPIS_LAZULI)),
        "the currency slot must accept lapis"
    );
    assert!(
        !slots[SLOT_LAPIS].may_place(&ItemStack::new(&vanilla_items::DIRT)),
        "the currency slot must not accept anything else"
    );

    // Vanilla: only the item slot overrides `getMaxStackSize()` to one.
    let guard = menu.behavior().lock_all_containers();
    assert_eq!(
        slots[SLOT_ITEM].get_max_stack_size(&guard),
        1,
        "a stack must never sit in the item slot"
    );
    assert!(
        slots[SLOT_LAPIS].get_max_stack_size(&guard) >= 3,
        "the lapis slot has to hold the three the top offer costs"
    );
}
