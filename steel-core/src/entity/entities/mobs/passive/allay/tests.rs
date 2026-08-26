//! Allay tests.

use steel_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_items};
use steel_utils::ChunkPos;
use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

const TEST_POS: BlockPos = BlockPos::new(8, 64, 8);
const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn allay_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(TEST_POS));
    world
}

fn live_allay(world: &Arc<World>) -> Arc<AllayEntity> {
    let allay = Arc::new(AllayEntity::new(
        &vanilla_entities::ALLAY,
        next_entity_id(),
        TEST_POSITION,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&allay) as SharedEntity)
        .unwrap_or_else(|error| panic!("allay should enter the test world: {error:?}"));
    allay
}

fn test_player(world: &Arc<World>) -> Arc<Player> {
    TestPlayerBuilder::new(Arc::clone(world), "AllayFriend", next_entity_id()).build()
}

/// Puts a player in the world's entity index, which is where a damage source's
/// attacker id is resolved from.
fn live_test_player(world: &Arc<World>) -> Arc<Player> {
    let player = test_player(world);
    player
        .try_set_position(TEST_POSITION)
        .unwrap_or_else(|error| panic!("test player should be placed: {error}"));
    player.set_old_position_to_current();
    world
        .try_add_entity(Arc::clone(&player) as SharedEntity)
        .unwrap_or_else(|error| panic!("player should enter the test world: {error:?}"));
    player
}

fn give(player: &Player, stack: ItemStack) {
    player
        .inventory
        .lock()
        .set_item_in_hand(InteractionHand::MainHand, stack);
}

#[test]
fn handing_an_allay_an_item_makes_it_yours() {
    // The `LIKED_PLAYER` memory is the whole relationship: it is what the allay
    // brings things back to, and it is why the same player can no longer hurt it.
    let world = allay_world("allay_given_item");
    let allay = live_allay(&world);
    let player = test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::DIAMOND));

    let result = allay.mob_interact(&player, InteractionHand::MainHand);

    assert_eq!(result, InteractionResult::Success);
    assert!(allay.has_item_in_hand());
    assert!(
        allay
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::DIAMOND)
    );
    assert_eq!(
        allay.brain.get_memory(memory_module_types::LIKED_PLAYER),
        Some(player.uuid())
    );
    assert!(allay.is_liked_player(Some(player.as_ref() as &dyn Entity)));
}

#[test]
fn an_allay_will_not_be_hurt_by_the_player_it_fetches_for() {
    // Vanilla's `hurtServer` returns false outright rather than cancelling the
    // damage later, so a stray swing costs the player nothing at all.
    let world = allay_world("allay_liked_player_damage");
    let allay = live_allay(&world);
    let player = live_test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::DIAMOND));
    allay.mob_interact(&player, InteractionHand::MainHand);

    let from_owner = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        .with_causing_entity(player.id());
    let health_before = allay.get_health();

    assert!(!allay.hurt_server(&world, &from_owner, 4.0));
    assert!((allay.get_health() - health_before).abs() < f32::EPSILON);

    // A stranger still hurts it.
    let stranger = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        .with_causing_entity(player.id() + 1000);
    assert!(allay.hurt_server(&world, &stranger, 4.0));
    assert!(allay.get_health() < health_before);
}

#[test]
fn taking_an_allays_item_back_returns_everything_it_gathered() {
    // The item in hand goes back into the player's inventory and the gathered
    // stack is thrown on the ground; an allay that kept either would be a
    // one-way hole.
    let world = allay_world("allay_item_taken");
    let allay = live_allay(&world);
    let player = test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::DIAMOND));
    allay.mob_interact(&player, InteractionHand::MainHand);

    allay
        .inventory
        .lock()
        .set_item(0, ItemStack::with_count(&vanilla_items::DIAMOND, 5));
    give(&player, ItemStack::empty());

    let dropped_before = world
        .get_entities_in_aabb_matching(&allay.bounding_box().inflate(8.0), |entity| {
            entity.entity_type() == &vanilla_entities::ITEM
        })
        .len();

    let result = allay.mob_interact(&player, InteractionHand::MainHand);

    assert_eq!(result, InteractionResult::Success);
    assert!(!allay.has_item_in_hand());
    assert!(allay.inventory.lock().is_empty());
    assert!(
        allay
            .brain
            .get_memory(memory_module_types::LIKED_PLAYER)
            .is_none(),
        "an allay whose item was taken back forgets who gave it"
    );
    let dropped_after = world
        .get_entities_in_aabb_matching(&allay.bounding_box().inflate(8.0), |entity| {
            entity.entity_type() == &vanilla_entities::ITEM
        })
        .len();
    assert_eq!(
        dropped_after,
        dropped_before + 1,
        "the gathered stack is thrown out"
    );
}

/// Counts the allays in the test world.
fn count_allays(world: &Arc<World>) -> usize {
    world
        .get_entities_in_aabb_matching(
            &steel_utils::WorldAabb::new(
                TEST_POSITION.x - 8.0,
                TEST_POSITION.y - 8.0,
                TEST_POSITION.z - 8.0,
                TEST_POSITION.x + 8.0,
                TEST_POSITION.y + 8.0,
                TEST_POSITION.z + 8.0,
            ),
            |entity| entity.entity_type() == &vanilla_entities::ALLAY,
        )
        .len()
}

#[test]
fn a_dancing_allay_given_an_amethyst_shard_becomes_two() {
    // Both allays then owe five minutes before either can do it again, which is
    // what stops one shard becoming an army.
    let world = allay_world("allay_duplication");
    let allay = live_allay(&world);
    let player = test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::AMETHYST_SHARD));
    let before = count_allays(&world);

    assert!(allay.can_duplicate());
    allay.set_dancing(true);
    assert_eq!(
        allay.mob_interact(&player, InteractionHand::MainHand),
        InteractionResult::Success
    );

    assert_eq!(count_allays(&world), before + 1);
    assert!(!allay.can_duplicate(), "the parent owes a cooldown");
    assert_eq!(allay.duplication_cooldown(), 6000);
    assert!(
        !allay.has_item_in_hand(),
        "a shard that duplicated is spent, not taken as a fetch item"
    );
}

#[test]
fn an_allay_that_is_not_dancing_takes_the_shard_instead_of_duplicating() {
    // The dance is the gate: an allay standing still treats an amethyst shard
    // as one more thing to fetch.
    let world = allay_world("allay_duplication_not_dancing");
    let allay = live_allay(&world);
    let player = test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::AMETHYST_SHARD));
    let before = count_allays(&world);

    assert!(!allay.is_dancing());
    assert_eq!(
        allay.mob_interact(&player, InteractionHand::MainHand),
        InteractionResult::Success
    );

    assert_eq!(count_allays(&world), before);
    assert!(allay.can_duplicate(), "nothing was duplicated");
    assert!(
        allay
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::AMETHYST_SHARD),
        "the shard became a fetch item instead"
    );
}

#[test]
fn only_an_amethyst_shard_duplicates_a_dancing_allay() {
    // `#minecraft:duplicates_allays` is one item long. A dancing allay handed
    // anything else simply takes it.
    let world = allay_world("allay_duplication_wrong_item");
    let allay = live_allay(&world);
    let player = test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::DIAMOND));
    let before = count_allays(&world);

    allay.set_dancing(true);
    assert_eq!(
        allay.mob_interact(&player, InteractionHand::MainHand),
        InteractionResult::Success
    );

    assert_eq!(count_allays(&world), before);
    assert!(allay.can_duplicate(), "a diamond duplicates nothing");
    assert!(
        allay
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::DIAMOND)
    );
}

#[test]
fn an_allay_on_cooldown_will_not_duplicate_again() {
    // The five minutes is the only thing between one shard and the next.
    let world = allay_world("allay_duplication_cooldown");
    let allay = live_allay(&world);
    let player = test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::AMETHYST_SHARD));
    let before = count_allays(&world);

    allay.set_dancing(true);
    allay.reset_duplication_cooldown();
    assert!(!allay.can_duplicate());

    assert_eq!(
        allay.mob_interact(&player, InteractionHand::MainHand),
        InteractionResult::Success
    );

    assert_eq!(count_allays(&world), before);
    assert!(
        allay
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::AMETHYST_SHARD),
        "an allay on cooldown takes the shard rather than spending it"
    );
}

#[test]
fn an_allay_only_fetches_more_of_what_it_is_already_holding() {
    // `allayConsidersItemEqual` is what stops an allay handed a diamond filling
    // up with dirt -- and the potion half is what stops it confusing two brews.
    init_vanilla_registry();
    let diamond = ItemStack::new(&vanilla_items::DIAMOND);
    let other_diamond = ItemStack::with_count(&vanilla_items::DIAMOND, 4);
    let dirt = ItemStack::new(&vanilla_items::DIRT);

    assert!(AllayEntity::considers_item_equal(&diamond, &other_diamond));
    assert!(!AllayEntity::considers_item_equal(&diamond, &dirt));
}

#[test]
fn an_empty_handed_allay_picks_nothing_up() {
    // Vanilla gates the whole pickup loop on `canPickUpLoot`, which is false
    // until somebody hands the allay something -- otherwise every allay would
    // hoover up every dropped item in the world.
    let world = allay_world("allay_pickup_gate");
    let allay = live_allay(&world);
    assert!(!Mob::can_pick_up_loot(allay.as_ref()));

    let player = test_player(&world);
    give(&player, ItemStack::new(&vanilla_items::DIAMOND));
    allay.mob_interact(&player, InteractionHand::MainHand);

    assert!(Mob::can_pick_up_loot(allay.as_ref()));
    assert!(allay.wants_to_pick_up(&world, &ItemStack::new(&vanilla_items::DIAMOND)));
    assert!(!allay.wants_to_pick_up(&world, &ItemStack::new(&vanilla_items::DIRT)));

    // The cooldown it earns after throwing is the other half of the gate.
    allay
        .brain
        .set_memory(memory_module_types::ITEM_PICKUP_COOLDOWN_TICKS, 60);
    assert!(!Mob::can_pick_up_loot(allay.as_ref()));
}

#[test]
fn an_allay_hears_a_note_block_and_serves_it_until_the_clock_runs_out() {
    // The note block is what an allay deposits at instead of a player, and the
    // ten-second clock is what makes it stop when nobody plays it any more.
    let world = allay_world("allay_noteblock");
    let allay = live_allay(&world);
    assert!(world.set_block(
        TEST_POS,
        vanilla_blocks::NOTE_BLOCK.default_state(),
        UpdateFlags::UPDATE_NONE
    ));

    world.game_event(
        &vanilla_game_events::NOTE_BLOCK_PLAY,
        TEST_POS,
        &GameEventContext::new(None, None),
    );

    assert_eq!(
        allay
            .brain
            .get_memory(memory_module_types::LIKED_NOTEBLOCK_POSITION)
            .map(|liked| liked.pos),
        Some(TEST_POS),
        "an allay in range should remember the note block it heard"
    );
    assert_eq!(
        allay
            .brain
            .get_memory(memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS),
        Some(600)
    );

    // A second note block somewhere else does not steal it.
    let other = BlockPos::new(TEST_POS.x() + 3, TEST_POS.y(), TEST_POS.z());
    assert!(world.set_block(
        other,
        vanilla_blocks::NOTE_BLOCK.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    world.game_event(
        &vanilla_game_events::NOTE_BLOCK_PLAY,
        other,
        &GameEventContext::new(None, None),
    );

    assert_eq!(
        allay
            .brain
            .get_memory(memory_module_types::LIKED_NOTEBLOCK_POSITION)
            .map(|liked| liked.pos),
        Some(TEST_POS),
        "an allay keeps the first note block it liked"
    );
}

#[test]
fn hearing_a_second_note_block_does_not_move_an_allay_to_it() {
    // This is `hearNoteblock` itself rather than the listener filter above it:
    // even asked directly, an allay that already likes a note block keeps it
    // and refuses to refresh the clock for anything else.
    let world = allay_world("allay_hear_noteblock");
    let allay = live_allay(&world);
    let other = BlockPos::new(TEST_POS.x() + 3, TEST_POS.y(), TEST_POS.z());

    allay_ai::hear_noteblock(&allay.brain, &world, TEST_POS);
    allay
        .brain
        .set_memory(memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS, 100);

    allay_ai::hear_noteblock(&allay.brain, &world, other);

    assert_eq!(
        allay
            .brain
            .get_memory(memory_module_types::LIKED_NOTEBLOCK_POSITION)
            .map(|liked| liked.pos),
        Some(TEST_POS),
        "the first note block keeps the allay"
    );
    assert_eq!(
        allay
            .brain
            .get_memory(memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS),
        Some(100),
        "another note block does not refresh the clock either"
    );

    // The one it does like refreshes it.
    allay_ai::hear_noteblock(&allay.brain, &world, TEST_POS);
    assert_eq!(
        allay
            .brain
            .get_memory(memory_module_types::LIKED_NOTEBLOCK_COOLDOWN_TICKS),
        Some(600)
    );
}

#[test]
fn a_jukebox_makes_an_allay_dance_and_stopping_it_makes_it_stop() {
    let world = allay_world("allay_jukebox");
    let allay = live_allay(&world);
    assert!(!allay.is_dancing());

    world.game_event(
        &vanilla_game_events::JUKEBOX_PLAY,
        TEST_POS,
        &GameEventContext::new(None, None),
    );
    assert!(allay.is_dancing(), "a jukebox nearby starts the dance");

    world.game_event(
        &vanilla_game_events::JUKEBOX_STOP_PLAY,
        TEST_POS,
        &GameEventContext::new(None, None),
    );
    assert!(!allay.is_dancing(), "stopping the record stops the dance");
}

#[test]
fn an_allay_saves_and_reloads_what_it_gathered_and_what_it_owes() {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    let world = allay_world("allay_save");
    let allay = live_allay(&world);
    allay
        .inventory
        .lock()
        .set_item(0, ItemStack::with_count(&vanilla_items::DIAMOND, 3));
    allay.set_duplication_cooldown(1234);

    let mut nbt = NbtCompound::new();
    allay.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("allay nbt should reborrow: {error}"));

    let reloaded = live_allay(&world);
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.inventory.lock().get_item(0).count(), 3);
    assert_eq!(reloaded.duplication_cooldown(), 1234);
    assert!(!reloaded.can_duplicate());
}

#[test]
fn an_allay_reaches_its_brain_and_survives_forty_ticks_in_a_live_world() {
    // An allay whose `server_ai_step` does not reach `Mob::mob_server_ai_step`
    // never ticks its brain at all, and the tick loop catches a lock-ordering
    // hang in the flying navigation.
    let world = allay_world("allay_ticks");
    let allay = live_allay(&world);

    allay.set_no_action_time(0);
    LivingEntity::server_ai_step(allay.as_ref());
    assert!(
        allay.no_action_time() > 0,
        "the allay's brain never ticks: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        allay.tick();
    }

    assert!(Entity::is_alive(allay.as_ref()));
}
