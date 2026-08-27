//! What a mob does with the items lying on the ground next to it.
//!
//! `Mob.aiStep` sweeps for `ItemEntity`s and hands each to `Mob.pickUpItem`,
//! whose body equips the stack. Steel had the sweep and it had
//! `equipItemIfPossible`, but nothing joined them -- so the only mobs that
//! picked anything up were the handful overriding `pickUpItem` themselves.
//! These tests come in through `Entity::tick`, the door the server tick uses,
//! rather than calling the pickup directly.

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::entities::{ItemEntity, ZombieEntity};
use crate::entity::{EntitySpawnReason, next_entity_id};
use steel_registry::vanilla_world_clocks;
use steel_utils::types::UpdateFlags;

const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);
const STAND: BlockPos = BlockPos::new(8, 64, 8);

/// A world clock past the 72000-tick grace and well into the 1440000-tick ramp.
///
/// `DifficultyInstance.getSpecialMultiplier` is zero below effective difficulty
/// 2, and even a hard world only clears that once the global term has grown.
const AGED_WORLD_TICKS: i64 = 2_000_000;

fn pickup_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    // An item entity over a void falls out of the pickup box in a tick or two.
    for x in (STAND.x() - 2)..=(STAND.x() + 2) {
        for z in (STAND.z() - 2)..=(STAND.z() + 2) {
            assert!(world.set_block(
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

/// A hard world old enough that the loot roll can actually succeed.
fn hard_aged_pickup_world(key: &'static str) -> Arc<World> {
    let world = pickup_world(key);
    world.set_difficulty(Difficulty::Hard);
    world
        .level_data
        .write()
        .world_clocks_mut()
        .set_total_ticks(&vanilla_world_clocks::OVERWORLD, AGED_WORLD_TICKS)
        .expect("the overworld clock exists in a test world");
    assert!(
        world.get_current_difficulty_at(STAND).special_multiplier() > 0.0,
        "these tests need a world whose special multiplier has left the floor"
    );
    world
}

fn new_zombie(world: &Arc<World>) -> Arc<ZombieEntity> {
    Arc::new(ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ))
}

fn spawn_zombie(world: &Arc<World>) -> Arc<ZombieEntity> {
    let zombie = new_zombie(world);
    world
        .try_add_entity(Arc::clone(&zombie) as SharedEntity)
        .expect("the test chunk is loaded, so the zombie should attach");
    zombie
}

fn drop_at_feet(world: &Arc<World>, stack: ItemStack) -> Arc<ItemEntity> {
    let item = world
        .spawn_item(SPAWN, stack)
        .expect("the test chunk accepts an item entity");
    // `spawn_item` gives the stack the vanilla pickup delay and a pop of
    // velocity. Neither belongs in a test about who ends up holding it.
    item.set_no_pickup_delay();
    item.set_velocity(DVec3::ZERO);
    item
}

/// Ticks the zombie the way the server would, watching for `reached`.
fn tick_until(zombie: &Arc<ZombieEntity>, ticks: i32, reached: impl Fn() -> bool) -> bool {
    for _ in 0..ticks {
        Entity::tick(zombie.as_ref());
        if reached() {
            return true;
        }
    }
    false
}

fn main_hand(zombie: &Arc<ZombieEntity>) -> ItemStack {
    LivingEntity::get_item_by_slot(zombie.as_ref(), EquipmentSlot::MainHand)
}

fn head(zombie: &Arc<ZombieEntity>) -> ItemStack {
    LivingEntity::get_item_by_slot(zombie.as_ref(), EquipmentSlot::Head)
}

/// The whole point: a zombie that walks over your sword ends up swinging it.
#[test]
fn a_zombie_that_may_loot_picks_up_the_sword_you_dropped() {
    let world = pickup_world("mob_pickup_sword");
    let zombie = spawn_zombie(&world);
    Mob::set_can_pick_up_loot(zombie.as_ref(), true);
    let dropped = drop_at_feet(&world, ItemStack::new(&vanilla_items::IRON_SWORD));

    let taken = tick_until(&zombie, 20, || dropped.is_removed());

    assert!(taken, "the sword should have left the ground");
    assert!(
        main_hand(&zombie).is(&vanilla_items::IRON_SWORD),
        "the sword has to end up in the zombie's hand, not merely vanish"
    );
}

/// The `canPickUpLoot` gate is the difference between a survival world where
/// the gear you dropped stays put and one where it walks away.
#[test]
fn a_zombie_that_may_not_loot_leaves_the_sword_alone() {
    let world = pickup_world("mob_pickup_gate");
    let zombie = spawn_zombie(&world);
    Mob::set_can_pick_up_loot(zombie.as_ref(), false);
    let dropped = drop_at_feet(&world, ItemStack::new(&vanilla_items::IRON_SWORD));

    tick_until(&zombie, 20, || dropped.is_removed());

    assert!(
        !dropped.is_removed(),
        "nothing may take an item while `canPickUpLoot` is false"
    );
    assert!(
        main_hand(&zombie).is_empty(),
        "and nothing may reach the zombie's hand either"
    );
}

/// An armor slot holds one piece, so the rest of the pile stays on the floor.
///
/// This is the half of `Mob.pickUpItem` that is easy to get wrong: vanilla
/// shrinks the ground stack by what was equipped and discards the entity only
/// once nothing is left. Swallowing all three, or leaving all three, fails
/// here. One tick, because on the tick after this one the zombie reaches for a
/// second helmet with its free hand.
#[test]
fn a_zombie_takes_one_helmet_out_of_a_pile_and_leaves_the_rest() {
    let world = pickup_world("mob_pickup_partial_stack");
    let zombie = spawn_zombie(&world);
    Mob::set_can_pick_up_loot(zombie.as_ref(), true);
    let dropped = drop_at_feet(
        &world,
        ItemStack::with_count(&vanilla_items::IRON_HELMET, 3),
    );

    Entity::tick(zombie.as_ref());

    assert!(
        head(&zombie).is(&vanilla_items::IRON_HELMET),
        "a bare-headed zombie should have put a helmet on within its first tick"
    );
    assert!(
        !dropped.is_removed(),
        "two helmets are left, so the item entity has to survive"
    );
    assert_eq!(
        dropped.get_item().count(),
        2,
        "exactly one helmet may leave the pile"
    );
}

/// Vanilla parity: the glow ink sac of `Zombie.wantsToPickUp`, the one drop a
/// zombie walks past. Without that override the shared sweep would take it.
#[test]
fn a_zombie_walks_past_a_glow_ink_sac() {
    let world = pickup_world("mob_pickup_glow_ink");
    let zombie = spawn_zombie(&world);
    Mob::set_can_pick_up_loot(zombie.as_ref(), true);
    let dropped = drop_at_feet(&world, ItemStack::new(&vanilla_items::GLOW_INK_SAC));

    tick_until(&zombie, 20, || dropped.is_removed());

    assert!(
        !dropped.is_removed(),
        "a glow ink sac is the one drop a zombie leaves where it fell"
    );
    assert!(main_hand(&zombie).is_empty(), "and it never reaches a hand");
}

/// `Zombie.finalizeSpawn` is the only place a zombie's `canPickUpLoot` comes
/// from, and it is a roll rather than a constant.
///
/// The odds are `0.55 * getSpecialMultiplier()`, so roughly one zombie in four
/// in this world. Over this many spawns, seeing none armed or all armed does
/// not happen unless the roll is gone or has become a constant.
#[test]
fn spawning_into_a_hard_aged_world_arms_some_zombies_and_not_others() {
    const SPAWNS: usize = 400;

    let world = hard_aged_pickup_world("mob_pickup_spawn_roll");
    let armed = (0..SPAWNS)
        .filter(|_| {
            let zombie = new_zombie(&world);
            Mob::finalize_spawn(zombie.as_ref(), &world, EntitySpawnReason::Natural, None);
            Mob::can_pick_up_loot(zombie.as_ref())
        })
        .count();

    assert!(
        armed > 0,
        "not one zombie in {SPAWNS} was armed to loot: `finalizeSpawn` never rolls it"
    );
    assert!(
        armed < SPAWNS,
        "every zombie was armed to loot, which is not the vanilla roll"
    );
}

/// Vanilla parity: the `spawnReason != CONVERSION` guard of
/// `Zombie.finalizeSpawn`. A zombie that drowned keeps the flag it already had.
#[test]
fn a_conversion_never_rerolls_the_loot_flag() {
    const SPAWNS: usize = 400;

    let world = hard_aged_pickup_world("mob_pickup_conversion");
    for _ in 0..SPAWNS {
        let zombie = new_zombie(&world);
        Mob::set_can_pick_up_loot(zombie.as_ref(), true);
        Mob::finalize_spawn(zombie.as_ref(), &world, EntitySpawnReason::Conversion, None);
        assert!(
            Mob::can_pick_up_loot(zombie.as_ref()),
            "a conversion has to carry the old flag over rather than reroll it"
        );
    }
}
