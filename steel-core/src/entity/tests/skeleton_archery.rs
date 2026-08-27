//! A skeleton's bow: where it comes from, and what its arrows remember.
//!
//! `AbstractSkeleton.populateDefaultEquipmentSlots` is the only place a
//! skeleton's bow comes from, and `finalizeSpawn` is the only thing that calls
//! it. Every spawn path in Steel goes through `Mob::finalize_spawn` -- natural
//! spawning, a spawner, `SpawnUtil.trySpawnMob`, `/summon` and a raid -- so
//! that is the door these tests come in through.

use super::*;
use crate::entity::entities::{
    ArrowEntity, BoggedEntity, ParchedEntity, PigEntity, SkeletonEntity, StrayEntity,
};
use crate::entity::{EntitySpawnReason, LivingEntity, Mob, SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;
use steel_registry::item_stack::ItemStack;
use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
use steel_utils::types::InteractionHand;
use steel_utils::{ChunkPos, WorldAabb};

/// Ticks the bow goal needs the target in sight before it looses.
///
/// Vanilla parity: the `seenTime >= 20` of `RangedBowAttackGoal.tick`.
const AIM_TICKS: i32 = 24;

fn archery_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn held(mob: &dyn LivingEntity) -> ItemStack {
    mob.get_item_in_hand(InteractionHand::MainHand)
}

macro_rules! assert_spawns_with_a_bow {
    ($($name:ident: $ty:ty, $entity_type:expr;)*) => {
        $(
            #[test]
            fn $name() {
                let world = archery_world(concat!("skeleton_bow_", stringify!($name)));
                let archer = <$ty>::new(
                    $entity_type,
                    next_entity_id(),
                    DVec3::new(8.5, 64.0, 8.5),
                    Arc::downgrade(&world),
                );

                Mob::finalize_spawn(&archer, &world, EntitySpawnReason::Natural, None);

                assert!(
                    held(&archer).is(&vanilla_items::BOW),
                    "an archer that spawned empty-handed has nothing to shoot with"
                );
            }
        )*
    };
}

assert_spawns_with_a_bow! {
    a_skeleton_spawns_with_a_bow: SkeletonEntity, &vanilla_entities::SKELETON;
    a_stray_spawns_with_a_bow: StrayEntity, &vanilla_entities::STRAY;
    a_parched_spawns_with_a_bow: ParchedEntity, &vanilla_entities::PARCHED;
    a_bogged_spawns_with_a_bow: BoggedEntity, &vanilla_entities::BOGGED;
}

/// The arrow has to remember the bow, or the skeleton's Power and Flame would
/// never reach the target.
#[test]
fn a_skeletons_arrow_carries_the_bow_it_came_off() {
    let world = archery_world("skeleton_arrow_carries_the_bow");

    let archer = Arc::new(SkeletonEntity::new(
        &vanilla_entities::SKELETON,
        next_entity_id(),
        DVec3::new(4.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    Mob::finalize_spawn(archer.as_ref(), &world, EntitySpawnReason::Natural, None);
    world
        .try_add_entity(Arc::clone(&archer) as SharedEntity)
        .expect("the test chunk is loaded");

    let quarry = Arc::new(PigEntity::new(
        &vanilla_entities::PIG,
        next_entity_id(),
        DVec3::new(9.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    let quarry_shared = Arc::clone(&quarry) as SharedEntity;
    world
        .try_add_entity(Arc::clone(&quarry_shared))
        .expect("the test chunk is loaded");
    assert!(
        archer.set_target(Some(&quarry_shared)),
        "the pig should be a valid target"
    );

    for _ in 0..AIM_TICKS {
        LivingEntity::server_ai_step(archer.as_ref());
    }

    let everywhere = WorldAabb::new(-256.0, -64.0, -256.0, 256.0, 320.0, 256.0);
    let arrows = world.get_entities_in_aabb_matching(&everywhere, |entity| {
        entity.downcast_ref::<ArrowEntity>().is_some()
    });
    let arrow = arrows
        .first()
        .expect("the skeleton should have loosed an arrow")
        .as_ref()
        .downcast_ref::<ArrowEntity>()
        .expect("filtered above");

    let weapon = arrow
        .weapon_item()
        .expect("the arrow should carry the bow it was fired from");
    assert!(weapon.is(&vanilla_items::BOW));
}
