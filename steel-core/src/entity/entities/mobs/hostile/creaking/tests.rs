//! Creaking tests, and with them the whole creaking-heart loop.

use std::sync::Arc;

use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::item_stack::ItemStack;
use steel_registry::{
    init_vanilla_registry, vanilla_blocks, vanilla_damage_types, vanilla_entities, vanilla_items,
};
use steel_utils::axis::Axis;
use steel_utils::types::UpdateFlags;
use steel_utils::{ChunkPos, Downcast as _};

use super::*;
use crate::behavior::blocks::CreakingHeartBlock;
use crate::behavior::init_behaviors;
use crate::block_entity::{BlockEntity as _, init_block_entities};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::next_entity_id;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);
/// Where the heart is buried, well clear of the creaking's own block.
const HEART: BlockPos = BlockPos::new(8, 68, 8);

fn creaking_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    // Three chunks north-to-south: the comparator test walks a creaking thirty
    // blocks out from its heart, which is two chunks away.
    for chunk_z in 0..3 {
        insert_ready_full_chunk(&world, ChunkPos::new(0, chunk_z));
    }
    for x in 4..=13 {
        for z in 4..=13 {
            assert!(world.set_block(
                BlockPos::new(x, 63, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

fn spawn_creaking(world: &Arc<World>, position: DVec3) -> Arc<CreakingEntity> {
    let creaking = Arc::new(CreakingEntity::new(
        &vanilla_entities::CREAKING,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&creaking) as SharedEntity)
        .expect("the test chunk is loaded, so the creaking should attach");
    creaking
}

/// Buries a heart in a pale oak trunk and returns it, awake.
fn place_awake_heart(world: &Arc<World>) {
    let log = vanilla_blocks::PALE_OAK_LOG
        .default_state()
        .set_value(&BlockStateProperties::AXIS, Axis::Y);
    for offset in [-1, 1] {
        assert!(world.set_block(
            BlockPos::new(HEART.x(), HEART.y() + offset, HEART.z()),
            log,
            UpdateFlags::UPDATE_NONE,
        ));
    }
    let heart = vanilla_blocks::CREAKING_HEART
        .default_state()
        .set_value(&BlockStateProperties::AXIS, Axis::Y)
        .set_value(CREAKING_HEART_STATE, CreakingHeartState::Awake);
    assert!(world.set_block(HEART, heart, UpdateFlags::UPDATE_ALL));
}

fn with_heart<T>(world: &Arc<World>, visit: impl FnOnce(&CreakingHeartBlockEntity) -> T) -> T {
    let block_entity = world
        .get_block_entity(HEART)
        .expect("the heart should have created its block entity");
    let heart = block_entity
        .downcast_ref::<CreakingHeartBlockEntity>()
        .expect("a creaking heart block entity");
    visit(heart)
}

/// Puts a player in the world at `position` facing `yaw`, and remembers it in
/// the creaking's brain the way the player sensor would.
fn watching_player(
    world: &Arc<World>,
    creaking: &Arc<CreakingEntity>,
    position: DVec3,
    yaw: f32,
) -> Arc<crate::player::Player> {
    let player = TestPlayerBuilder::new(Arc::clone(world), "Watcher", next_entity_id()).build();
    player
        .try_set_position(position)
        .expect("the test chunk is loaded");
    player.set_rotation((yaw, 0.0));
    player.set_y_head_rot(yaw);
    add_player(world, &player);

    let shared: SharedEntity = Arc::clone(&player) as SharedEntity;
    creaking.brain.set_memory(
        memory_module_types::NEAREST_PLAYERS,
        vec![EntityMemory::new(&shared)],
    );
    player
}

/// The whole point of the mob: it moves while you are not looking, and stops
/// dead the moment you are. Getting the gaze test backwards would make a
/// creaking either permanently frozen or unstoppable, and both look plausible
/// in isolation.
#[test]
fn a_creaking_stops_while_a_player_looks_at_it_and_moves_again_when_they_look_away() {
    let world = creaking_world("creaking_freeze");
    let creaking = spawn_creaking(&world, SPAWN);
    // Six blocks to the south, looking north, straight at the creaking.
    let player = watching_player(&world, &creaking, DVec3::new(8.5, 64.0, 14.5), 180.0);

    assert!(
        !creaking.check_can_move(),
        "a creaking under a player's gaze must not move"
    );
    assert!(
        creaking.is_active(),
        "the gaze that froze it also woke it up"
    );

    // Turn the player around: same distance, opposite heading.
    player.set_rotation((0.0, 0.0));
    player.set_y_head_rot(0.0);

    assert!(
        creaking.check_can_move(),
        "a creaking nobody is looking at moves again"
    );
}

/// Vanilla's `PLAYER_NOT_WEARING_DISGUISE_ITEM`: once a creaking is awake, a
/// player wearing a carved pumpkin is not recognised as looking at it. That is
/// the only counterplay the mob has, so it has to actually be wired up.
#[test]
fn a_carved_pumpkin_hides_a_players_gaze_from_a_woken_creaking() {
    let world = creaking_world("creaking_pumpkin");
    let creaking = spawn_creaking(&world, SPAWN);
    let player = watching_player(&world, &creaking, DVec3::new(8.5, 64.0, 14.5), 180.0);

    assert!(!creaking.check_can_move(), "bare-headed, the gaze holds it");
    assert!(creaking.is_active());

    // A player's equipment *is* its inventory, so the helmet has to be written
    // through the same seam `get_item_by_slot` reads.
    player.with_equipment_slot_mut(EquipmentSlot::Head, &mut |slot| {
        *slot = ItemStack::new(&vanilla_items::CARVED_PUMPKIN);
    });

    assert!(
        creaking.check_can_move(),
        "a woken creaking ignores the gaze of a player in a carved pumpkin"
    );
}

/// A heart-bound creaking is invulnerable: the blow lands on the heart, which
/// grows resin out of the tree instead. If the damage went through, the mob
/// would simply die on the first hit and the heart would never matter.
#[test]
fn a_blow_on_a_heart_bound_creaking_lands_on_the_heart_instead() {
    let world = creaking_world("creaking_hurt_transfer");
    place_awake_heart(&world);
    let creaking = spawn_creaking(&world, SPAWN);
    creaking.set_transient(HEART);
    with_heart(&world, |heart| heart.set_creaking_info(creaking.uuid()));

    let player = TestPlayerBuilder::new(Arc::clone(&world), "Chopper", next_entity_id()).build();
    player
        .try_set_position(DVec3::new(9.5, 64.0, 8.5))
        .expect("the test chunk is loaded");
    add_player(&world, &player);

    let health_before = creaking.get_health();
    let source = DamageSource::environment(&vanilla_damage_types::PLAYER_ATTACK)
        .with_causing_entity(player.id())
        .with_direct_entity(player.id());

    assert!(
        creaking.hurt_server(&world, &source, 100.0),
        "the blow registers, even though it does no damage"
    );
    assert!(
        (creaking.get_health() - health_before).abs() < f32::EPSILON,
        "a heart-bound creaking must not lose health"
    );
    assert!(
        Entity::is_alive(creaking.as_ref()),
        "and must not be killed by a hundred points of damage"
    );

    let resin = resin_clumps_near(&world);
    assert!(
        resin > 0,
        "the heart should have grown resin out of the pale oak; found none"
    );
}

/// The tether: breaking the heart, or the night ending, tears the creaking
/// down. Without this the creaking outlives its heart and nothing can be rid
/// of it.
#[test]
fn removing_the_hearts_protector_tears_the_creaking_down() {
    let world = creaking_world("creaking_tear_down");
    place_awake_heart(&world);
    let creaking = spawn_creaking(&world, SPAWN);
    creaking.set_transient(HEART);
    with_heart(&world, |heart| heart.set_creaking_info(creaking.uuid()));

    assert!(Entity::is_alive(creaking.as_ref()));

    with_heart(&world, |heart| heart.remove_protector(None));

    assert!(
        creaking.is_removed(),
        "the creaking should have crumbled with its heart"
    );
    assert!(
        with_heart(&world, |heart| !heart.is_protector(creaking.uuid())),
        "and the heart should have forgotten it"
    );
}

/// The other half of the same loop: a creaking whose heart no longer names it
/// dies on its own next tick, which is what stops a creaking surviving a heart
/// that was broken while its chunk was unloaded.
#[test]
fn a_creaking_dies_when_its_heart_stops_naming_it() {
    let world = creaking_world("creaking_orphaned");
    place_awake_heart(&world);
    let creaking = spawn_creaking(&world, SPAWN);
    creaking.set_transient(HEART);

    // The heart was never told about this creaking, so it is not its protector.
    creaking.tick();

    assert!(
        creaking.get_health() <= 0.0,
        "an orphaned heart-bound creaking should have set its own health to zero"
    );
}

/// The comparator reads the leash: fifteen with the creaking on top of the
/// heart, falling to nothing as it wanders to the edge of its range.
#[test]
fn the_hearts_comparator_falls_off_as_the_creaking_wanders() {
    let world = creaking_world("creaking_comparator");
    place_awake_heart(&world);
    let close = spawn_creaking(&world, DVec3::new(8.5, 68.0, 8.5));
    // Vanilla only drops a creaking for the night ending when it is not
    // persistence-required; this test is about the signal, not the tether.
    close.set_persistence_required();
    with_heart(&world, |heart| heart.set_creaking_info(close.uuid()));

    with_heart(&world, |heart| heart.tick(&world));
    let near_signal = with_heart(&world, CreakingHeartBlockEntity::analog_output_signal);
    assert_eq!(
        near_signal, 15,
        "a creaking standing on its heart is the strongest signal there is"
    );

    close
        .try_set_position(DVec3::new(8.5, 68.0, 38.5))
        .expect("the creaking may stand thirty blocks out");

    with_heart(&world, |heart| heart.tick(&world));
    let far_signal = with_heart(&world, CreakingHeartBlockEntity::analog_output_signal);
    assert!(
        far_signal < near_signal,
        "the signal has to fall off with distance, got {far_signal} at thirty blocks \
         against {near_signal} at zero"
    );
    assert_eq!(
        far_signal, 1,
        "thirty of thirty-two blocks out leaves one level"
    );
}

/// Vanilla's `Creaking` constructor sets `xpReward = 0` where `Monster` would
/// set five. A creaking that dropped experience would be a farm, since the
/// heart respawns one every night.
#[test]
fn a_creaking_is_worth_no_experience() {
    init_vanilla_registry();
    let creaking = CreakingEntity::new(
        &vanilla_entities::CREAKING,
        next_entity_id(),
        SPAWN,
        Weak::<World>::new(),
    );
    assert_eq!(creaking.xp_reward(), 0);
}

/// A frozen creaking must not keep pathing: vanilla gates the navigation, the
/// move control, the look control and the jump control on `canMove`, and a
/// creaking that slid along its last path while frozen would give the whole
/// mechanic away.
#[test]
fn a_frozen_creaking_keeps_none_of_its_momentum() {
    let world = creaking_world("creaking_stop_in_place");
    let creaking = spawn_creaking(&world, SPAWN);
    let player = watching_player(&world, &creaking, DVec3::new(8.5, 64.0, 14.5), 180.0);
    let _ = &player;

    creaking.set_velocity(DVec3::new(0.5, 0.0, 0.5));
    // `aiStep` is where vanilla notices the flip and calls `stopInPlace`.
    let _ = LivingEntity::ai_step(creaking.as_ref());

    assert!(!creaking.can_move(), "the gaze should have frozen it");
    let velocity = creaking.velocity();
    assert!(
        velocity.x.abs() < 1.0e-9 && velocity.z.abs() < 1.0e-9,
        "a frozen creaking should have been stopped where it stood, but it still \
         carries {velocity:?}"
    );
}

/// Puts a player in both places the world keeps them: the player index the
/// sensors read, and the entity manager `DamageSource` resolution goes through.
fn add_player(world: &Arc<World>, player: &Arc<crate::player::Player>) {
    assert!(world.players.insert(Arc::clone(player)));
    world
        .try_add_entity(Arc::clone(player) as SharedEntity)
        .expect("the test chunk is loaded, so the player should attach");
}

fn resin_clumps_near(world: &Arc<World>) -> usize {
    let mut found = 0;
    for x in 4..=13 {
        for y in 64..=72 {
            for z in 4..=13 {
                if world.get_block_state(BlockPos::new(x, y, z)).get_block()
                    == &vanilla_blocks::RESIN_CLUMP
                {
                    found += 1;
                }
            }
        }
    }
    found
}

/// Keeps `CreakingHeartBlock` referenced from the test module: the heart's log
/// requirement is what makes [`place_awake_heart`] a valid setup at all.
#[test]
fn the_test_heart_is_actually_rooted() {
    let world = creaking_world("creaking_test_heart_rooted");
    place_awake_heart(&world);
    assert!(CreakingHeartBlock::has_required_logs(
        world.get_block_state(HEART),
        world.as_ref(),
        HEART,
    ));
}
