//! Holding an item up, for anything alive rather than only for players.
//!
//! `LivingEntity::is_using_item` and `is_blocking` both returned a hardcoded
//! `false`, nothing called vanilla's `updatingUsingItem` from the living tick,
//! and the only way into the state machine read a player's inventory. These
//! tests come in through a mob and through the shared living tick, which is
//! the door the server actually uses.

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::entities::{DrownedEntity, RavagerEntity};
use crate::entity::next_entity_id;
use crate::inventory::container::Container as _;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use steel_registry::data_components::vanilla_components::BLOCKS_ATTACKS;

/// The one spot the test world is solid ground rather than a column the fluid
/// scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn item_use_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn spawn_drowned_at(world: &Arc<World>, position: DVec3) -> Arc<DrownedEntity> {
    let drowned = Arc::new(DrownedEntity::new(
        &vanilla_entities::DROWNED,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&drowned) as SharedEntity)
        .expect("the test chunk is loaded, so the drowned should attach");
    drowned
}

fn spawn_drowned(world: &Arc<World>) -> Arc<DrownedEntity> {
    spawn_drowned_at(world, SPAWN)
}

/// Ticks the shield has to be up before it stops anything.
///
/// Read off the component rather than written down, so a change to the
/// extracted shield data moves the test with it.
fn shield_block_delay_ticks() -> i32 {
    ItemStack::new(&vanilla_items::SHIELD)
        .get(BLOCKS_ATTACKS)
        .expect("the vanilla shield carries blocks_attacks")
        .block_delay_ticks()
}

/// Raises a shield and holds it until it is actually blocking.
fn raise_a_shield(defender: &dyn LivingEntity) {
    defender.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::SHIELD),
    );
    defender.start_using_item(InteractionHand::MainHand);
    for _ in 0..shield_block_delay_ticks() {
        defender.updating_using_item();
    }
}

fn hit_from(attacker: &dyn Entity) -> DamageSource {
    DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        .with_causing_entity(attacker.id())
        .with_direct_entity(attacker.id())
        .with_source_position(attacker.position())
}

/// A mob asked to hold an item up says so afterwards.
///
/// `LivingEntity::is_using_item` used to answer `false` no matter what, so the
/// state machine underneath it was invisible to everything but the player.
#[test]
fn a_mob_that_raises_an_item_counts_as_using_it() {
    let world = item_use_world("item_use_mob_raises");
    let drowned = spawn_drowned(&world);
    drowned.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::TRIDENT),
    );

    assert!(!drowned.is_using_item(), "nothing raised yet");

    drowned.start_using_item(InteractionHand::MainHand);

    assert!(LivingEntity::is_using_item(drowned.as_ref()));
    assert_eq!(
        drowned.active_item_use_hand(),
        Some(InteractionHand::MainHand)
    );
}

/// The living tick is what carries a raised item forward.
///
/// Vanilla calls `updatingUsingItem` from `LivingEntity.tick` for every living
/// entity. Steel called the player's copy from the player alone, so a mob's
/// wind-up sat frozen at its full duration forever.
#[test]
fn the_living_tick_counts_a_mobs_raised_item_down() {
    const TICKS: i32 = 5;

    let world = item_use_world("item_use_tick_counts_down");
    let drowned = spawn_drowned(&world);
    drowned.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::TRIDENT),
    );
    drowned.start_using_item(InteractionHand::MainHand);

    let started_at = drowned
        .living_base()
        .active_item_use()
        .expect("the trident is up")
        .remaining_ticks();

    for _ in 0..TICKS {
        drowned.tick();
    }

    let now = drowned
        .living_base()
        .active_item_use()
        .expect("the trident is still up")
        .remaining_ticks();
    assert_eq!(
        started_at - now,
        TICKS,
        "the living tick has to spend one tick of the wind-up per tick"
    );
}

/// Taking the item out of the hand ends the use on the next tick.
///
/// Vanilla's `updatingUsingItem` compares the hand against the stack the use
/// started with and stops when they differ; without that a mob keeps the pose
/// of an item it no longer holds.
#[test]
fn a_mob_stops_using_an_item_that_leaves_its_hand() {
    let world = item_use_world("item_use_item_leaves_hand");
    let drowned = spawn_drowned(&world);
    drowned.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::TRIDENT),
    );
    drowned.start_using_item(InteractionHand::MainHand);
    assert!(drowned.is_using_item());

    drowned.set_item_in_hand(InteractionHand::MainHand, ItemStack::empty());
    drowned.tick();

    assert!(!drowned.is_using_item());
}

/// The living hand seam reads a player's selected hotbar slot.
///
/// A player's inventory *is* its equipment storage, which is why `Player` needs
/// no override here. If someone gives it one that reads a fixed slot, this
/// catches it.
#[test]
fn a_players_living_hand_follows_the_selected_hotbar_slot() {
    let world = item_use_world("item_use_player_hand");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "HandReader", next_entity_id()).build();

    player.inventory.lock().set_selected_slot(4);
    player.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::TRIDENT),
    );

    assert!(
        player
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::TRIDENT)
    );
    assert!(
        player
            .inventory
            .lock()
            .get_item(4)
            .is(&vanilla_items::TRIDENT),
        "the seam has to write through to the selected slot itself"
    );

    player.inventory.lock().set_selected_slot(5);
    assert!(
        player
            .get_item_in_hand(InteractionHand::MainHand)
            .is_empty(),
        "a different slot is a different hand"
    );
}

/// A shield only blocks once it has been up for its block delay.
#[test]
fn a_shield_raised_this_very_tick_does_not_block_yet() {
    let world = item_use_world("item_use_block_delay");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Blocker", next_entity_id()).build();
    player.base().set_position_local(SPAWN);
    player.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::SHIELD),
    );

    player.start_using_item(InteractionHand::MainHand);
    assert!(!player.is_blocking(), "the shield is not up yet");

    for _ in 0..shield_block_delay_ticks() {
        player.updating_using_item();
    }
    assert!(player.is_blocking());
}

/// A raised shield eats a melee hit that comes at the front of it.
#[test]
fn a_raised_shield_swallows_a_hit_from_the_front() {
    let world = item_use_world("item_use_block_front");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Blocker", next_entity_id()).build();
    player.base().set_position_local(SPAWN);
    player.set_y_head_rot(0.0);
    // A player whose client has not finished loading is invulnerable to
    // everything, which would make every one of these assertions vacuous.
    player.set_client_loaded(true);
    raise_a_shield(player.as_ref());

    // Zero yaw looks down +Z, so this attacker is dead ahead.
    let attacker = spawn_drowned_at(&world, SPAWN + DVec3::new(0.0, 0.0, 3.0));

    let health_before = player.get_health();
    let hurt = player.hurt_server(&world, &hit_from(attacker.as_ref()), 6.0);

    assert!(
        !hurt,
        "a hit a shield swallows whole never counts as damage"
    );
    assert!((player.get_health() - health_before).abs() < f32::EPSILON);
}

/// The same hit from behind goes straight through.
///
/// This is the half that makes the front-facing test mean something: without
/// the angle check both would pass with blocking hardcoded on.
#[test]
fn a_raised_shield_does_nothing_against_a_hit_from_behind() {
    let world = item_use_world("item_use_block_behind");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Blocker", next_entity_id()).build();
    player.base().set_position_local(SPAWN);
    player.set_y_head_rot(0.0);
    // A player whose client has not finished loading is invulnerable to
    // everything, which would make every one of these assertions vacuous.
    player.set_client_loaded(true);
    raise_a_shield(player.as_ref());

    let attacker = spawn_drowned_at(&world, SPAWN - DVec3::new(0.0, 0.0, 3.0));

    let health_before = player.get_health();
    let hurt = player.hurt_server(&world, &hit_from(attacker.as_ref()), 6.0);

    assert!(hurt);
    assert!(player.get_health() < health_before);
}

/// A ravager that runs into a raised shield eventually staggers.
///
/// `Ravager::blocked_by_item` was written whole and never called, because
/// nothing in Steel could block. Half of its rolls stagger the ravager, so a
/// run of blocked hits that never produces one means the shield path is not
/// reaching the attacker at all.
#[test]
fn a_ravager_that_runs_into_a_raised_shield_is_eventually_stunned() {
    // One in two rolls staggers, so forty blocked hits without one would be a
    // one-in-a-trillion accident rather than a passing implementation.
    const ATTEMPTS: usize = 40;

    let world = item_use_world("item_use_ravager_stun");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Blocker", next_entity_id()).build();
    player.base().set_position_local(SPAWN);
    player.set_y_head_rot(0.0);
    // A player whose client has not finished loading is invulnerable to
    // everything, which would make every one of these assertions vacuous.
    player.set_client_loaded(true);
    raise_a_shield(player.as_ref());

    let ravager = Arc::new(RavagerEntity::new(
        &vanilla_entities::RAVAGER,
        next_entity_id(),
        SPAWN + DVec3::new(0.0, 0.0, 3.0),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&ravager) as SharedEntity)
        .expect("the test chunk is loaded, so the ravager should attach");

    let source = hit_from(ravager.as_ref());
    for _ in 0..ATTEMPTS {
        if ravager.stunned_tick() > 0 {
            return;
        }
        player.living_base().set_invulnerable_time(0);
        assert!(
            player.is_blocking(),
            "the shield stays up across the whole run"
        );
        player.hurt_server(&world, &source, 6.0);
    }

    panic!("a ravager blocked {ATTEMPTS} times over should have staggered at least once");
}

fn tridents_in_flight(world: &Arc<World>) -> usize {
    world
        .get_entities_in_aabb(&WorldAabb::new(-16.0, 0.0, -16.0, 32.0, 128.0, 32.0))
        .iter()
        .filter(|entity| entity.entity_type() == &vanilla_entities::TRIDENT)
        .count()
}

/// A drowned holding a trident winds it up and throws it.
///
/// The drowned is vanilla's one monster that calls `startUsingItem`, and its
/// trident goal was left out of Steel entirely. It is the end-to-end witness
/// that a mob can now reach the use-item state machine: the goal starts the
/// wind-up, the shared living tick carries it, and the ranged attack lands.
#[test]
fn a_drowned_with_a_trident_winds_it_up_and_throws_it() {
    // The interval between throws is forty ticks and the first one waits it
    // out, so the run has to be longer than that.
    const TICKS: usize = 60;

    let world = item_use_world("item_use_drowned_trident");
    let drowned = spawn_drowned(&world);
    drowned.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::TRIDENT),
    );

    let quarry = TestPlayerBuilder::new(Arc::clone(&world), "Quarry", next_entity_id()).build();
    quarry
        .base()
        .set_position_local(SPAWN + DVec3::new(0.0, 0.0, 6.0));
    let quarry: SharedEntity = quarry;

    let mut wound_up = false;
    for _ in 0..TICKS {
        // Held steady: the target selector would otherwise drop a player the
        // test never registered with the world.
        drowned.set_target(Some(&quarry));
        drowned.tick();
        wound_up |= drowned.is_using_item();
    }

    assert!(wound_up, "the trident goal has to start the wind-up");
    assert_eq!(
        tridents_in_flight(&world),
        1,
        "and the ranged attack has to actually throw one"
    );
}
