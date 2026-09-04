use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, ChunkPos, Downcast as _};
use glam::DVec3;

use super::{SmithingKind, smithing};
use crate::behavior::init_behaviors;
use crate::entity::Entity as _;
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::lock::ContainerId;
use crate::inventory::menu::Menu;
use crate::inventory::slots::{
    ResultHandler as _, SMITHING_ADDITION, SMITHING_BASE, SMITHING_TEMPLATE,
};
use crate::player::Player;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

const TABLE_POS: BlockPos = BlockPos::new(0, 64, 0);

fn test_table(key: &'static str) -> (Arc<World>, Arc<Player>, Menu) {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(TABLE_POS));
    assert!(world.set_block(
        TABLE_POS,
        vanilla_blocks::SMITHING_TABLE.default_state(),
        UpdateFlags::UPDATE_ALL,
    ));

    let player = TestPlayerBuilder::new(Arc::clone(&world), "SmithingTester", 1).build();
    player.base().set_position_local(DVec3::new(0.5, 64.0, 0.5));

    let menu = smithing(Arc::clone(&player.inventory), 1, TABLE_POS);
    (world, player, menu)
}

fn input_container_id(menu: &Menu) -> ContainerId {
    let kind = menu
        .kind()
        .downcast_ref::<SmithingKind>()
        .expect("the smithing menu carries a SmithingKind");
    kind.handler
        .dependencies()
        .first()
        .expect("the smithing handler depends on its input container")
        .container_id()
}

/// Each input slot only takes what a smithing recipe wants there.
///
/// Vanilla parity: the three `mayPlace` tests of
/// `SmithingMenu.createInputSlotDefinitions`. Foton built the section with no
/// predicate at all, so anything went anywhere -- report #16 -- and shift-click
/// then filled the first free slot, which is why nothing ever upgraded (#17).
#[test]
fn each_slot_only_takes_what_belongs_in_it() {
    let (_world, _player, menu) = test_table("smithing_slot_predicates");
    let slots = menu.behavior().slots();

    let template = ItemStack::new(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE);
    let base = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
    let addition = ItemStack::new(&vanilla_items::NETHERITE_INGOT);
    let nonsense = ItemStack::new(&vanilla_items::DIRT);

    assert!(slots[SMITHING_TEMPLATE].may_place(&template));
    assert!(slots[SMITHING_BASE].may_place(&base));
    assert!(slots[SMITHING_ADDITION].may_place(&addition));

    assert!(
        !slots[SMITHING_TEMPLATE].may_place(&base),
        "a pickaxe in the template slot is what stopped every upgrade"
    );
    assert!(!slots[SMITHING_BASE].may_place(&template));
    assert!(!slots[SMITHING_ADDITION].may_place(&base));

    for slot in [SMITHING_TEMPLATE, SMITHING_BASE, SMITHING_ADDITION] {
        assert!(
            !slots[slot].may_place(&nonsense),
            "no smithing recipe wants dirt anywhere"
        );
    }
}

/// The three right items still make the upgrade.
#[test]
fn a_correct_layout_still_upgrades() {
    let (_world, player, mut menu) = test_table("smithing_upgrade");
    let input_id = input_container_id(&menu);

    {
        let mut guard = menu.behavior().lock_all_containers();
        {
            let container = guard
                .get_typed_mut::<SimpleContainer>(input_id)
                .expect("the input container should be locked");
            container.set_item(
                SMITHING_TEMPLATE,
                ItemStack::new(&vanilla_items::NETHERITE_UPGRADE_SMITHING_TEMPLATE),
            );
            container.set_item(
                SMITHING_BASE,
                ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
            );
            container.set_item(
                SMITHING_ADDITION,
                ItemStack::new(&vanilla_items::NETHERITE_INGOT),
            );
        }
        menu.slots_changed(&mut guard, &player);
    }

    let guard = menu.behavior().lock_all_containers();
    let result = menu.behavior().slots()[3].get_item(&guard);
    assert!(
        result.is(&vanilla_items::NETHERITE_PICKAXE),
        "the table should still upgrade a diamond pickaxe"
    );
}
