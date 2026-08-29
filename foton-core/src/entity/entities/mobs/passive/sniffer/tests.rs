use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_blocks};
use foton_utils::ChunkPos;
use foton_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::SharedEntity;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

fn new_sniffer() -> SnifferEntity {
    init_vanilla_registry();
    SnifferEntity::new(
        &vanilla_entities::SNIFFER,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn the_dig_state_is_the_only_one_that_crouches_the_hitbox() {
    // Vanilla's `getDefaultDimensions` shortens the box by 0.4 while digging so
    // the sniffer's head is visibly in the ground; every other state is normal.
    let sniffer = new_sniffer();
    let standing = sniffer.dimensions_for_pose(EntityPose::Standing).height;

    sniffer.transition_to(SnifferState::Digging);
    let digging = sniffer.dimensions_for_pose(EntityPose::Standing).height;

    assert!(
        (standing - digging - DIGGING_BB_HEIGHT_OFFSET).abs() < f32::EPSILON,
        "digging should be exactly {DIGGING_BB_HEIGHT_OFFSET} shorter, was {standing} vs {digging}"
    );

    sniffer.transition_to(SnifferState::Searching);
    let searching = sniffer.dimensions_for_pose(EntityPose::Standing).height;
    assert!((standing - searching).abs() < f32::EPSILON);
}

#[test]
fn entering_the_dig_state_schedules_the_seed_two_seconds_out() {
    // The seed drops on one exact tick. A sniffer that scheduled it for the tick
    // it started digging would spit the seed out before its head was down.
    let sniffer = new_sniffer();

    sniffer.transition_to(SnifferState::Digging);

    let drop_at = *sniffer.entity_data.lock().drop_seed_at_tick.get();
    assert_eq!(
        drop_at,
        sniffer.tick_count() + DIGGING_DROP_SEED_OFFSET_TICKS
    );
}

#[test]
fn a_sniffer_remembers_the_holes_it_finished_and_forgets_the_oldest() {
    // The explored list is what stops a sniffer digging the same patch forever.
    // Vanilla caps it at twenty, newest first.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("sniffer_explored");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

    let sniffer = Arc::new(SnifferEntity::new(
        &vanilla_entities::SNIFFER,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&sniffer) as SharedEntity)
        .unwrap_or_else(|error| panic!("sniffer should enter the test world: {error:?}"));

    for x in 0..25 {
        sniffer.store_explored_position(BlockPos::new(x, 63, 8));
    }

    let explored = sniffer
        .brain
        .get_memory(memory_module_types::SNIFFER_EXPLORED_POSITIONS)
        .unwrap_or_default();

    assert_eq!(
        explored.len(),
        MAX_EXPLORED_POSITIONS + 1,
        "the trim keeps twenty and then prepends, so twenty-one survive"
    );
    assert_eq!(
        explored[0].pos,
        BlockPos::new(24, 63, 8),
        "the newest hole is first"
    );
    assert!(
        sniffer.has_explored(BlockPos::new(24, 63, 8)),
        "a hole just dug is remembered"
    );
    assert!(
        !sniffer.has_explored(BlockPos::new(0, 63, 8)),
        "the oldest hole has been forgotten"
    );
}

#[test]
fn a_sniffer_that_is_tempted_or_in_love_stops_sniffing() {
    // Every one of these is a real interruption: vanilla will not let a sniffer
    // start its ritual while a player is leading it around with torchflower
    // seeds, and a dig already in progress is abandoned.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("sniffer_can_sniff");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::GRASS_BLOCK.default_state(),
        UpdateFlags::UPDATE_NONE
    ));

    let sniffer = Arc::new(SnifferEntity::new(
        &vanilla_entities::SNIFFER,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&sniffer) as SharedEntity)
        .unwrap_or_else(|error| panic!("sniffer should enter the test world: {error:?}"));
    sniffer.set_on_ground(true);

    assert!(sniffer.can_sniff());

    sniffer
        .brain
        .set_memory(memory_module_types::IS_TEMPTED, true);
    assert!(!sniffer.can_sniff());
    sniffer
        .brain
        .erase_memory(memory_module_types::IS_TEMPTED.id());
    assert!(sniffer.can_sniff());

    sniffer.set_in_love_time(600);
    assert!(!sniffer.can_sniff());
}

#[test]
fn two_sniffers_mid_ritual_will_not_court() {
    // Vanilla's `canMate` only accepts the three calm states, so a dig is never
    // interrupted by breeding.
    let first = new_sniffer();
    let second = new_sniffer();
    first.set_in_love_time(600);
    second.set_in_love_time(600);

    assert!(first.can_mate(&second));

    first.transition_to(SnifferState::Digging);
    assert!(!first.can_mate(&second));

    first.transition_to(SnifferState::FeelingHappy);
    assert!(first.can_mate(&second));

    second.transition_to(SnifferState::Searching);
    assert!(!first.can_mate(&second));
}

#[test]
fn a_sniffer_reaches_its_brain_and_survives_forty_ticks_in_a_live_world() {
    // A sniffer whose `server_ai_step` does not reach `Mob::mob_server_ai_step`
    // never ticks its brain, and the tick loop catches a lock-ordering hang.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("sniffer_ticks");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::GRASS_BLOCK.default_state(),
        UpdateFlags::UPDATE_NONE
    ));

    let sniffer = Arc::new(SnifferEntity::new(
        &vanilla_entities::SNIFFER,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&sniffer) as SharedEntity)
        .unwrap_or_else(|error| panic!("sniffer should enter the test world: {error:?}"));

    sniffer.set_no_action_time(0);
    LivingEntity::server_ai_step(sniffer.as_ref());
    assert!(
        sniffer.no_action_time() > 0,
        "the sniffer's brain never ticks: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        sniffer.tick();
    }

    assert!(Entity::is_alive(sniffer.as_ref()));
}
