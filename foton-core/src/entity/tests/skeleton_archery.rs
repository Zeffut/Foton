//! A skeleton's bow: where it comes from, and what its arrows remember.
//!
//! `AbstractSkeleton.populateDefaultEquipmentSlots` is the only place a
//! skeleton's bow comes from, and `finalizeSpawn` is the only thing that calls
//! it. Every spawn path in Foton goes through `Mob::finalize_spawn` -- natural
//! spawning, a spawner, `SpawnUtil.trySpawnMob`, `/summon` and a raid -- so
//! that is the door these tests come in through.

use super::*;
use crate::entity::entities::{
    ArrowEntity, BoggedEntity, ParchedEntity, PigEntity, SkeletonEntity, StrayEntity,
};
use crate::entity::{EntitySpawnReason, LivingEntity, Mob, SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;
use foton_registry::item_stack::ItemStack;
use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
use foton_utils::types::InteractionHand;
use foton_utils::{ChunkPos, WorldAabb};

// The archery tests drive `tick_living_entity` rather than `server_ai_step`
// alone: vanilla's `LivingEntity.tick` runs `updatingUsingItem` before it
// reaches `aiStep`, and that countdown is what the draw is measured with.

/// Ticks the bow goal needs before an arrow leaves.
///
/// Vanilla parity: twenty ticks of line of sight (`seeTime >= 20`), one more to
/// raise the bow, then the twenty-tick draw (`getTicksUsingItem() >= 20`) of
/// `RangedBowAttackGoal.tick`.
const AIM_TICKS: i32 = 48;

/// Ticks after which the bow is up but not yet loosed.
///
/// The goal raises the bow on its first tick, so this is inside the twenty-tick
/// draw and well short of releasing it.
const MID_DRAW_TICKS: i32 = 10;

/// Ticks by which the first arrow must have left.
///
/// One tick to raise the bow plus the twenty-tick draw, with a little slack.
const FIRST_SHOT_TICKS: i32 = 25;

/// Ticks by which a second arrow must *not* have left yet.
///
/// Vanilla parity: `AbstractSkeleton.getAttackInterval` is 40 below Hard, and
/// the next arrow costs another twenty-tick draw on top -- so the second shot
/// lands somewhere past tick 80. Foton used to fire every 21 ticks, which is
/// what the report called much too fast.
const BEFORE_SECOND_SHOT_TICKS: i32 = 60;

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
        LivingEntity::tick_living_entity(archer.as_ref());
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

/// The bow is drawn before it looses, and the draw is the animation.
///
/// Vanilla raises the bow, holds it for twenty ticks
/// (`getTicksUsingItem() >= 20`) and only then fires. Foton fired on a bare
/// countdown instead: roughly three times too fast, and the client was never
/// told the skeleton was pulling, so nothing was drawn. Both halves of the
/// player's report came from that one missing phase.
#[test]
fn a_skeleton_draws_its_bow_before_it_looses() {
    let world = archery_world("skeleton_draws_before_loosing");

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
    assert!(archer.set_target(Some(&quarry_shared)));

    let everywhere = WorldAabb::new(-256.0, -64.0, -256.0, 256.0, 320.0, 256.0);
    let arrows_now = || {
        world
            .get_entities_in_aabb_matching(&everywhere, |entity| {
                entity.downcast_ref::<ArrowEntity>().is_some()
            })
            .len()
    };

    for _ in 0..MID_DRAW_TICKS {
        LivingEntity::tick_living_entity(archer.as_ref());
    }
    assert!(
        LivingEntity::is_using_item(archer.as_ref()),
        "the skeleton should be holding its bow drawn -- this is the flag the          client renders the pull from"
    );
    assert_eq!(
        arrows_now(),
        0,
        "nothing should have been loosed part-way through the draw"
    );

    for _ in MID_DRAW_TICKS..FIRST_SHOT_TICKS {
        LivingEntity::tick_living_entity(archer.as_ref());
    }
    assert_eq!(arrows_now(), 1, "the finished draw should loose one arrow");
    assert!(
        !LivingEntity::is_using_item(archer.as_ref()),
        "the bow should be lowered once the arrow is away"
    );

    for _ in FIRST_SHOT_TICKS..BEFORE_SECOND_SHOT_TICKS {
        LivingEntity::tick_living_entity(archer.as_ref());
    }
    assert_eq!(
        arrows_now(),
        1,
        "a second arrow this soon is the report: vanilla waits the attack          interval and then draws again"
    );
}
