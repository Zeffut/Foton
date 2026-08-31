//! Holding an item up, for anything alive rather than only for players.
//!
//! `LivingEntity::is_using_item` and `is_blocking` both returned a hardcoded
//! `false`, nothing called vanilla's `updatingUsingItem` from the living tick,
//! and the only way into the state machine read a player's inventory. These
//! tests come in through a mob and through the shared living tick, which is
//! the door the server actually uses.

use super::*;
use crate::behavior::{ITEM_BEHAVIORS, init_behaviors};
use crate::entity::entities::{DrownedEntity, RavagerEntity};
use crate::entity::next_entity_id;
use crate::inventory::container::Container as _;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use foton_registry::data_components::vanilla_components::BLOCKS_ATTACKS;
use foton_utils::locks::SyncMutex;

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
/// entity. Foton called the player's copy from the player alone, so a mob's
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

/// Eating spends exactly one use tick per server tick, and takes all 32.
///
/// `Player` does not reuse `LivingEntity.tick`: it overrides
/// `tick_living_entity` to call its own hand-rolled `Player::tick`, which
/// carries its own copy of the countdown call. A second `updating_using_item`
/// anywhere on that path would halve every eating time in the game and nothing
/// else in the suite would notice, because every other test drives
/// `updating_using_item` by hand rather than through the tick the world runs.
#[test]
fn eating_spends_one_use_tick_per_server_tick() {
    let world = item_use_world("item_use_eat_rate");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Eater", next_entity_id()).build();
    player.base().set_position_local(SPAWN);
    player.set_client_loaded(true);
    player.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::GOLDEN_APPLE),
    );

    player.start_using_item(InteractionHand::MainHand);
    let started = player.use_item_remaining_ticks();
    assert_eq!(
        started, 32,
        "vanilla's default `consume_seconds` of 1.6 is 32 ticks"
    );

    // The one call `World::tick_entities` makes.
    Entity::tick(player.as_ref());

    assert_eq!(
        player.use_item_remaining_ticks(),
        started - 1,
        "one server tick must spend exactly one use tick"
    );
}

/// Right-clicking armour on sends the wearer the sound of it going on.
///
/// Foton has three ways to put armour on and they share no code: the inventory
/// screen's `ArmorSlot`, a shift-click's `move_item_stack_to`, and this one,
/// `Equippable.swapWithEquipmentSlot`. Vanilla routes all three through
/// `setItemSlot` and so through `onEquipItem`; Foton's swap works straight on
/// the inventory container, so the hook has to be run around it by hand.
///
/// The assertion is on what the client is sent, not on what the server holds.
/// Every other equipment test in the suite checks the slot's contents, and the
/// slot was always right -- it was the sound that never left the server.
#[test]
fn right_clicking_armour_on_sends_the_wearer_the_equip_sound() {
    use crate::player::PlayerConnection;
    use crate::player::ResetReason;
    use foton_protocol::packets::game::SUseItem;
    use foton_registry::equipment::EquipmentSlot;
    use foton_registry::packets::play::C_SOUND;

    let world = item_use_world("item_use_equip_sound");
    let ids: Arc<SyncMutex<Vec<i32>>> = Arc::new(SyncMutex::new(Vec::new()));
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Dresser", next_entity_id())
        .connection(Arc::new(PlayerConnection::Other(Box::new(
            PacketIdRecorder {
                ids: Arc::clone(&ids),
            },
        ))))
        .build();
    player.base().set_position_local(SPAWN);
    player.set_client_loaded(true);
    assert!(
        world.add_player(Arc::clone(&player), ResetReason::InitialJoin),
        "the sound is broadcast to the world's players, so the wearer has to be one"
    );

    player.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::DIAMOND_CHESTPLATE),
    );
    ids.lock().clear();

    player.handle_use_item(SUseItem {
        hand: InteractionHand::MainHand,
        sequence: 1,
        y_rot: 0.0,
        x_rot: 0.0,
    });

    assert!(
        player
            .get_item_by_slot(EquipmentSlot::Chest)
            .is(&vanilla_items::DIAMOND_CHESTPLATE),
        "the chestplate should be worn"
    );
    assert!(
        ids.lock().contains(&C_SOUND),
        "the wearer never heard the armour go on: vanilla's          `swapWithEquipmentSlot` writes through `setItemSlot`, whose          `onEquipItem` is what plays it"
    );
}

/// Eating a notch apple on a half-empty hunger bar actually feeds the player.
///
/// The food-data arithmetic already had a test, and so did the item's own
/// component -- both pass. Neither goes through `Consumable.onConsume`, which
/// is the only thing the server actually runs when a player finishes eating,
/// and which reaches the hunger bar through a different door than the mob
/// effects an enchanted golden apple also carries. So a break there shows up
/// to a player as "the absorption hearts work but the bar never moves".
#[test]
fn a_notch_apple_refills_a_half_empty_hunger_bar() {
    let world = item_use_world("item_use_notch_apple");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Eater", next_entity_id()).build();
    player.base().set_position_local(SPAWN);
    player.set_client_loaded(true);
    {
        let mut food = player.food_data.lock();
        food.food_level = 10;
        food.saturation_level = 0.0;
    }

    let mut stack = ItemStack::new(&vanilla_items::ENCHANTED_GOLDEN_APPLE);
    let behavior = ITEM_BEHAVIORS.get_behavior(stack.item());
    behavior.finish_using(&mut stack, &world, player.as_ref());

    let food = player.food_data.lock();
    assert_eq!(
        food.food_level, 14,
        "vanilla's enchanted golden apple restores 4 nutrition"
    );
    assert!(
        food.saturation_level > 0.0,
        "and 9.6 saturation with it, clamped to the food level; got {}",
        food.saturation_level
    );
}

/// Holding a notch apple down for its whole 1.6 seconds feeds the player.
///
/// The two tests above take the arithmetic and `Consumable.onConsume` apart
/// and both pass. This is the journey between them: `start_using_item`, then
/// the ticks the world really runs, then the `finish_using` the countdown is
/// supposed to reach. Nothing else in the suite eats a whole item this way.
#[test]
fn holding_a_notch_apple_down_for_its_full_duration_feeds_the_player() {
    let world = item_use_world("item_use_eat_end_to_end");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Eater", next_entity_id()).build();
    player.base().set_position_local(SPAWN);
    player.set_client_loaded(true);
    {
        let mut food = player.food_data.lock();
        food.food_level = 10;
        food.saturation_level = 0.0;
    }
    player.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::ENCHANTED_GOLDEN_APPLE),
    );

    player.start_using_item(InteractionHand::MainHand);
    let duration = player.use_item_remaining_ticks();
    assert_eq!(duration, 32, "vanilla eats in 1.6 seconds");

    for _ in 0..duration {
        Entity::tick(player.as_ref());
    }

    assert!(
        !player.is_using_item(),
        "the countdown should have run out and finished the meal"
    );
    let food = player.food_data.lock();
    assert_eq!(
        food.food_level, 14,
        "the hunger bar has to move: a player who eats and stays hungry is the \
         whole bug report"
    );
    assert!(food.saturation_level > 0.0, "and saturation with it");
}

/// Saturation gained at full health and full hunger reaches the client.
///
/// This is the state where nothing else moves: health is capped, the hunger
/// bar is full, and eating changes saturation alone. Vanilla remembers only
/// whether saturation *was zero*, so it sends nothing here and the value a
/// client holds quietly goes stale -- which is exactly what a player sees as
/// "I stop gaining saturation once I'm at full health", because while they are
/// hurt the regeneration keeps health moving and the packet keeps flowing.
///
/// The assertion is on the packet, not on the server's own number: the server
/// number was always right, and three separate tests already prove it.
#[test]
fn saturation_gained_at_full_health_reaches_the_client() {
    use crate::player::PlayerConnection;
    use crate::player::ResetReason;
    use foton_registry::packets::play::C_SET_HEALTH;

    let world = item_use_world("item_use_saturation_sync");
    let ids: Arc<SyncMutex<Vec<i32>>> = Arc::new(SyncMutex::new(Vec::new()));
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Eater", next_entity_id())
        .connection(Arc::new(PlayerConnection::Other(Box::new(
            PacketIdRecorder {
                ids: Arc::clone(&ids),
            },
        ))))
        .build();
    player.base().set_position_local(SPAWN);
    player.set_client_loaded(true);
    assert!(world.add_player(Arc::clone(&player), ResetReason::InitialJoin));

    player.set_health(player.get_max_health());
    {
        let mut food = player.food_data.lock();
        food.food_level = 20;
        food.saturation_level = 5.0;
    }
    // Let the first sync go out, so what follows is only the change we make.
    Entity::tick(player.as_ref());
    ids.lock().clear();

    player.food_data.lock().eat_food(4, 9.6);
    Entity::tick(player.as_ref());

    assert!(
        ids.lock().contains(&C_SET_HEALTH),
        "the client was never told: at full health and full hunger, saturation \
         is the only value that moved, and it has to be enough to send"
    );
}

/// A Wind Burst mace swung mid-fall launches the attacker.
///
/// Three separate things have to line up before this enchantment does
/// anything, and each one failed silently on its own: the `minecraft:explode`
/// effect had to exist, the `is_flying` and `fall_distance` predicate fields
/// had to be modeled, and the blast has to reach the attacker's own client.
/// The shape test next door proves the data parsed; only this one proves the
/// server acts on it.
#[test]
fn a_wind_burst_mace_swung_mid_fall_launches_the_attacker() {
    use crate::enchantment_helper::{
        EnchantmentPostAttackContext, do_post_attack_effects_from_item,
    };
    use crate::entity::damage::DamageSource;
    use crate::player::{PlayerConnection, ResetReason};
    use foton_registry::data_components::vanilla_components::{ENCHANTMENTS, ItemEnchantments};

    use foton_registry::packets::play::C_EXPLODE;
    use foton_registry::vanilla_damage_types;
    use foton_utils::Identifier;

    let world = item_use_world("item_use_wind_burst");
    let ids: Arc<SyncMutex<Vec<i32>>> = Arc::new(SyncMutex::new(Vec::new()));
    let attacker = TestPlayerBuilder::new(Arc::clone(&world), "Smasher", next_entity_id())
        .connection(Arc::new(PlayerConnection::Other(Box::new(
            PacketIdRecorder {
                ids: Arc::clone(&ids),
            },
        ))))
        .build();
    attacker.base().set_position_local(SPAWN);
    attacker.set_client_loaded(true);
    assert!(world.add_player(Arc::clone(&attacker), ResetReason::InitialJoin));

    // Vanilla gates the burst on `fall_distance >= 1.5`: a swing at ground
    // level is supposed to do nothing, and did nothing for a different reason.
    attacker.set_fall_distance(3.0);

    let victim = spawn_drowned_at(&world, SPAWN + DVec3::new(0.0, 0.0, 1.0));

    let mut mace = ItemStack::new(&vanilla_items::MACE);
    let mut enchantments = ItemEnchantments::empty();
    enchantments.set(Identifier::vanilla_static("wind_burst"), 1);
    mace.set(ENCHANTMENTS, enchantments);

    let damage_source = DamageSource::environment(&vanilla_damage_types::PLAYER_ATTACK)
        .with_causing_entity(Entity::id(attacker.as_ref()))
        .with_direct_entity(Entity::id(attacker.as_ref()));
    let context = EnchantmentPostAttackContext::new(
        victim.as_ref(),
        Some(attacker.as_ref()),
        Some(attacker.as_ref()),
        &damage_source,
    );

    ids.lock().clear();
    do_post_attack_effects_from_item(&world, &mace, &context);

    assert!(
        ids.lock().contains(&C_EXPLODE),
        "no blast reached the attacker's client: wind burst has to raise an \
         explosion the swinging player is inside of, and it is that explosion \
         packet's knockback that actually launches them"
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
/// nothing in Foton could block. Half of its rolls stagger the ravager, so a
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
/// trident goal was left out of Foton entirely. It is the end-to-end witness
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
