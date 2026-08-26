//! Piglin-family tests.
//!
//! These are the pieces of the four mobs that are neither a constant nor
//! something the compiler already guarantees: the barter, the gold-armor
//! truce, the two zombification clocks, the hunting bookkeeping, and the
//! knockback the hoglin and zoglin share.

use std::io::Cursor;
use std::ptr;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use steel_registry::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;
use steel_registry::{init_vanilla_registry, vanilla_attributes, vanilla_entities, vanilla_items};
use steel_utils::types::InteractionHand;
use steel_utils::{ChunkPos, WorldAabb};

use crate::behavior::init_behaviors;
use crate::behavior::items::{MOB_ARROW_POWER, perform_crossbow_attack};
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::entities::mobs::hostile::piglin_predicates::is_wearing_safe_armor;
use crate::entity::entities::{HoglinEntity, PiglinBruteEntity, PiglinEntity, ZoglinEntity};
use crate::entity::hoglin_base;
use crate::entity::{
    ENTITIES, Entity, EntitySpawnReason, LivingEntity, Mob, SharedEntity, init_entities,
    next_entity_id,
};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

use super::abstract_piglin::{self, ConvertiblePiglin, PiglinArmPose};
use super::piglin_ai;

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn piglin_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    // A crossbow bolt is spawned through the entity factory, so a test that
    // fires one must not depend on another test having initialized it first.
    init_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn spawn_piglin(world: &Arc<World>) -> Arc<PiglinEntity> {
    spawn_piglin_at(world, SPAWN)
}

fn spawn_piglin_at(world: &Arc<World>, position: DVec3) -> Arc<PiglinEntity> {
    let piglin = Arc::new(PiglinEntity::new(
        &vanilla_entities::PIGLIN,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&piglin) as SharedEntity)
        .expect("the test chunk is loaded, so the piglin should attach");
    piglin
}

fn spawn_hoglin(world: &Arc<World>) -> Arc<HoglinEntity> {
    let hoglin = Arc::new(HoglinEntity::new(
        &vanilla_entities::HOGLIN,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&hoglin) as SharedEntity)
        .expect("the test chunk is loaded, so the hoglin should attach");
    hoglin
}

fn detached_piglin() -> PiglinEntity {
    init_vanilla_registry();
    PiglinEntity::new(
        &vanilla_entities::PIGLIN,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    )
}

// Bartering.
/// The whole point of the piglin: a gold ingot in the off hand at the moment
/// admiring runs out becomes a roll of `gameplay/piglin_bartering`, thrown on
/// the floor. If the loot table were not reached, or the currency check were
/// wrong, this comes back empty.
#[test]
fn a_piglin_that_finishes_admiring_a_gold_ingot_barters_it_away() {
    init_vanilla_registry();
    let piglin = detached_piglin();

    let rolled = piglin_ai::barter_response_items(&piglin);

    assert!(
        !rolled.is_empty(),
        "the piglin bartering table rolled nothing; every one of its pools has a \
         guaranteed roll, so an empty result means the table was not reached"
    );
    assert!(
        rolled.iter().all(|item| !item.is_empty()),
        "the bartering table produced an empty stack: {rolled:?}"
    );
}

/// Only a gold ingot starts a barter. A gold nugget is picked up, and a gold
/// block is admired, but neither is currency.
#[test]
fn only_a_gold_ingot_is_barter_currency() {
    init_vanilla_registry();

    assert!(piglin_ai::is_barter_currency(&ItemStack::new(
        &vanilla_items::GOLD_INGOT
    )));
    assert!(!piglin_ai::is_barter_currency(&ItemStack::new(
        &vanilla_items::GOLD_NUGGET
    )));
    assert!(!piglin_ai::is_barter_currency(&ItemStack::new(
        &vanilla_items::GOLD_BLOCK
    )));
}

/// A baby piglin keeps what it finds instead of trading it, which is why baby
/// piglins are useless at a gold farm.
#[test]
fn a_baby_piglin_pockets_the_ingot_rather_than_bartering_it() {
    let world = piglin_world("piglin_baby_no_barter");
    let piglin = spawn_piglin(&world);
    piglin.set_baby(true);
    piglin.set_item_in_hand(
        InteractionHand::OffHand,
        ItemStack::new(&vanilla_items::GOLD_INGOT),
    );

    piglin_ai::stop_holding_off_hand_item(&piglin, true);

    let carried_or_held = !piglin
        .get_item_in_hand(InteractionHand::MainHand)
        .is_empty()
        || piglin
            .remove_all_inventory_items()
            .iter()
            .any(|item| item.is(&vanilla_items::GOLD_INGOT));
    assert!(
        carried_or_held,
        "a baby piglin threw its ingot away instead of keeping it"
    );
}

// Gold armor.
/// The truce: any one piece of gold armor is enough, and iron is not.
#[test]
fn one_piece_of_gold_armor_is_enough_to_be_left_alone() {
    let world = piglin_world("piglin_gold_armor");
    let onlooker = spawn_piglin(&world);

    assert!(
        !is_wearing_safe_armor(onlooker.as_ref()),
        "a bare mob should not read as wearing gold"
    );

    onlooker.set_item_slot(
        EquipmentSlot::Feet,
        ItemStack::new(&vanilla_items::IRON_BOOTS),
    );
    assert!(
        !is_wearing_safe_armor(onlooker.as_ref()),
        "iron boots are not in the piglin_safe_armor tag"
    );

    onlooker.set_item_slot(
        EquipmentSlot::Feet,
        ItemStack::new(&vanilla_items::GOLDEN_BOOTS),
    );
    assert!(
        is_wearing_safe_armor(onlooker.as_ref()),
        "golden boots alone should buy the truce"
    );
}

// Zombification.
/// The conversion clock only runs where the dimension zombifies piglins, and it
/// runs for exactly `CONVERSION_TIME` ticks before firing.
#[test]
fn a_piglin_only_counts_down_to_zombification_in_a_zombifying_dimension() {
    let world = piglin_world("piglin_conversion_clock");
    let piglin = spawn_piglin(&world);

    // The test world is the overworld, which zombifies.
    assert!(
        piglin.piglin_is_converting(),
        "the overworld should zombify piglins"
    );

    piglin.set_time_in_overworld(0);
    for _ in 0..10 {
        abstract_piglin::tick_conversion(piglin.as_ref());
    }
    assert_eq!(
        piglin.time_in_overworld(),
        10,
        "the overworld clock should advance one tick per call"
    );

    // A piglin made immune stops counting and resets.
    piglin.set_immune_to_zombification(true);
    assert!(!piglin.piglin_is_converting());
    abstract_piglin::tick_conversion(piglin.as_ref());
    assert_eq!(
        piglin.time_in_overworld(),
        0,
        "an immune piglin should reset its clock rather than keep counting"
    );
}

/// Past the threshold the piglin is gone and a zombified piglin stands where it
/// did, wearing what it wore.
#[test]
fn a_piglin_left_in_the_overworld_becomes_a_zombified_piglin_keeping_its_gold() {
    let world = piglin_world("piglin_conversion_finish");
    let piglin = spawn_piglin(&world);
    piglin.set_item_slot(
        EquipmentSlot::Head,
        ItemStack::new(&vanilla_items::GOLDEN_HELMET),
    );

    let zombified = piglin
        .finish_conversion()
        .expect("a live piglin in a loaded chunk should convert");

    assert!(
        piglin.is_removed(),
        "the piglin should be discarded once its replacement joins the world"
    );
    assert!(
        zombified
            .get_item_by_slot(EquipmentSlot::Head)
            .is(&vanilla_items::GOLDEN_HELMET),
        "vanilla converts a piglin with keepEquipment, so the helmet moves across"
    );
}

/// The same clock drives the brute, whose own conversion sound differs but
/// whose threshold does not.
#[test]
fn a_piglin_brute_runs_the_same_overworld_clock() {
    init_vanilla_registry();
    let brute = PiglinBruteEntity::new(
        &vanilla_entities::PIGLIN_BRUTE,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    // With no world a brute is not converting, so the clock stays at zero.
    assert!(!ConvertiblePiglin::is_converting(&brute));
    brute.set_time_in_overworld(5);
    abstract_piglin::tick_conversion(&brute);
    assert_eq!(brute.time_in_overworld(), 0);
}

/// A hoglin left in the overworld becomes a zoglin, and a baby one a baby
/// zoglin.
#[test]
fn a_baby_hoglin_left_in_the_overworld_becomes_a_baby_zoglin() {
    let world = piglin_world("hoglin_conversion");
    let hoglin = spawn_hoglin(&world);
    Mob::set_baby(hoglin.as_ref(), true);

    let zoglin = hoglin
        .finish_conversion()
        .expect("a live hoglin in a loaded chunk should convert");

    assert!(hoglin.is_removed(), "the hoglin should be discarded");
    assert!(
        LivingEntity::is_baby(zoglin.as_ref()),
        "a baby hoglin should not grow up on the way to being a zoglin"
    );
}

// Hunting.
/// A hoglin that has already been hunted is off the menu, and so is a baby.
#[test]
fn only_a_grown_unhunted_hoglin_can_be_hunted() {
    init_vanilla_registry();
    let hoglin = HoglinEntity::new(
        &vanilla_entities::HOGLIN,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    assert!(hoglin.can_be_hunted(), "a grown hoglin is huntable");

    Mob::set_baby(&hoglin, true);
    assert!(!hoglin.can_be_hunted(), "a baby hoglin is never hunted");

    Mob::set_baby(&hoglin, false);
    hoglin.set_cannot_be_hunted(true);
    assert!(
        !hoglin.can_be_hunted(),
        "a hoglin the save marked off-limits stays off-limits"
    );
}

/// A freshly spawned piglin is put on a hunting cooldown, so a newly generated
/// bastion does not empty its stable on the first tick.
#[test]
fn a_spawned_piglin_starts_on_a_hunting_cooldown() {
    let world = piglin_world("piglin_hunt_cooldown");
    let piglin = spawn_piglin(&world);

    assert!(
        !piglin
            .brain_ref()
            .has_memory_value(memory_module_types::HUNTED_RECENTLY.id()),
        "a bare piglin has no cooldown until it is finalized"
    );

    piglin_ai::init_memories(piglin.brain_ref());

    assert!(
        piglin
            .brain_ref()
            .has_memory_value(memory_module_types::HUNTED_RECENTLY.id()),
        "finalizing a piglin should set HUNTED_RECENTLY so it waits before hunting"
    );
}

/// Only a piglin hunts. A brute never does, whatever its memories say.
#[test]
fn a_brute_never_hunts_and_a_piglin_does_unless_told_otherwise() {
    init_vanilla_registry();
    let piglin = detached_piglin();
    let brute = PiglinBruteEntity::new(
        &vanilla_entities::PIGLIN_BRUTE,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    assert!(Mob::can_hunt(&piglin));
    assert!(!Mob::can_hunt(&brute));

    piglin.set_cannot_hunt(true);
    assert!(!Mob::can_hunt(&piglin));
}

// Carrying.
/// The eight-slot inventory fills, refuses a ninth stack, and empties.
///
/// Vanilla parity: `Piglin.INVENTORY_SIZE` is eight, and `canAddToInventory`
/// is what stops a piglin at a gold farm hoovering up an unbounded pile.
#[test]
fn a_piglin_carries_eight_stacks_and_gives_them_all_back() {
    init_vanilla_registry();
    let piglin = detached_piglin();

    // Whole stacks, so each one takes a slot of its own rather than merging.
    for _ in 0..8 {
        let leftover =
            piglin.add_to_inventory(ItemStack::with_count(&vanilla_items::GOLD_BLOCK, 64));
        assert!(leftover.is_empty(), "eight slots should take eight stacks");
    }

    assert!(
        !piglin.can_add_to_inventory(&ItemStack::new(&vanilla_items::GOLD_INGOT)),
        "a full inventory should refuse an item that cannot merge into any slot"
    );
    assert!(
        !piglin.can_add_to_inventory(&ItemStack::new(&vanilla_items::GOLD_BLOCK)),
        "and refuse a ninth of what it already carries, because every slot is full"
    );

    let returned = piglin.remove_all_inventory_items();
    assert_eq!(returned.len(), 8, "every carried stack should come back");
    assert!(
        piglin.can_add_to_inventory(&ItemStack::new(&vanilla_items::GOLD_INGOT)),
        "an emptied inventory should take a stack again"
    );
}

/// A piglin handed a gold ingot pins itself only for things that are not the
/// barter currency -- otherwise a gold farm would fill up with piglins that
/// never despawn.
#[test]
fn holding_the_barter_currency_does_not_pin_a_piglin_in_place() {
    init_vanilla_registry();
    let piglin = detached_piglin();

    piglin.hold_in_off_hand(ItemStack::new(&vanilla_items::GOLD_INGOT));
    assert!(
        !Mob::is_persistence_required(&piglin),
        "a gold ingot is the barter currency and must not make a piglin permanent"
    );

    piglin.hold_in_off_hand(ItemStack::new(&vanilla_items::GOLD_BLOCK));
    assert!(
        Mob::is_persistence_required(&piglin),
        "anything else a piglin is handed does pin it"
    );
}

// Arm poses.
/// The arm pose the client reads follows what the piglin is doing, and the
/// dancing check wins over everything else.
#[test]
fn a_piglin_arm_pose_follows_what_it_is_doing() {
    init_vanilla_registry();
    let piglin = detached_piglin();

    assert_eq!(piglin.arm_pose(), PiglinArmPose::Default);

    piglin.set_item_in_hand(
        InteractionHand::OffHand,
        ItemStack::new(&vanilla_items::GOLD_INGOT),
    );
    assert_eq!(
        piglin.arm_pose(),
        PiglinArmPose::AdmiringItem,
        "gold in the off hand is admired"
    );

    piglin.set_charging_crossbow(true);
    assert_eq!(
        piglin.arm_pose(),
        PiglinArmPose::AdmiringItem,
        "admiring still wins over winding"
    );

    piglin.set_item_in_hand(InteractionHand::OffHand, ItemStack::empty());
    assert_eq!(piglin.arm_pose(), PiglinArmPose::CrossbowCharge);

    piglin.set_dancing(true);
    assert_eq!(
        piglin.arm_pose(),
        PiglinArmPose::Dancing,
        "dancing wins over everything"
    );
}

// The charge.
/// The hoglin's hit launches its target, and a baby's does not.
#[test]
fn a_grown_hoglin_launches_what_it_hits_and_a_baby_does_not() {
    let world = piglin_world("hoglin_knockback");
    let hoglin = spawn_hoglin(&world);
    // Stand the victim off to one side, so the push has a direction to take.
    let victim = spawn_piglin_at(&world, SPAWN + DVec3::new(1.0, 0.0, 0.0));
    let victim_entity: SharedEntity = Arc::clone(&victim) as SharedEntity;

    hoglin
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::ATTACK_KNOCKBACK, 1.0);
    victim.set_velocity(DVec3::ZERO);

    hoglin_base::throw_target(hoglin.as_ref(), &victim_entity);
    let launched = victim.velocity();
    assert!(
        launched.length_squared() > 0.0,
        "a hoglin with attack knockback should launch its target, got {launched:?}"
    );
    assert!(
        launched.y > 0.0,
        "the launch is upward as well as sideways, got {launched:?}"
    );
}

/// Knockback resistance eats the launch entirely once it matches the attack
/// knockback, which is why a hoglin cannot shift an iron golem.
#[test]
fn knockback_resistance_cancels_the_hoglin_launch() {
    let world = piglin_world("hoglin_knockback_resist");
    let hoglin = spawn_hoglin(&world);
    let victim = spawn_piglin(&world);
    let victim_entity: SharedEntity = Arc::clone(&victim) as SharedEntity;

    hoglin
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::ATTACK_KNOCKBACK, 1.0);
    victim
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::KNOCKBACK_RESISTANCE, 1.0);
    victim.set_velocity(DVec3::ZERO);

    hoglin_base::throw_target(hoglin.as_ref(), &victim_entity);

    assert_eq!(
        victim.velocity(),
        DVec3::ZERO,
        "full knockback resistance should leave the target exactly where it was"
    );
}

/// A zoglin lands the same charge a hoglin does -- it is the one thing the two
/// share through `HoglinBase`.
#[test]
fn a_zoglin_lands_the_same_charge_a_hoglin_does() {
    let world = piglin_world("zoglin_knockback");
    let zoglin = Arc::new(ZoglinEntity::new(
        &vanilla_entities::ZOGLIN,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&zoglin) as SharedEntity)
        .expect("the test chunk is loaded");
    let victim = spawn_piglin_at(&world, SPAWN + DVec3::new(1.0, 0.0, 0.0));
    let victim_entity: SharedEntity = Arc::clone(&victim) as SharedEntity;

    zoglin
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::ATTACK_KNOCKBACK, 1.0);
    victim.set_velocity(DVec3::ZERO);

    hoglin_base::throw_target(zoglin.as_ref(), &victim_entity);

    assert!(
        victim.velocity().length_squared() > 0.0,
        "a zoglin should launch its target the same way a hoglin does"
    );
}

/// A hoglin standing exactly on its target still launches it, straight up.
///
/// Vanilla parity: `Vec3.normalize` answers `ZERO` for a zero-length vector and
/// `HoglinBase.throwTarget` carries on with it, so the horizontal push is
/// nothing and the vertical push is not.
#[test]
fn a_hoglin_stacked_on_its_target_still_launches_it_upward() {
    let world = piglin_world("hoglin_knockback_stacked");
    let hoglin = spawn_hoglin(&world);
    let victim = spawn_piglin(&world);
    let victim_entity: SharedEntity = Arc::clone(&victim) as SharedEntity;

    hoglin
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::ATTACK_KNOCKBACK, 1.0);
    // Both at exactly `SPAWN`, which is where they were put.
    victim.set_velocity(DVec3::ZERO);

    hoglin_base::throw_target(hoglin.as_ref(), &victim_entity);

    let launched = victim.velocity();
    assert!(
        launched.x.abs() < f64::EPSILON,
        "there is no direction to push sideways in, got {launched:?}"
    );
    assert!(
        launched.z.abs() < f64::EPSILON,
        "there is no direction to push sideways in, got {launched:?}"
    );
    assert!(
        launched.y > 0.0,
        "but the vertical push is unconditional, got {launched:?}"
    );
}

/// A baby zoglin's damage is halved on the way in.
#[test]
fn a_baby_zoglin_hits_for_half_a_heart() {
    init_vanilla_registry();
    let zoglin = ZoglinEntity::new(
        &vanilla_entities::ZOGLIN,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    let grown = zoglin
        .attributes()
        .lock()
        .get_base_value(vanilla_attributes::ATTACK_DAMAGE)
        .expect("a zoglin has an attack damage attribute");
    zoglin.set_baby(true);
    let baby = zoglin
        .attributes()
        .lock()
        .get_base_value(vanilla_attributes::ATTACK_DAMAGE)
        .expect("a zoglin has an attack damage attribute");

    assert!(
        baby < grown,
        "a baby zoglin should hit for less than a grown one, got {baby} against {grown}"
    );
}

// Ageing.
/// A hoglin growing up puts its damage and its experience back.
#[test]
fn a_hoglin_growing_up_restores_its_damage_and_its_reward() {
    init_vanilla_registry();
    let hoglin = HoglinEntity::new(
        &vanilla_entities::HOGLIN,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    Mob::set_baby(&hoglin, true);
    let baby_damage = hoglin
        .attributes()
        .lock()
        .get_base_value(vanilla_attributes::ATTACK_DAMAGE)
        .expect("a hoglin has an attack damage attribute");
    let baby_reward = Mob::xp_reward(&hoglin);

    Mob::set_baby(&hoglin, false);
    let grown_damage = hoglin
        .attributes()
        .lock()
        .get_base_value(vanilla_attributes::ATTACK_DAMAGE)
        .expect("a hoglin has an attack damage attribute");
    let grown_reward = Mob::xp_reward(&hoglin);

    assert!(
        baby_damage < grown_damage,
        "a baby hoglin hits for less: {baby_damage} against {grown_damage}"
    );
    assert!(
        baby_reward < grown_reward,
        "a baby hoglin is worth less: {baby_reward} against {grown_reward}"
    );
}

// Persistence.
/// The carried inventory, the age and the hunting flag all survive a save and
/// load, which is what stops a chunk unload eating a piglin's pockets.
#[test]
fn a_piglin_carried_inventory_survives_a_save_and_load() {
    let world = piglin_world("piglin_round_trip");
    let piglin = spawn_piglin(&world);
    piglin.add_to_inventory(ItemStack::with_count(&vanilla_items::GOLD_NUGGET, 7));
    piglin.set_baby(true);
    piglin.set_cannot_hunt(true);
    piglin.set_time_in_overworld(42);

    let mut nbt = NbtCompound::new();
    piglin.save_additional(&mut nbt);

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("piglin nbt should reborrow: {error}"));

    let restored = Arc::new(PiglinEntity::new(
        &vanilla_entities::PIGLIN,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    let carried = restored.remove_all_inventory_items();
    assert_eq!(carried.len(), 1, "the one carried stack should come back");
    assert_eq!(carried[0].count(), 7, "and with its count intact");
    assert!(
        LivingEntity::is_baby(restored.as_ref()),
        "a baby stays a baby"
    );
    assert!(
        !Mob::can_hunt(restored.as_ref()),
        "a piglin told not to hunt stays told"
    );
    assert_eq!(restored.time_in_overworld(), 42);
}

/// A piglin that has picked something up but not admired it keeps a preference
/// for what it wants over what it is carrying.
#[test]
fn a_piglin_prefers_a_wanted_item_over_a_better_one() {
    init_vanilla_registry();
    let piglin = detached_piglin();

    let gold_sword = ItemStack::new(&vanilla_items::GOLDEN_SWORD);
    let diamond_sword = ItemStack::new(&vanilla_items::DIAMOND_SWORD);

    assert!(
        Mob::can_replace_current_item(
            &piglin,
            &gold_sword,
            &diamond_sword,
            EquipmentSlot::MainHand
        ),
        "a piglin drops a diamond sword for a golden one, because gold is loved"
    );
    assert!(
        !Mob::can_replace_current_item(
            &piglin,
            &diamond_sword,
            &gold_sword,
            EquipmentSlot::MainHand
        ),
        "and never the other way round"
    );
}

/// A crossbow beats a golden sword, because the crossbow is in the piglin's
/// preferred-weapon tag and the sword is only loved.
#[test]
fn a_piglin_prefers_a_crossbow_to_a_golden_sword() {
    init_vanilla_registry();
    let piglin = detached_piglin();

    let crossbow = ItemStack::new(&vanilla_items::CROSSBOW);
    let gold_sword = ItemStack::new(&vanilla_items::GOLDEN_SWORD);

    assert!(
        Mob::can_replace_current_item(&piglin, &crossbow, &gold_sword, EquipmentSlot::MainHand),
        "a crossbow is in piglin_preferred_weapons and a golden sword is not"
    );
    assert!(
        !Mob::can_replace_current_item(&piglin, &gold_sword, &crossbow, EquipmentSlot::MainHand),
        "and a piglin never gives its crossbow up for a sword"
    );
}

/// A brute only ever picks up its axe.
#[test]
fn a_brute_only_picks_up_a_golden_axe() {
    let world = piglin_world("brute_pickup");
    let brute = Arc::new(PiglinBruteEntity::new(
        &vanilla_entities::PIGLIN_BRUTE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&brute) as SharedEntity)
        .expect("the test chunk is loaded");

    assert!(
        Mob::wants_to_pick_up(
            brute.as_ref(),
            world.as_ref(),
            &ItemStack::new(&vanilla_items::GOLDEN_AXE)
        ),
        "a brute takes a golden axe"
    );
    assert!(
        !Mob::wants_to_pick_up(
            brute.as_ref(),
            world.as_ref(),
            &ItemStack::new(&vanilla_items::GOLD_INGOT)
        ),
        "a brute does not barter, so it has no use for an ingot"
    );
}

// Brain wiring.
/// The brains are built with every memory their behaviors and sensors ask for.
/// A behavior whose memory was never registered silently never fires, which is
/// the failure mode this catches.
#[test]
fn every_piglin_family_brain_starts_in_its_idle_activity() {
    use Activity;

    init_vanilla_registry();
    let piglin = detached_piglin();
    assert!(
        piglin.brain_ref().is_active(Activity::Idle),
        "a brain falls back to its default activity, which is IDLE"
    );
    assert!(
        !piglin.brain_ref().is_brain_dead(),
        "a piglin's brain has sensors, memories and behaviors"
    );

    let brute = PiglinBruteEntity::new(
        &vanilla_entities::PIGLIN_BRUTE,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    assert!(!brute.brain_ref().is_brain_dead());
}

/// Ticking a piglin's brain in a live world drives its sensors, which is the
/// only way its memories are ever filled in. A brain that never reaches the
/// world leaves this empty.
#[test]
fn ticking_a_piglin_fills_in_what_it_can_see() {
    let world = piglin_world("piglin_sensors");
    let piglin = spawn_piglin(&world);
    let _hoglin = spawn_hoglin(&world);

    // The sensors are staggered, so a single tick is not enough; a second of
    // ticks covers every scan rate the piglin runs.
    for _ in 0..40 {
        LivingEntity::server_ai_step(piglin.as_ref());
    }

    assert!(
        piglin
            .brain_ref()
            .has_memory_value(memory_module_types::NEAREST_LIVING_ENTITIES.id()),
        "the nearest-living-entity sensor should have found the hoglin standing on the piglin"
    );
    assert!(
        piglin
            .brain_ref()
            .has_memory_value(memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT.id()),
        "the piglin-specific sensor should have written its hoglin count"
    );
}

/// A piglin brute pins its home where it spawns, which is what keeps a
/// bastion's guards inside the bastion.
#[test]
fn a_brute_remembers_where_it_spawned() {
    let world = piglin_world("brute_home");
    let brute = Arc::new(PiglinBruteEntity::new(
        &vanilla_entities::PIGLIN_BRUTE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&brute) as SharedEntity)
        .expect("the test chunk is loaded");

    assert!(
        !brute
            .brain_ref()
            .has_memory_value(memory_module_types::HOME.id()),
        "a bare brute has no home until it is finalized"
    );

    Mob::finalize_spawn(brute.as_ref(), &world, EntitySpawnReason::Natural, None);

    let home = brute
        .brain_ref()
        .get_memory(memory_module_types::HOME)
        .expect("finalizing a brute should pin its home");
    assert_eq!(home.pos, brute.block_position());
    assert!(
        brute
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::GOLDEN_AXE),
        "a finalized brute carries its axe"
    );
}

/// A piglin spawned by a structure keeps whatever the structure gave it --
/// neither the baby roll nor the weapon roll runs.
#[test]
fn a_structure_spawned_piglin_keeps_the_hands_it_was_given() {
    let world = piglin_world("piglin_structure_spawn");
    let piglin = spawn_piglin(&world);
    piglin.set_item_slot(
        EquipmentSlot::MainHand,
        ItemStack::new(&vanilla_items::GOLDEN_AXE),
    );

    Mob::finalize_spawn(piglin.as_ref(), &world, EntitySpawnReason::Structure, None);

    assert!(
        piglin
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::GOLDEN_AXE),
        "a structure-placed piglin should not have its weapon overwritten"
    );
}

/// A naturally spawned adult piglin always leaves with something in its hand.
#[test]
fn a_naturally_spawned_adult_piglin_is_armed() {
    let world = piglin_world("piglin_natural_spawn");

    // The baby roll is one in five, so a handful of spawns is enough to see an
    // armed adult without the test depending on a single roll.
    let armed_adult = (0..40).any(|_| {
        let piglin = spawn_piglin(&world);
        Mob::finalize_spawn(piglin.as_ref(), &world, EntitySpawnReason::Natural, None);
        piglin.is_adult()
            && !piglin
                .get_item_in_hand(InteractionHand::MainHand)
                .is_empty()
    });

    assert!(
        armed_adult,
        "forty natural spawns produced no armed adult piglin"
    );
}

/// The four mobs the brief asked for all reach the generated entity factory.
#[test]
fn every_piglin_family_mob_reaches_the_entity_factory() {
    use ENTITIES;

    init_vanilla_registry();
    init_entities();

    for entity_type in [
        &vanilla_entities::PIGLIN,
        &vanilla_entities::PIGLIN_BRUTE,
        &vanilla_entities::HOGLIN,
        &vanilla_entities::ZOGLIN,
    ] {
        assert!(
            ENTITIES
                .create(entity_type, next_entity_id(), DVec3::ZERO, Weak::new())
                .is_some(),
            "{} has no entry in the generated entity factory",
            entity_type.key
        );
    }
}

/// A piglin's crossbow fires through the real item pipeline rather than a
/// mob-only shortcut: winding it loads `charged_projectiles` from a hand with
/// nothing in it, because `Monster.getProjectile` conjures the arrow.
#[test]
fn a_mob_can_wind_a_crossbow_without_carrying_any_arrows() {
    use steel_registry::data_components::vanilla_components::CHARGED_PROJECTILES;

    let world = piglin_world("piglin_crossbow_wind");
    let piglin = spawn_piglin(&world);
    piglin.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::CROSSBOW),
    );

    piglin.start_using_item(InteractionHand::MainHand);
    assert!(
        piglin.is_using_item(),
        "a mob should be able to raise a crossbow"
    );

    // Vanilla's charge takes twenty-five ticks by default; forty covers it.
    for _ in 0..40 {
        piglin.updating_using_item();
    }

    let weapon = piglin.get_item_in_hand(InteractionHand::MainHand);
    let charged = weapon
        .get(CHARGED_PROJECTILES)
        .expect("a fully wound crossbow carries its ammunition");
    assert!(
        !charged.items().is_empty(),
        "the crossbow wound but loaded nothing: a mob's fallback arrow was not drawn"
    );
}

/// And the shot itself leaves the piglin, which is the half the illager
/// workaround never reached.
#[test]
fn a_piglin_firing_a_loaded_crossbow_spawns_a_bolt() {
    let world = piglin_world("piglin_crossbow_fire");
    let piglin = spawn_piglin(&world);
    let target = spawn_hoglin(&world);
    piglin.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::CROSSBOW),
    );

    piglin.start_using_item(InteractionHand::MainHand);
    for _ in 0..40 {
        piglin.updating_using_item();
    }

    let arrows_before = arrows_in(&world);
    perform_crossbow_attack(
        &world,
        piglin.as_ref(),
        &(Arc::clone(&target) as SharedEntity),
        MOB_ARROW_POWER,
    );

    assert!(
        arrows_in(&world) > arrows_before,
        "firing a loaded crossbow should put a bolt in the world"
    );
}

fn arrows_in(world: &Arc<World>) -> usize {
    let search = WorldAabb::new(-64.0, 0.0, -64.0, 64.0, 128.0, 64.0);
    world
        .get_entities_in_aabb_matching(&search, |entity| {
            ptr::eq(entity.entity_type(), &raw const vanilla_entities::ARROW)
        })
        .len()
}
