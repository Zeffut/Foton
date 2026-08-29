//! What answers when a command names a slot by number.
//!
//! Vanilla lays one flat numbering over every kind of slot an entity can have,
//! and the answer depends entirely on what the entity is: 500 is a horse's
//! first cargo slot and a player's first crafting slot, 499 is a mount's chest
//! and a player's cursor. `execute if items` is built on that, so a wrong
//! answer here is a condition that reads the wrong container and says nothing
//! about it.
//!
//! The distinction these tests keep insisting on is `None` against
//! `Some(empty)`. Vanilla returns a null `SlotAccess` for a slot the entity
//! does not have and an empty stack for one it has and has not filled;
//! collapsing the two would make `execute if items` count an armor slot on a
//! chest minecart.

use std::sync::Weak;

use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
use glam::DVec3;

use foton_utils::Downcast as _;

use crate::entity::entities::ItemEntity;
use crate::entity::{ENTITIES, SharedEntity, init_entities, next_entity_id};
use crate::inventory::container::Container as _;
use crate::inventory::equipment::EquipmentSlot;
use crate::inventory::slot_ranges::{SLOT_RANGES, SlotRange};

/// Builds one live entity of `entity_type`.
fn fresh(entity_type: EntityTypeRef) -> SharedEntity {
    init_vanilla_registry();
    init_entities();
    ENTITIES
        .create(entity_type, next_entity_id(), DVec3::ZERO, Weak::new())
        .unwrap_or_else(|| panic!("{} has no entity factory", entity_type.key))
}

/// The single slot id a named range covers.
fn only_slot(name: &str) -> i32 {
    let Some(range) = SLOT_RANGES.name_to_ids(name) else {
        panic!("{name} should be a slot range");
    };
    assert_eq!(
        range.slots().len(),
        1,
        "{name} is not a single-slot range any more"
    );
    range.slots()[0]
}

/// The slot ids a named range covers.
fn range(name: &str) -> &'static SlotRange {
    let Some(range) = SLOT_RANGES.name_to_ids(name) else {
        panic!("{name} should be a slot range");
    };
    range
}

/// Every living entity answers for the eight equipment slots and for nothing
/// else in that block. 104 is the hole vanilla leaves between the armor slots
/// and the body slot, and a pig has no cargo, no crafting grid and no chest.
#[test]
fn a_living_entity_answers_for_its_equipment_and_for_no_other_number() {
    let entity = fresh(&vanilla_entities::PIG);
    let Some(living) = entity.as_living_entity() else {
        panic!("a pig is a living entity");
    };
    let helmet = ItemStack::new(&vanilla_items::DIAMOND_HELMET);
    living.set_item_slot(EquipmentSlot::Head, helmet.clone());

    assert_eq!(
        entity.slot_item(only_slot("armor.head")),
        Some(helmet),
        "the head slot should hand back what was put in it"
    );
    // A slot that exists and is empty, which the condition tests rather than
    // skips.
    assert_eq!(
        entity.slot_item(only_slot("weapon.mainhand")),
        Some(ItemStack::empty())
    );
    assert_eq!(
        entity.slot_item(only_slot("saddle")),
        Some(ItemStack::empty())
    );

    // Slots a pig does not have at all.
    assert_eq!(entity.slot_item(104), None, "104 is not an equipment slot");
    assert_eq!(entity.slot_item(only_slot("contents")), None);
    assert_eq!(entity.slot_item(only_slot("horse.0")), None);
    assert_eq!(entity.slot_item(only_slot("mob.inventory.0")), None);
    assert_eq!(entity.slot_item(only_slot("player.cursor")), None);
}

/// A mount's cargo is the `horse.*` range, and the chest itself is 499. A
/// donkey with no chest has no cargo slots at all, which is vanilla's own
/// `getInventoryColumns() == 0`.
#[test]
fn a_mount_answers_for_its_cargo_and_for_the_chest_it_wears() {
    let entity = fresh(&vanilla_entities::DONKEY);
    let Some(horse) = entity.as_abstract_horse() else {
        panic!("a donkey is a horse");
    };
    let Some(chested) = entity.as_abstract_chested_horse() else {
        panic!("a donkey can wear a chest");
    };

    assert!(!chested.has_chest());
    assert_eq!(
        entity.slot_item(only_slot("horse.chest")),
        Some(ItemStack::empty()),
        "the chest slot exists whether or not a chest is in it"
    );
    assert_eq!(
        entity.slot_item(only_slot("horse.0")),
        None,
        "a chestless donkey carries nothing"
    );

    chested.set_chest(true);
    horse.create_horse_inventory();
    let hay = ItemStack::new(&vanilla_items::HAY_BLOCK);
    horse
        .abstract_horse_base()
        .inventory()
        .lock()
        .set_item(0, hay.clone());

    assert_eq!(
        entity.slot_item(only_slot("horse.chest")),
        Some(ItemStack::new(&vanilla_items::CHEST))
    );
    assert_eq!(entity.slot_item(only_slot("horse.0")), Some(hay));
    assert_eq!(
        entity.slot_item(only_slot("horse.1")),
        Some(ItemStack::empty())
    );
    // A chest gives fifteen slots, and `horse.*` names exactly those.
    assert_eq!(range("horse.*").slots().len(), 15);
    assert_eq!(entity.slot_item(515), None, "515 is past the cargo");
}

/// The `mob.inventory.*` range is the container a villager, a pillager or a
/// piglin carries. The pillager and the piglin had one and did not expose it;
/// that is what this covers beyond the villager.
#[test]
fn a_mob_that_carries_a_container_answers_for_mob_inventory() {
    for entity_type in [
        &vanilla_entities::VILLAGER,
        &vanilla_entities::PILLAGER,
        &vanilla_entities::PIGLIN,
    ] {
        let key = &entity_type.key;
        let entity = fresh(entity_type);
        let Some(carrier) = entity.as_inventory_carrier() else {
            panic!("{key} should carry a container");
        };
        let bread = ItemStack::new(&vanilla_items::BREAD);
        carrier
            .carried_inventory()
            .lock()
            .set_item(0, bread.clone());

        assert_eq!(
            entity.slot_item(only_slot("mob.inventory.0")),
            Some(bread),
            "{key} lost what was put in its first inventory slot"
        );
        assert_eq!(
            entity.slot_item(only_slot("weapon.mainhand")),
            Some(ItemStack::empty()),
            "{key} should still answer for its equipment"
        );
    }
}

/// An entity that holds exactly one item answers for `contents` and for
/// nothing else.
#[test]
fn a_single_item_entity_answers_only_for_contents() {
    let entity = fresh(&vanilla_entities::ITEM);
    let Some(item_entity) = entity.downcast_ref::<ItemEntity>() else {
        panic!("an item entity should downcast to itself");
    };
    let stone = ItemStack::new(&vanilla_items::STONE);
    item_entity.set_item(stone.clone());

    assert_eq!(entity.slot_item(only_slot("contents")), Some(stone));
    assert_eq!(entity.slot_item(1), None);
    assert_eq!(entity.slot_item(only_slot("armor.head")), None);
}

/// A chest vehicle is a plain container over the `container.*` range, and
/// stops at its own size rather than at the range's.
///
/// The filled case is `dev/items-test.sh`, which needs a world to put items
/// into a cart; what is worth pinning down here is the boundary, because
/// `container.*` names fifty-four slots and a chest minecart has twenty-seven.
#[test]
fn a_chest_vehicle_answers_for_the_container_range_up_to_its_own_size() {
    let entity = fresh(&vanilla_entities::CHEST_MINECART);

    assert_eq!(
        entity.slot_item(only_slot("container.0")),
        Some(ItemStack::empty())
    );
    assert_eq!(
        entity.slot_item(only_slot("container.26")),
        Some(ItemStack::empty())
    );
    assert_eq!(
        entity.slot_item(only_slot("container.27")),
        None,
        "a chest minecart has twenty-seven slots and no more"
    );
    assert_eq!(
        entity.slot_item(only_slot("armor.head")),
        None,
        "a minecart is not a living entity"
    );
}
