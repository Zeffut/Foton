//! The five tameable pets, driven in a real world.
//!
//! Their goal sets reach for the world, the goal selector and each other's
//! synchronized data from inside the goal tick. Every mob-local unit test runs
//! with `Weak::new()` and never notices; these run the AI for real, which is
//! what would catch a re-entrant lock or a missing world guard.

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::entities::{CatEntity, FoxEntity, OcelotEntity, ParrotEntity, WolfEntity};
use crate::entity::{EntitySpawnReason, SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

/// How many ticks each pet runs for.
///
/// Long enough for every goal's `canUse` to be polled several times, including
/// the ones the selector only reaches on an odd tick.
const TICKS: i32 = 20;

fn pet_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn spawn(world: &Arc<World>, pet: SharedEntity) -> SharedEntity {
    world
        .try_add_entity(Arc::clone(&pet))
        .expect("the test chunk is loaded, so the pet should attach");
    pet
}

fn run_ai(world: &Arc<World>, pet: &SharedEntity) {
    for _ in 0..TICKS {
        pet.base_tick();
        pet.tick();
    }

    assert!(
        pet.is_alive(),
        "{} should still be alive after {TICKS} ticks",
        pet.entity_type().key
    );

    let Some(mob) = pet.as_mob() else {
        panic!("every pet is a mob");
    };
    mob.finalize_spawn(world, EntitySpawnReason::Natural, None);
    assert_eq!(pet.base().spawn_reason(), Some(EntitySpawnReason::Natural));
}

#[test]
fn a_wolf_runs_its_own_ai_in_a_live_world() {
    let world = pet_world("pets_wolf");
    let wolf = spawn(
        &world,
        Arc::new(WolfEntity::new(
            &vanilla_entities::WOLF,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &wolf);
}

#[test]
fn a_cat_runs_its_own_ai_in_a_live_world() {
    let world = pet_world("pets_cat");
    let cat = spawn(
        &world,
        Arc::new(CatEntity::new(
            &vanilla_entities::CAT,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &cat);
}

#[test]
fn an_ocelot_runs_its_own_ai_in_a_live_world() {
    let world = pet_world("pets_ocelot");
    let ocelot = spawn(
        &world,
        Arc::new(OcelotEntity::new(
            &vanilla_entities::OCELOT,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &ocelot);
}

#[test]
fn a_parrot_runs_its_own_ai_in_a_live_world() {
    let world = pet_world("pets_parrot");
    let parrot = spawn(
        &world,
        Arc::new(ParrotEntity::new(
            &vanilla_entities::PARROT,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &parrot);
}

#[test]
fn a_fox_runs_its_own_ai_in_a_live_world() {
    let world = pet_world("pets_fox");
    let fox = spawn(
        &world,
        Arc::new(FoxEntity::new(
            &vanilla_entities::FOX,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &fox);
}
