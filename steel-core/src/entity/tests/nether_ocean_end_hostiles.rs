//! The nether, ocean and end hostiles, driven in a real world.
//!
//! Each of these mobs replaces at least one of Steel's shared controls -- the
//! ghast, guardian, vex and phantom all install their own move control, and the
//! shulker its own look control -- and every one of those reaches for the
//! navigation, the goal selector and the mob's own synchronized data from
//! inside a tick that already holds a lock or two. The mob-local unit tests run
//! with `Weak::new()` and never notice; these run the AI for real, which is
//! what would catch a re-entrant lock.

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::entities::{
    BlazeEntity, ElderGuardianEntity, EndermiteEntity, GhastEntity, GuardianEntity, PhantomEntity,
    ShulkerEntity, VexEntity,
};
use crate::entity::{SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

/// How many ticks each hostile runs for.
///
/// Long enough for every goal's `canUse` to be polled several times, including
/// the ones the selector only reaches on an odd tick.
const TICKS: i32 = 20;

fn hostile_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn run_ai(world: &Arc<World>, mob: SharedEntity) {
    world
        .try_add_entity(Arc::clone(&mob))
        .expect("the test chunk is loaded, so the mob should attach");

    for _ in 0..TICKS {
        mob.base_tick();
        mob.tick();
    }

    assert!(
        mob.is_alive(),
        "{} should still be alive after {TICKS} ticks",
        mob.entity_type().key
    );
}

macro_rules! assert_ai_survives_a_live_world {
    ($($name:ident: $ty:ty, $entity_type:expr, $world_key:literal;)*) => {
        $(
            #[test]
            fn $name() {
                let world = hostile_world($world_key);
                run_ai(
                    &world,
                    Arc::new(<$ty>::new(
                        $entity_type,
                        next_entity_id(),
                        SPAWN,
                        Arc::downgrade(&world),
                    )),
                );
            }
        )*
    };
}

assert_ai_survives_a_live_world! {
    a_blaze_runs_its_own_ai_in_a_live_world:
        BlazeEntity, &vanilla_entities::BLAZE, "hostiles_blaze";
    a_ghast_runs_its_own_ai_in_a_live_world:
        GhastEntity, &vanilla_entities::GHAST, "hostiles_ghast";
    a_guardian_runs_its_own_ai_in_a_live_world:
        GuardianEntity, &vanilla_entities::GUARDIAN, "hostiles_guardian";
    an_elder_guardian_runs_its_own_ai_in_a_live_world:
        ElderGuardianEntity, &vanilla_entities::ELDER_GUARDIAN, "hostiles_elder_guardian";
    an_endermite_runs_its_own_ai_in_a_live_world:
        EndermiteEntity, &vanilla_entities::ENDERMITE, "hostiles_endermite";
    a_vex_runs_its_own_ai_in_a_live_world:
        VexEntity, &vanilla_entities::VEX, "hostiles_vex";
    a_phantom_runs_its_own_ai_in_a_live_world:
        PhantomEntity, &vanilla_entities::PHANTOM, "hostiles_phantom";
    a_shulker_runs_its_own_ai_in_a_live_world:
        ShulkerEntity, &vanilla_entities::SHULKER, "hostiles_shulker";
}
