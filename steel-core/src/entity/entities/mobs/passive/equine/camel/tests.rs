//! Camel tests.

use steel_registry::{
    init_vanilla_registry, vanilla_blocks, vanilla_damage_types, vanilla_entities,
};
use steel_utils::ChunkPos;
use steel_utils::types::UpdateFlags;

use super::super::camel_common::DASH_COOLDOWN_TICKS;
use super::*;
use crate::behavior::init_behaviors;
use crate::entity::{SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

const TEST_POS: BlockPos = BlockPos::new(8, 64, 8);
const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

/// A game time far enough from zero that a sitting camel reads as sitting.
///
/// Vanilla stores the pose as the negated tick the change started on, so at
/// game time zero `-0` is indistinguishable from standing. A fresh test world
/// starts at zero; a real one is past it before the first camel spawns.
const TEST_GAME_TIME: i64 = 1000;

fn camel_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    world.level_data.write().set_game_time(TEST_GAME_TIME);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(TEST_POS));
    assert!(world.set_block(
        TEST_POS.below(),
        vanilla_blocks::SAND.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    world
}

fn live_camel(world: &Arc<World>) -> Arc<CamelEntity> {
    let camel = Arc::new(CamelEntity::new(
        &vanilla_entities::CAMEL,
        next_entity_id(),
        TEST_POSITION,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&camel) as SharedEntity)
        .unwrap_or_else(|error| panic!("camel should enter the test world: {error:?}"));
    camel
}

#[test]
fn a_camel_sits_down_and_stands_up_and_refuses_to_move_in_between() {
    // The sign of the stored pose tick is the pose, and its distance from the
    // game time is how far through the animation the camel is. Both halves
    // matter: `refuseToMove` is what pins a sitting camel in place, and it stays
    // true through the whole stand-up.
    let world = camel_world("camel_pose");
    let camel = live_camel(&world);

    assert!(!camel.is_camel_sitting());
    camel.sit_down();

    assert!(camel.is_camel_sitting());
    assert_eq!(camel.pose(), EntityPose::Sitting);
    assert!(camel.refuse_to_move(), "a sitting camel goes nowhere");

    camel.stand_up();
    assert!(!camel.is_camel_sitting());
    assert_eq!(camel.pose(), EntityPose::Standing);
    assert!(
        camel.refuse_to_move(),
        "a camel still getting up goes nowhere either"
    );
    assert!(camel.is_in_pose_transition());
}

#[test]
fn standing_up_instantly_skips_the_whole_animation() {
    // This is what damage and water do to a sitting camel: no fifty-two ticks
    // of getting up, straight to moving.
    let world = camel_world("camel_instant_stand");
    let camel = live_camel(&world);
    camel.sit_down();
    assert!(camel.refuse_to_move());

    camel.stand_up_instantly();

    assert!(!camel.is_camel_sitting());
    assert!(!camel.is_in_pose_transition());
    assert!(!camel.refuse_to_move());
}

#[test]
fn a_sitting_camel_stands_up_the_moment_it_is_hurt() {
    let world = camel_world("camel_hurt");
    let camel = live_camel(&world);
    camel.sit_down();

    camel.actually_hurt(
        &world,
        &DamageSource::environment(&vanilla_damage_types::GENERIC),
        1.0,
    );

    assert!(!camel.is_camel_sitting());
    assert!(!camel.refuse_to_move());
}

#[test]
fn a_sitting_camel_stands_up_when_it_lands_in_water() {
    let world = camel_world("camel_water");
    let camel = live_camel(&world);
    camel.sit_down();
    assert!(camel.is_camel_sitting());

    for offset in 0..3 {
        assert!(world.set_block(
            BlockPos::new(TEST_POS.x(), TEST_POS.y() + offset, TEST_POS.z()),
            vanilla_blocks::WATER.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
    }
    camel.refresh_fluid_contact();
    assert!(camel.is_in_water());

    camel.tick();

    assert!(!camel.is_camel_sitting());
}

#[test]
fn a_camel_will_not_sit_down_under_a_low_ceiling() {
    // `canCamelChangePose` is what stops a camel sitting somewhere it could not
    // stand back up -- so it also stops it standing up into a ceiling.
    let world = camel_world("camel_ceiling");
    let camel = live_camel(&world);
    camel.sit_down();
    assert!(camel.is_camel_sitting());

    // A slab of stone right on top of the sitting camel: standing back up would
    // put its head inside.
    for y in 1..3 {
        assert!(world.set_block(
            BlockPos::new(TEST_POS.x(), TEST_POS.y() + y, TEST_POS.z()),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
    }

    assert!(
        !camel.can_camel_change_pose(),
        "a camel under a ceiling should know it cannot stand up"
    );
}

#[test]
fn a_camel_dashes_forward_and_then_owes_a_cooldown() {
    // The dash is the whole reason to ride a camel: the impulse is forward
    // along the look vector rather than up, and the cooldown is what stops it
    // being a flight.
    let world = camel_world("camel_dash");
    let camel = live_camel(&world);
    camel.set_on_ground(true);
    camel.set_rotation((0.0, 0.0));

    assert_eq!(camel.dash_cooldown(), 0);
    camel.execute_camel_dash(1.0);

    let velocity = camel.velocity();
    assert!(
        velocity.z > 0.0,
        "a dash throws the camel forward, not just up: {velocity:?}"
    );
    assert!(velocity.y > 0.0, "with a little lift on top");
    assert!(
        velocity.z.abs() > velocity.y.abs(),
        "and mostly forward: {velocity:?}"
    );
    assert_eq!(camel.dash_cooldown(), DASH_COOLDOWN_TICKS);
    assert!(camel.is_dashing());
}

#[test]
fn the_dash_cooldown_runs_down_and_the_dash_ends_on_landing() {
    // The five-tick minimum is what stops the dash flag being cleared on the
    // same tick it went up, while the camel is still on the ground.
    let world = camel_world("camel_dash_cooldown");
    let camel = live_camel(&world);
    camel.set_on_ground(true);
    camel.set_dashing(true);
    assert_eq!(camel.dash_cooldown(), DASH_COOLDOWN_TICKS);

    camel.tick();
    assert!(camel.is_dashing(), "the dash lasts at least five ticks");

    for _ in 0..DASH_COOLDOWN_TICKS {
        camel.tick();
    }

    assert!(!camel.is_dashing());
    assert_eq!(camel.dash_cooldown(), 0);
}

#[test]
fn a_camel_carries_two_riders_and_no_more() {
    let world = camel_world("camel_riders");
    let camel = live_camel(&world);
    let riders: Vec<SharedEntity> = (0..3).map(|_| live_camel(&world) as SharedEntity).collect();

    assert!(camel.can_add_passenger(riders[0].as_ref()));
    assert!(riders[0].start_riding(&(Arc::clone(&camel) as SharedEntity)));
    assert!(camel.can_add_passenger(riders[1].as_ref()));
    assert!(riders[1].start_riding(&(Arc::clone(&camel) as SharedEntity)));

    assert_eq!(camel.passengers().len(), 2);
    assert!(
        !camel.can_add_passenger(riders[2].as_ref()),
        "a camel seats two"
    );
}

#[test]
fn a_camel_is_tame_from_birth_and_never_rears() {
    // Two of the three things that separate a camel from a horse: it needs no
    // breaking in, and it dashes rather than rearing.
    let world = camel_world("camel_tame");
    let camel = live_camel(&world);

    assert!(AbstractHorse::is_tamed(camel.as_ref()));
    assert!(!camel.can_perform_rearing());
}

#[test]
fn a_camel_saves_and_reloads_the_pose_it_was_in() {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    let world = camel_world("camel_save");
    let camel = live_camel(&world);
    camel.sit_down();
    assert!(camel.is_camel_sitting());

    let mut nbt = NbtCompound::new();
    camel.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("camel nbt should reborrow: {error}"));

    let reloaded = live_camel(&world);
    reloaded.load_additional((&borrowed).into());

    assert!(
        reloaded.is_camel_sitting(),
        "a camel that was sitting is still sitting after a restart"
    );
    assert_eq!(reloaded.pose(), EntityPose::Sitting);
}

#[test]
fn a_camel_reaches_its_brain_and_survives_forty_ticks_in_a_live_world() {
    // A camel whose `server_ai_step` does not reach `Mob::mob_server_ai_step`
    // never ticks its brain at all, and the tick loop catches a lock-ordering
    // hang between the move control's stand-up and the navigation.
    let world = camel_world("camel_ticks");
    let camel = live_camel(&world);

    camel.set_no_action_time(0);
    LivingEntity::server_ai_step(camel.as_ref());
    assert!(
        camel.no_action_time() > 0,
        "the camel's brain never ticks: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        camel.tick();
    }

    assert!(Entity::is_alive(camel.as_ref()));
}
