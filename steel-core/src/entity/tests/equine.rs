//! The horse family, driven in a real world.
//!
//! Their goals reach for the world, the goal selector, the navigation lock and
//! each other's synchronized data from inside a goal tick. Running the AI for
//! real is what catches a re-entrant lock -- the caravan goal walks a chain of
//! other llamas, and the trap goal spawns four more horses mid-tick.

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::ai::goal::Goal as _;
use crate::entity::entities::mobs::passive::equine::SkeletonTrapGoal;
use crate::entity::entities::{
    DonkeyEntity, HorseEntity, LlamaEntity, LlamaSpitEntity, MuleEntity, SkeletonHorseEntity,
    TraderLlamaEntity, ZombieHorseEntity,
};
use crate::entity::{
    AbstractHorse, EntitySpawnReason, Projectile, SharedEntity, init_entities, next_entity_id,
};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

/// How many ticks each mob runs for.
const TICKS: i32 = 20;

fn equine_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn spawn(world: &Arc<World>, mob: SharedEntity) -> SharedEntity {
    world
        .try_add_entity(Arc::clone(&mob))
        .expect("the test chunk is loaded, so the mob should attach");
    mob
}

fn run_ai(world: &Arc<World>, mob: &SharedEntity) {
    for _ in 0..TICKS {
        mob.base_tick();
        mob.tick();
    }

    assert!(
        mob.is_alive(),
        "{} should still be alive after {TICKS} ticks",
        mob.entity_type().key
    );

    let Some(as_mob) = mob.as_mob() else {
        panic!("every horse is a mob");
    };
    as_mob.finalize_spawn(world, EntitySpawnReason::Natural, None);
}

#[test]
fn a_horse_runs_its_own_ai_in_a_live_world() {
    let world = equine_world("equine_horse");
    let horse = spawn(
        &world,
        Arc::new(HorseEntity::new(
            &vanilla_entities::HORSE,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &horse);
}

#[test]
fn a_donkey_runs_its_own_ai_in_a_live_world() {
    let world = equine_world("equine_donkey");
    let donkey = spawn(
        &world,
        Arc::new(DonkeyEntity::new(
            &vanilla_entities::DONKEY,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &donkey);
}

#[test]
fn a_mule_runs_its_own_ai_in_a_live_world() {
    let world = equine_world("equine_mule");
    let mule = spawn(
        &world,
        Arc::new(MuleEntity::new(
            &vanilla_entities::MULE,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &mule);
}

#[test]
fn a_skeleton_horse_runs_its_own_ai_in_a_live_world() {
    let world = equine_world("equine_skeleton_horse");
    let skeleton_horse = spawn(
        &world,
        Arc::new(SkeletonHorseEntity::new(
            &vanilla_entities::SKELETON_HORSE,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &skeleton_horse);
}

#[test]
fn a_zombie_horse_runs_its_own_ai_in_a_live_world() {
    let world = equine_world("equine_zombie_horse");
    let zombie_horse = spawn(
        &world,
        Arc::new(ZombieHorseEntity::new(
            &vanilla_entities::ZOMBIE_HORSE,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &zombie_horse);
}

#[test]
fn a_llama_runs_its_own_ai_in_a_live_world() {
    let world = equine_world("equine_llama");
    let llama = spawn(
        &world,
        Arc::new(LlamaEntity::new(
            &vanilla_entities::LLAMA,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &llama);
}

#[test]
fn a_trader_llama_runs_its_own_ai_in_a_live_world() {
    let world = equine_world("equine_trader_llama");
    let trader_llama = spawn(
        &world,
        Arc::new(TraderLlamaEntity::new(
            &vanilla_entities::TRADER_LLAMA,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );

    run_ai(&world, &trader_llama);
}

#[test]
fn a_caravan_goal_walks_a_chain_of_llamas_without_deadlocking() {
    // The caravan goal reads the head llama's own caravan state from inside its
    // tick, so a whole line has to be ticked before the locking is proven.
    let world = equine_world("equine_caravan");
    let mut llamas = Vec::new();
    for index in 0..3 {
        let llama: SharedEntity = Arc::new(LlamaEntity::new(
            &vanilla_entities::LLAMA,
            next_entity_id(),
            SPAWN + DVec3::new(f64::from(index) * 1.5, 0.0, 0.0),
            Arc::downgrade(&world),
        ));
        llamas.push(spawn(&world, llama));
    }

    // Chain them by hand so the goal has a real caravan to follow and to stop.
    let head = llamas[0].as_llama().expect("a llama is a llama");
    let middle = llamas[1].as_llama().expect("a llama is a llama");
    let tail = llamas[2].as_llama().expect("a llama is a llama");
    middle.join_caravan(head);
    tail.join_caravan(middle);

    for _ in 0..TICKS {
        for llama in &llamas {
            llama.base_tick();
            llama.tick();
        }
    }

    for llama in &llamas {
        assert!(llama.is_alive(), "a caravan llama should survive its goals");
    }
}

#[test]
fn a_llama_spits_a_projectile_that_flies_and_dies() {
    let world = equine_world("equine_llama_spit");
    let llama = spawn(
        &world,
        Arc::new(LlamaEntity::new(
            &vanilla_entities::LLAMA,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        )),
    );
    let target = spawn(
        &world,
        Arc::new(HorseEntity::new(
            &vanilla_entities::HORSE,
            next_entity_id(),
            SPAWN + DVec3::new(6.0, 0.0, 0.0),
            Arc::downgrade(&world),
        )),
    );

    llama.as_llama().expect("a llama is a llama").spit(&target);

    let spit = world
        .get_entities_in_aabb(&llama.bounding_box().inflate(8.0))
        .into_iter()
        .find(|entity| entity.entity_type() == &vanilla_entities::LLAMA_SPIT)
        .expect("spitting should put a projectile in the world");
    assert_eq!(
        spit.as_projectile().and_then(Projectile::owner_uuid),
        Some(llama.uuid()),
        "the spit should remember who fired it"
    );

    let start = spit.position();
    for _ in 0..TICKS {
        if spit.is_removed() {
            break;
        }
        spit.tick();
    }

    assert!(
        spit.is_removed() || spit.position() != start,
        "a spit should either travel or be consumed by its first hit"
    );
    assert!(
        spit.downcast_ref::<LlamaSpitEntity>().is_some(),
        "the projectile in the world should be the concrete spit"
    );
}

#[test]
fn a_sprung_skeleton_trap_calls_down_its_riders() {
    // The trap goal spawns four horses and four skeletons inside its own tick,
    // which is the heaviest re-entrancy in the family.
    let world = equine_world("equine_skeleton_trap");
    let trap: SharedEntity = Arc::new(SkeletonHorseEntity::new(
        &vanilla_entities::SKELETON_HORSE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    let Some(skeleton_horse) = trap.downcast_ref::<SkeletonHorseEntity>() else {
        panic!("the trap is a skeleton horse");
    };
    skeleton_horse.set_trap(true);
    spawn(&world, Arc::clone(&trap));

    let mut goal = SkeletonTrapGoal::new();
    let Some(pathfinder) = trap.as_pathfinder_mob() else {
        panic!("a skeleton horse paths");
    };
    goal.tick(pathfinder);

    assert!(!skeleton_horse.is_trap(), "the trap should have sprung");
    assert!(skeleton_horse.is_tamed(), "a sprung trap tames its bait");

    let nearby = world.get_entities_in_aabb(&trap.bounding_box().inflate(4.0));
    let horses = nearby
        .iter()
        .filter(|entity| entity.entity_type() == &vanilla_entities::SKELETON_HORSE)
        .count();
    assert_eq!(horses, 4, "vanilla springs the bait plus three more horses");
    assert!(
        nearby
            .iter()
            .any(|entity| entity.entity_type() == &vanilla_entities::SKELETON),
        "the trap should have produced riders"
    );
}
