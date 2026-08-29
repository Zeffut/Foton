use std::io::Cursor;
use std::sync::Weak;

use foton_registry::init_vanilla_registry;
use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;

fn fox() -> FoxEntity {
    init_vanilla_registry();
    FoxEntity::new(&vanilla_entities::FOX, 1, DVec3::ZERO, Weak::new())
}

/// Every fox state lives in one synced byte, and `DEFENDING` is its sign bit.
/// Setting any one flag must leave the others alone.
#[test]
fn the_seven_state_flags_are_independent() {
    let fox = fox();

    fox.set_sitting(true);
    fox.set_defending(true);
    fox.set_sleeping(true);
    assert!(fox.is_sitting() && fox.is_defending() && fox.is_sleeping());
    assert!(!fox.is_crouching_flag() && !fox.is_interested() && !fox.is_pouncing());
    assert!(!fox.is_faceplanted());

    fox.set_sleeping(false);
    assert!(fox.is_sitting() && fox.is_defending() && !fox.is_sleeping());

    fox.clear_states();
    assert!(!fox.is_sitting() && !fox.is_defending() && !fox.is_faceplanted());
}

/// Vanilla parity: `Fox.canMove`, which is what the move control gates on --
/// a sleeping, sitting or faceplanted fox stays put.
#[test]
fn a_sleeping_sitting_or_faceplanted_fox_cannot_move() {
    let fox = fox();
    assert!(fox.can_move());

    for set in [
        FoxEntity::set_sleeping,
        FoxEntity::set_sitting,
        FoxEntity::set_faceplanted,
    ] {
        set(&fox, true);
        assert!(!fox.can_move());
        set(&fox, false);
        assert!(fox.can_move());
    }
}

/// Vanilla parity: `Fox.Variant.byId`, whose out-of-bounds strategy is `ZERO`
/// rather than the parrot's clamp -- a corrupt id makes a red fox, not a
/// white one.
#[test]
fn an_unknown_variant_id_falls_back_to_red() {
    assert_eq!(FoxVariant::by_id(0), FoxVariant::Red);
    assert_eq!(FoxVariant::by_id(1), FoxVariant::Snow);
    assert_eq!(FoxVariant::by_id(7), FoxVariant::Red);
    assert_eq!(FoxVariant::by_id(-1), FoxVariant::Red);
}

/// Vanilla parity: `Fox.setTargetGoals`, which is why a snow fox goes for fish
/// first and a red fox for chickens.
#[test]
fn the_coat_decides_which_prey_a_fox_hunts_first() {
    use super::goals::prey_goal_priorities;

    assert_eq!(prey_goal_priorities(FoxVariant::Red), (4, 6));
    assert_eq!(prey_goal_priorities(FoxVariant::Snow), (6, 4));
}

/// Vanilla parity: `Fox.addTrustedEntity`, which fills the first free slot and
/// then the second. Two players who bred a pair both end up trusted.
#[test]
fn a_fox_trusts_at_most_two_players_and_keeps_them_through_a_save() {
    let fox = fox();
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);
    let third = Uuid::from_u128(3);

    fox.add_trusted_entity(first);
    fox.add_trusted_entity(second);
    fox.add_trusted_entity(third);

    assert_eq!(fox.trusted_uuids(), [Some(first), Some(third)]);

    fox.set_variant(FoxVariant::Snow);
    fox.set_sleeping(true);
    let mut nbt = NbtCompound::new();
    fox.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("fox save data should reborrow: {error}"));

    let reloaded = FoxEntity::new(&vanilla_entities::FOX, 2, DVec3::ZERO, Weak::new());
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.trusted_uuids(), [Some(first), Some(third)]);
    assert_eq!(reloaded.variant(), FoxVariant::Snow);
    assert!(reloaded.is_sleeping());
}

/// Vanilla parity: `Fox.canHoldItem`. A fox with a trinket in its mouth swaps
/// it for food, but never the other way round.
#[test]
fn a_fox_swaps_a_trinket_for_food_but_not_food_for_a_trinket() {
    use foton_registry::item_stack::ItemStack;
    use foton_registry::vanilla_items;

    init_vanilla_registry();
    let fox = fox();
    let berries = ItemStack::new(&vanilla_items::SWEET_BERRIES);
    let emerald = ItemStack::new(&vanilla_items::EMERALD);

    assert!(Mob::can_hold_item(&fox, &emerald));

    fox.living_base
        .equipment()
        .lock()
        .set(EquipmentSlot::MainHand, emerald.clone());
    *fox.ticks_since_eaten.lock() = 1;
    assert!(Mob::can_hold_item(&fox, &berries));
    assert!(!Mob::can_hold_item(&fox, &emerald));

    fox.living_base
        .equipment()
        .lock()
        .set(EquipmentSlot::MainHand, berries.clone());
    assert!(!Mob::can_hold_item(&fox, &berries));
}
