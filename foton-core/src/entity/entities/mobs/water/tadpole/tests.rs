use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_blocks};
use foton_utils::ChunkPos;
use foton_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::SharedEntity;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

fn new_tadpole() -> TadpoleEntity {
    init_vanilla_registry();
    TadpoleEntity::new(
        &vanilla_entities::TADPOLE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn feeding_a_tadpole_moves_its_clock_toward_being_a_frog() {
    // `getSpeedUpSecondsWhenFeeding` is a tenth of the seconds left, and `ageUp`
    // multiplies that back into ticks -- so one feed is worth a tenth of the
    // remaining wait, and a tadpole nearly grown gains almost nothing.
    let tadpole = new_tadpole();
    assert_eq!(tadpole.age(), 0);
    assert_eq!(tadpole.ticks_left_until_adult(), TICKS_TO_BE_FROG);

    tadpole.set_age(1_000);

    assert_eq!(tadpole.age(), 1_000);
    assert_eq!(tadpole.ticks_left_until_adult(), TICKS_TO_BE_FROG - 1_000);
}

#[test]
fn an_age_locked_tadpole_stops_counting_toward_the_frog() {
    // Without the lock a bucketed tadpole would grow up in a player's inventory.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("tadpole_age_lock");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

    let tadpole = Arc::new(TadpoleEntity::new(
        &vanilla_entities::TADPOLE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&tadpole) as SharedEntity)
        .unwrap_or_else(|error| panic!("tadpole should enter the test world: {error:?}"));

    tadpole.set_age_locked(true);
    for _ in 0..20 {
        LivingEntity::ai_step(tadpole.as_ref());
    }
    assert_eq!(tadpole.age(), 0);

    tadpole.set_age_locked(false);
    for _ in 0..20 {
        LivingEntity::ai_step(tadpole.as_ref());
    }
    assert_eq!(tadpole.age(), 20);
}

#[test]
fn a_tadpole_that_runs_out_of_time_becomes_a_frog() {
    // This is the far end of the frogspawn loop. A tadpole that never converted
    // would leave the loop open at the same place the block used to.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("tadpole_grows_up");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::GRASS_BLOCK.default_state(),
        UpdateFlags::UPDATE_NONE
    ));

    let tadpole = Arc::new(TadpoleEntity::new(
        &vanilla_entities::TADPOLE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&tadpole) as SharedEntity)
        .unwrap_or_else(|error| panic!("tadpole should enter the test world: {error:?}"));

    tadpole.set_age(TICKS_TO_BE_FROG);

    assert!(
        tadpole.is_removed(),
        "the tadpole should have been replaced by its frog"
    );
    let frogs = world
        .get_entities_in_aabb_matching(&tadpole.bounding_box().inflate(4.0), |entity| {
            entity.entity_type() == &vanilla_entities::FROG
        });
    assert_eq!(frogs.len(), 1, "growing up leaves exactly one frog behind");
}

#[test]
fn a_tadpole_reaches_its_brain_and_survives_forty_ticks_in_a_live_world() {
    // A tadpole whose `server_ai_step` does not reach `Mob::mob_server_ai_step`
    // never ticks its brain, and the tick loop catches a lock-ordering hang in
    // the water navigation.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("tadpole_ticks");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos,
        vanilla_blocks::WATER.default_state(),
        UpdateFlags::UPDATE_NONE
    ));

    let tadpole = Arc::new(TadpoleEntity::new(
        &vanilla_entities::TADPOLE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&tadpole) as SharedEntity)
        .unwrap_or_else(|error| panic!("tadpole should enter the test world: {error:?}"));

    tadpole.set_no_action_time(0);
    LivingEntity::server_ai_step(tadpole.as_ref());
    assert!(
        tadpole.no_action_time() > 0,
        "the tadpole's brain never ticks: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        tadpole.tick();
    }

    assert!(Entity::is_alive(tadpole.as_ref()));
}
