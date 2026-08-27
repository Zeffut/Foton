//! A village raising an iron golem.
//!
//! Nothing counts the golems a village has, and no golem is ever unregistered.
//! What keeps a village to one is a three-part agreement: a villager only wants
//! a golem if it has slept in the last day, enough neighbours have to want one
//! at the same moment, and everybody nearby is told once one is standing there.
//! Break any of the three and a village either never gets a protector or fills
//! up with them, so each is asked for separately here.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos, Downcast as _};

use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::ai::brain::behavior::{BrainContext, TimedBehavior as _};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::{Brain, ScheduleAttribute};
use crate::entity::entities::mobs::npc::VILLAGERS_NEEDED_TO_AGREE_WHEN_GOSSIPING;
use crate::entity::entities::mobs::npc::villager_ai::VillagerPanicTrigger;
use crate::entity::entities::{IronGolemEntity, VillagerEntity};
use crate::entity::mob::Mob;
use crate::entity::{Entity as _, SharedEntity, init_entities, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// The middle of the floor the village stands on.
const STAND: BlockPos = BlockPos::new(8, 64, 8);

/// Vanilla's `Villager.golemSpawnConditionsMet` window: a villager that slept
/// this long ago has stopped counting as one of a living village.
const GOLEM_WISH_AFTER_SLEEP: i64 = 24_000;

/// Vanilla's `Villager.gossip` cooldown, which two villagers have to be off
/// before they will talk -- and so before they will agree on a golem.
const GOSSIP_COOLDOWN: i64 = 1_200;

/// Vanilla's `VillagerPanicTrigger.tick` interval.
const GOLEM_CHECK_INTERVAL: i64 = 100;

/// Vanilla's `spawnGolemIfNeeded` searches eight blocks each way from the
/// villager and drops down from six above, so the floor has to reach past that
/// or a search would fail for want of ground rather than for want of agreement.
const FLOOR_REACH: i32 = 12;

/// A flat, loaded world with room for a golem to be put down anywhere the
/// eight-block search might land.
fn golem_world(key: &'static str) -> Arc<World> {
    let world = empty_golem_world(key);
    let stone = vanilla_blocks::STONE.default_state();
    for x in (STAND.x() - FLOOR_REACH)..=(STAND.x() + FLOOR_REACH) {
        for z in (STAND.z() - FLOOR_REACH)..=(STAND.z() + FLOOR_REACH) {
            let pos = BlockPos::new(x, STAND.y() - 1, z);
            world.set_block(pos, stone, UpdateFlags::UPDATE_NONE);
            assert_eq!(
                world.get_block_state(pos).get_block(),
                &vanilla_blocks::STONE,
                "the golem needs ground at {pos:?} to be put on"
            );
        }
    }
    world
}

/// The same world with no floor at all, so `SpawnUtil` can find nowhere to put
/// a golem however much the village wants one.
fn empty_golem_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    init_entities();
    let world = fresh_test_world(key);
    for chunk_x in -1..=1 {
        for chunk_z in -1..=1 {
            insert_ready_full_chunk(&world, ChunkPos::new(chunk_x, chunk_z));
        }
    }
    world
}

fn brain(villager: &Arc<VillagerEntity>) -> &Brain {
    Mob::brain(villager.as_ref()).expect("a villager has a brain")
}

/// Stands `count` villagers in a row, each having slept just now.
///
/// The schedule attribute is what a spawned villager gets from its own
/// constructor; it is set here because these villagers are built directly.
fn villagers_that_have_slept(
    world: &Arc<World>,
    count: usize,
    slept_at: i64,
) -> Vec<Arc<VillagerEntity>> {
    (0..count)
        .map(|offset| {
            let position = DVec3::new(
                f64::from(STAND.x()) + 0.5 + (offset % 3) as f64,
                f64::from(STAND.y()),
                f64::from(STAND.z()) + 0.5 + (offset / 3) as f64,
            );
            let villager = Arc::new(VillagerEntity::new(
                &vanilla_entities::VILLAGER,
                next_entity_id(),
                position,
                Arc::downgrade(world),
            ));
            world
                .try_add_entity(Arc::clone(&villager) as SharedEntity)
                .expect("the test chunk is loaded, so the villager should attach");
            brain(&villager).set_schedule(ScheduleAttribute::VillagerActivity);
            brain(&villager).set_memory(memory_module_types::LAST_SLEPT, slept_at);
            villager
        })
        .collect()
}

/// How many iron golems are standing in the village.
fn golem_count(world: &Arc<World>) -> usize {
    let (x, y, z) = STAND.get_center();
    let box_around = steel_utils::WorldAabb::new(x, y, z, x + 1.0, y + 1.0, z + 1.0).inflate(32.0);
    world
        .get_entities_in_aabb(&box_around)
        .iter()
        .filter(|entity| entity.downcast_ref::<IronGolemEntity>().is_some())
        .count()
}

/// Five villagers who have all slept recently is what vanilla asks of a village
/// standing around gossiping, and it is the ordinary way a village gets its
/// protector.
#[test]
fn five_villagers_who_have_slept_raise_a_golem_between_them() {
    let world = golem_world("villager_golem_five_agree");
    let villagers = villagers_that_have_slept(&world, 5, 0);
    assert_eq!(golem_count(&world), 0, "no golem before anybody asks");
    assert!(villagers[0].wants_to_spawn_golem(0));

    villagers[0].spawn_golem_if_needed(&world, 0, 5);

    assert_eq!(
        golem_count(&world),
        1,
        "five villagers who agree should have raised one golem"
    );
}

/// Four is not five. This is the assertion that keeps the count from being
/// decoration: without it, a rule that let any single villager summon a golem
/// would pass the test above unchanged.
#[test]
fn four_villagers_are_not_enough_to_raise_a_golem() {
    let world = golem_world("villager_golem_four_disagree");
    let villagers = villagers_that_have_slept(&world, 4, 0);

    villagers[0].spawn_golem_if_needed(&world, 0, 5);

    assert_eq!(
        golem_count(&world),
        0,
        "four villagers should not have been enough"
    );
}

/// A village that has not been to bed in a day has stopped being a village as
/// far as the game is concerned, and gets no golem however many stand there.
#[test]
fn a_village_that_has_not_slept_in_a_day_wants_no_golem() {
    let world = golem_world("villager_golem_sleepless");
    let now = GOLEM_WISH_AFTER_SLEEP * 2;
    let villagers = villagers_that_have_slept(&world, 5, now - GOLEM_WISH_AFTER_SLEEP);
    assert!(
        !villagers[0].wants_to_spawn_golem(now),
        "the last sleep is exactly a day old, which is one tick too long"
    );

    villagers[0].spawn_golem_if_needed(&world, now, 5);

    assert_eq!(golem_count(&world), 0);
}

/// The other half of raising one: everybody who was standing there is told, so
/// the same village does not immediately raise a second.
#[test]
fn raising_a_golem_stops_the_village_asking_for_another() {
    let world = golem_world("villager_golem_only_one");
    let villagers = villagers_that_have_slept(&world, 5, 0);
    villagers[0].spawn_golem_if_needed(&world, 0, 5);
    assert_eq!(golem_count(&world), 1);

    for villager in &villagers {
        assert!(
            brain(villager).has_memory_value(memory_module_types::GOLEM_DETECTED_RECENTLY.id()),
            "every villager in range is told a golem was raised"
        );
        assert!(!villager.wants_to_spawn_golem(0));
    }

    villagers[0].spawn_golem_if_needed(&world, 0, 5);
    assert_eq!(golem_count(&world), 1, "and so no second golem is raised");
}

/// A villager that walks up to a golem it had nothing to do with has to notice
/// it too, or a village whose golem was built by a player would keep raising
/// its own. That is `GolemSensor`, and it reads the nearby-mobs memory another
/// sensor fills -- so this only passes if both are on the villager's list and
/// both are being ticked.
#[test]
fn a_villager_that_can_see_a_golem_stops_wanting_one() {
    let world = golem_world("villager_golem_sensor");
    let villagers = villagers_that_have_slept(&world, 1, 0);
    let villager = &villagers[0];
    assert!(villager.wants_to_spawn_golem(0));

    let golem = Arc::new(IronGolemEntity::new(
        &vanilla_entities::IRON_GOLEM,
        next_entity_id(),
        DVec3::new(
            f64::from(STAND.x()) + 2.5,
            f64::from(STAND.y()),
            f64::from(STAND.z()) + 0.5,
        ),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&golem) as SharedEntity)
        .expect("the test chunk is loaded, so the golem should attach");

    // `GolemSensor` rescans every two hundred ticks and its first scan is
    // staggered anywhere inside that, so two full periods is the shortest run
    // that always contains one.
    let mut noticed = false;
    for _ in 0..400 {
        let now = world.game_time();
        world.level_data.write().set_game_time(now + 1);
        villager.base_tick();
        villager.tick();
        if brain(villager).has_memory_value(memory_module_types::GOLEM_DETECTED_RECENTLY.id()) {
            noticed = true;
            break;
        }
    }

    assert!(noticed, "the villager should have seen the golem");
    assert!(
        !villager.wants_to_spawn_golem(world.game_time()),
        "and stopped wanting one of its own"
    );
}

/// A village that agreed on a golem and could not put one anywhere has to keep
/// wanting one. Telling it otherwise is the quiet failure: the whole village
/// would then spend the next thirty seconds satisfied by a golem that was never
/// raised, and on a cramped rooftop it would never get one at all.
#[test]
fn a_village_with_nowhere_to_put_a_golem_keeps_wanting_one() {
    let world = empty_golem_world("villager_golem_no_ground");
    let villagers = villagers_that_have_slept(&world, 5, 0);

    villagers[0].spawn_golem_if_needed(&world, 0, 5);

    assert_eq!(
        golem_count(&world),
        0,
        "there is no ground in this world to stand a golem on"
    );
    for villager in &villagers {
        assert!(
            villager.wants_to_spawn_golem(0),
            "so the village should still be asking"
        );
    }
}

/// The ordinary way a village gets its protector: two of its people find time
/// to talk. `Villager.gossip` is where vanilla hangs the check, and this is the
/// wiring rather than the rule -- the rules above all call
/// `spawn_golem_if_needed` themselves, and would pass unchanged if nothing ever
/// called it.
#[test]
fn two_villagers_finding_time_to_talk_raise_the_village_a_golem() {
    let world = golem_world("villager_golem_gossip");
    let villagers = villagers_that_have_slept(&world, 5, 0);

    // Both villagers have to be off the twelve-hundred-tick gossip cooldown,
    // which they start on at zero.
    villagers[0].gossip_with(&world, &villagers[1], GOSSIP_COOLDOWN);

    assert_eq!(
        golem_count(&world),
        1,
        "a village that gossips and has slept should have raised a golem"
    );
}

/// The other way in, and the urgent one: `VillagerPanicTrigger.tick` asks every
/// hundredth tick while a villager is frightened, and settles for three
/// neighbours agreeing where gossiping wants five.
///
/// The behavior is ticked directly rather than through a staged fright. A
/// frightened village runs, and runs *apart* -- within a hundred ticks its
/// people are twenty blocks from each other and no longer count as each other's
/// neighbours -- so a test that waited for the hundredth tick to come round
/// naturally would pass or fail on where they happened to scatter to. That the
/// trigger is scheduled and does start on a zombie is
/// `a_villager_panics_when_a_zombie_comes_close`, beside the rest of the
/// villager's day; what is left for here is what its tick does.
#[test]
fn three_frightened_villagers_raise_a_golem_that_gossiping_would_not() {
    let world = golem_world("villager_golem_panic");
    let villagers = villagers_that_have_slept(&world, 3, 0);
    let mut trigger = VillagerPanicTrigger;

    villagers[0].spawn_golem_if_needed(&world, 0, VILLAGERS_NEEDED_TO_AGREE_WHEN_GOSSIPING);
    assert_eq!(
        golem_count(&world),
        0,
        "three is short of the five a gossiping village needs"
    );

    let context = |timestamp| {
        BrainContext::new(
            &world,
            villagers[0].as_ref(),
            brain(&villagers[0]),
            timestamp,
        )
    };
    trigger.tick(&context(GOLEM_CHECK_INTERVAL - 1));
    assert_eq!(
        golem_count(&world),
        0,
        "the check only comes round on every hundredth tick"
    );

    trigger.tick(&context(GOLEM_CHECK_INTERVAL));
    assert_eq!(
        golem_count(&world),
        1,
        "and three frightened villagers are enough when it does"
    );
}
