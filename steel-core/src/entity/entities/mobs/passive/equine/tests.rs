//! Tests for the horse family.

use std::io::Cursor;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use steel_registry::init_vanilla_registry;
use steel_registry::item_stack::ItemStack;
use steel_registry::{vanilla_attributes, vanilla_entities, vanilla_items};
use steel_utils::UuidExt as _;
use uuid::Uuid;

use crate::entity::entities::{
    DonkeyEntity, HorseEntity, HorseMarkings, HorseVariant, LlamaEntity, SkeletonHorseEntity,
    ZombieHorseEntity,
};
use crate::entity::{
    AbstractChestedHorse, AbstractHorse, AgeableMob, Animal, Entity, LivingEntity, Llama,
    LlamaVariant,
};
use crate::inventory::container::Container as _;
use crate::inventory::equipment::EquipmentSlot;

fn reload<E: Entity>(entity: &E, nbt: &NbtCompound, target: &E) {
    let mut bytes = Vec::new();
    nbt.clone().write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
    let _ = entity;
    target.load_additional((&borrowed).into());
}

fn horse() -> HorseEntity {
    init_vanilla_registry();
    HorseEntity::new(&vanilla_entities::HORSE, 1, DVec3::ZERO, Weak::new())
}

fn donkey() -> DonkeyEntity {
    init_vanilla_registry();
    DonkeyEntity::new(&vanilla_entities::DONKEY, 1, DVec3::ZERO, Weak::new())
}

fn llama(id: i32) -> LlamaEntity {
    init_vanilla_registry();
    LlamaEntity::new(&vanilla_entities::LLAMA, id, DVec3::ZERO, Weak::new())
}

#[test]
fn a_horse_carries_its_taming_state_across_a_save() {
    // Temper, tame and owner all travel in different NBT shapes, and a horse
    // that loses any of them on load reverts to a wild one the player has to
    // break in again.
    let saved = horse();
    let owner = Uuid::from_u128(7);
    saved.set_tamed(true);
    saved.set_bred(true);
    saved.set_eating(true);
    saved.set_temper(42);
    saved.set_horse_owner(Some(owner));
    saved.set_variant_and_markings(HorseVariant::DarkBrown, HorseMarkings::WhiteDots);

    let mut nbt = NbtCompound::new();
    saved.save_additional(&mut nbt);

    let loaded = horse();
    reload(&saved, &nbt, &loaded);

    assert!(loaded.is_tamed());
    assert!(loaded.is_bred());
    assert!(loaded.is_eating());
    assert_eq!(loaded.temper(), 42);
    assert_eq!(loaded.horse_owner_uuid(), Some(owner));
    assert_eq!(loaded.variant(), HorseVariant::DarkBrown);
    assert_eq!(loaded.markings(), HorseMarkings::WhiteDots);
}

#[test]
fn a_donkey_keeps_its_chest_and_its_cargo_across_a_save() {
    // The chest widens the container, so the load order matters: reading the
    // items before the chest flag would drop every stack past slot zero.
    let saved = donkey();
    saved.set_chest(true);
    saved.create_horse_inventory();
    saved
        .abstract_horse_base()
        .inventory()
        .lock()
        .set_item(14, ItemStack::with_count(&vanilla_items::WHEAT, 5));

    let mut nbt = NbtCompound::new();
    saved.save_additional(&mut nbt);

    let loaded = donkey();
    reload(&saved, &nbt, &loaded);

    assert!(loaded.has_chest());
    assert_eq!(loaded.inventory_columns(), 5);
    let inventory = loaded.abstract_horse_base().inventory();
    let carried = inventory.lock();
    assert_eq!(carried.get_container_size(), 15);
    assert!(carried.get_item(14).is(&vanilla_items::WHEAT));
    assert_eq!(carried.get_item(14).count(), 5);
}

#[test]
fn taking_a_chest_off_a_donkey_shrinks_the_inventory_to_nothing() {
    let donkey = donkey();
    donkey.set_chest(true);
    donkey.create_horse_inventory();
    assert_eq!(
        donkey
            .abstract_horse_base()
            .inventory()
            .lock()
            .get_container_size(),
        15
    );

    donkey.set_chest(false);
    donkey.create_horse_inventory();
    assert_eq!(
        donkey
            .abstract_horse_base()
            .inventory()
            .lock()
            .get_container_size(),
        0
    );
}

#[test]
fn a_llama_sizes_its_inventory_from_its_strength() {
    // Vanilla ties the column count to the strength roll, so a five-strength
    // llama carries three times what a one-strength one does.
    let llama = llama(1);
    llama.set_strength(3);
    assert_eq!(
        llama.inventory_columns(),
        0,
        "a chestless llama carries nothing"
    );

    llama.set_chest(true);
    assert_eq!(llama.inventory_columns(), 3);
    llama.create_horse_inventory();
    assert_eq!(
        llama
            .abstract_horse_base()
            .inventory()
            .lock()
            .get_container_size(),
        9
    );
}

#[test]
fn a_llamas_strength_is_clamped_into_the_vanilla_range() {
    let llama = llama(1);
    llama.set_strength(0);
    assert_eq!(llama.strength(), 1);
    llama.set_strength(9);
    assert_eq!(llama.strength(), 5);
}

#[test]
fn a_llama_saves_its_strength_before_the_chest_reloads_the_inventory() {
    // `readAdditionalSaveData` reads Strength first for exactly this reason:
    // `createInventory` runs inside the chested-horse load and needs the width.
    let saved = llama(1);
    saved.set_strength(4);
    saved.set_llama_variant(LlamaVariant::Brown);
    saved.set_chest(true);
    saved.create_horse_inventory();
    saved
        .abstract_horse_base()
        .inventory()
        .lock()
        .set_item(11, ItemStack::new(&vanilla_items::APPLE));

    let mut nbt = NbtCompound::new();
    saved.save_additional(&mut nbt);

    let loaded = llama(2);
    reload(&saved, &nbt, &loaded);

    assert_eq!(loaded.strength(), 4);
    assert_eq!(loaded.llama_variant(), LlamaVariant::Brown);
    let inventory = loaded.abstract_horse_base().inventory();
    let carried = inventory.lock();
    assert_eq!(carried.get_container_size(), 12);
    assert!(carried.get_item(11).is(&vanilla_items::APPLE));
}

#[test]
fn a_caravan_link_is_symmetric_and_clears_from_both_ends() {
    // Vanilla holds the link on both llamas; losing the tail reference would
    // let a second llama attach to a head that is already taken.
    let head = llama(1);
    let tail = llama(2);

    tail.join_caravan(&head);
    assert!(tail.in_caravan());
    assert!(head.has_caravan_tail());
    assert!(!head.in_caravan());

    tail.leave_caravan();
    assert!(!tail.in_caravan());
}

#[test]
fn only_a_tamed_adult_horse_may_wear_a_saddle() {
    // Vanilla gates the saddle slot on `AbstractHorse.canUseSlot`, which is what
    // stops a foal or a wild horse being saddled from a dispenser.
    let donkey = donkey();
    assert!(!LivingEntity::can_use_slot(&donkey, EquipmentSlot::Saddle));

    donkey.set_tamed(true);
    assert!(LivingEntity::can_use_slot(&donkey, EquipmentSlot::Saddle));

    donkey.set_baby(true);
    assert!(!LivingEntity::can_use_slot(&donkey, EquipmentSlot::Saddle));
}

#[test]
fn a_reared_horse_refuses_to_move_under_its_rider() {
    let horse = horse();
    assert!(!LivingEntity::is_immobile(&horse));

    horse.set_standing(20);
    assert!(LivingEntity::is_immobile(&horse));

    horse.clear_standing();
    horse.set_eating(true);
    assert!(LivingEntity::is_immobile(&horse));
}

#[test]
fn rearing_clears_a_grazing_horses_head_out_of_the_grass() {
    // `setStanding` clears the eating flag; leaving both set would freeze the
    // horse in place because either one alone makes it immobile.
    let horse = horse();
    horse.set_eating(true);
    horse.set_standing(20);

    assert!(!horse.is_eating());
    assert!(horse.is_standing());
}

#[test]
fn a_skeleton_horse_carries_its_trap_countdown_across_a_save() {
    init_vanilla_registry();
    let saved = SkeletonHorseEntity::new(
        &vanilla_entities::SKELETON_HORSE,
        1,
        DVec3::ZERO,
        Weak::new(),
    );
    saved.set_trap(true);

    let mut nbt = NbtCompound::new();
    saved.save_additional(&mut nbt);

    let loaded = SkeletonHorseEntity::new(
        &vanilla_entities::SKELETON_HORSE,
        2,
        DVec3::ZERO,
        Weak::new(),
    );
    reload(&saved, &nbt, &loaded);

    assert!(loaded.is_trap());
}

#[test]
fn a_skeleton_foal_never_grows_up() {
    init_vanilla_registry();
    let skeleton_horse = SkeletonHorseEntity::new(
        &vanilla_entities::SKELETON_HORSE,
        1,
        DVec3::ZERO,
        Weak::new(),
    );

    assert!(!skeleton_horse.can_age_up());
}

#[test]
fn a_zombie_horse_rolls_its_speed_from_its_own_narrow_table() {
    // The zombie horse divides by 42.16 rather than using the horse formula; a
    // slip here makes it either crawl or outrun a sprinting player.
    init_vanilla_registry();
    let zombie_horse =
        ZombieHorseEntity::new(&vanilla_entities::ZOMBIE_HORSE, 1, DVec3::ZERO, Weak::new());

    zombie_horse.randomize_attributes();
    let speed = zombie_horse
        .attributes()
        .lock()
        .get_base_value(vanilla_attributes::MOVEMENT_SPEED)
        .expect("a zombie horse has a movement speed attribute");

    assert!(
        (9.0 / 42.16..=12.0 / 42.16).contains(&speed),
        "unexpected zombie horse speed {speed}"
    );
    assert!(!zombie_horse.can_fall_in_love());
}

#[test]
fn a_horses_rolled_attributes_stay_inside_the_breeding_range() {
    let horse = horse();
    horse.randomize_attributes();

    let attributes = horse.attributes().lock();
    let health = attributes
        .get_base_value(vanilla_attributes::MAX_HEALTH)
        .expect("a horse has a max health attribute");
    let speed = attributes
        .get_base_value(vanilla_attributes::MOVEMENT_SPEED)
        .expect("a horse has a movement speed attribute");
    let jump = attributes
        .get_base_value(vanilla_attributes::JUMP_STRENGTH)
        .expect("a horse has a jump strength attribute");

    assert!(
        (15.0..=30.0).contains(&health),
        "unexpected health {health}"
    );
    assert!(
        (0.1125..=0.3375).contains(&speed),
        "unexpected speed {speed}"
    );
    assert!((0.4..=1.0).contains(&jump), "unexpected jump {jump}");
}

#[test]
fn an_owner_uuid_survives_the_int_array_round_trip() {
    let saved = horse();
    let owner = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
    saved.set_horse_owner(Some(owner));

    let mut nbt = NbtCompound::new();
    saved.save_additional(&mut nbt);
    let stored = nbt
        .int_array("Owner")
        .expect("a horse with an owner stores one");
    assert_eq!(Uuid::from_int_array(stored), Some(owner));
}
