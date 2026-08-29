//! The game the village's children play with each other.
//!
//! Tag is one behavior playing both halves, and which half a child is on is
//! read off everybody else's `INTERACTION_TARGET` -- so the only way to tell
//! chasing from fleeing is to have two children and look at what each of them
//! does about the other.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, init_vanilla_registry, vanilla_blocks,
    vanilla_entities, vanilla_poi_types, vanilla_world_clocks,
};
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, ChunkPos, SectionPos};
use glam::DVec3;

use super::villagers::{active_activity, place_bed};
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::ai::brain::behavior::{BrainContext, PlayTagWithOtherKids, Trigger as _, utils};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::{Activity, Brain};
use crate::entity::entities::VillagerEntity;
use crate::entity::mob::Mob;
use crate::entity::{AgeableMob, Entity as _, SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// The middle of the floor, and the bed that makes this a village.
const STAND: BlockPos = BlockPos::new(8, 64, 8);
const BED: BlockPos = BlockPos::new(6, 64, 8);

/// 3000..6000 is PLAY on the baby track of `Timelines.VILLAGER_SCHEDULE`.
const PLAY_HOURS: i64 = 3_500;

/// Long enough for the one-in-ten roll tag opens with to come up several times.
const TICKS_TO_START_PLAYING: i32 = 400;

/// How many times a round is offered before giving up on that same roll.
const ROUNDS_TO_ROLL: i32 = 400;

/// A village with room to run in: `PlayTagWithOtherKids` only accepts somewhere
/// to flee to that is still inside one.
fn play_world(key: &'static str) -> Arc<World> {
    play_world_with_floor(key, 0)
}

/// The same village, with the floor carried on east for `extra_chunks` more.
///
/// Everywhere inside one chunk of the bed is inside the village, so a test
/// about the village bound needs somewhere outside it that a child can still
/// stand -- which means ground that reaches further than the village does.
fn play_world_with_floor(key: &'static str, extra_chunks: i32) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    for chunk_x in 0..=extra_chunks {
        insert_ready_full_chunk(&world, ChunkPos::new(chunk_x, 0));
    }

    let stone = vanilla_blocks::STONE.default_state();
    for x in 1..=(15 + 16 * extra_chunks) {
        for z in 1..=15 {
            let pos = BlockPos::new(x, STAND.y() - 1, z);
            world.set_block(pos, stone, UpdateFlags::UPDATE_NONE);
            assert_eq!(
                world.get_block_state(pos).get_block(),
                &vanilla_blocks::STONE,
                "children need ground to run on at {pos:?}"
            );
        }
    }

    // A claimed bed is all `PoiManager.isVillageCenter` asks for, and being
    // inside a village is what lets a fleeing child pick anywhere to run to.
    place_bed(&world, BED);
    let type_id = vanilla_poi_types::HOME.id();
    let tickets = REGISTRY
        .poi_types
        .by_id(type_id)
        .expect("the home POI type must resolve")
        .ticket_count;
    {
        let mut storage = world.poi_storage.lock();
        storage.add(BED, type_id, tickets);
        assert!(storage.reserve_ticket(BED), "the bed should be claimable");
    }
    assert_eq!(
        world.sections_to_village(SectionPos::from_block_pos(STAND)),
        0,
        "a claimed bed makes this a village"
    );

    world
        .set_clock_total_ticks(&vanilla_world_clocks::OVERWORLD, PLAY_HOURS)
        .expect("the overworld clock should exist in a test world");
    world
}

fn brain(villager: &Arc<VillagerEntity>) -> &Brain {
    Mob::brain(villager.as_ref()).expect("a villager has a brain")
}

fn spawn_villager(world: &Arc<World>, position: DVec3, baby: bool) -> Arc<VillagerEntity> {
    let villager = Arc::new(VillagerEntity::new(
        &vanilla_entities::VILLAGER,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&villager) as SharedEntity)
        .expect("the test chunk is loaded, so the villager should attach");
    if baby {
        AgeableMob::set_age(villager.as_ref(), -24_000);
        assert!(AgeableMob::is_baby(villager.as_ref()));
    }
    villager
}

/// Ticks `villagers` together, watching for `reached` after each round.
fn run_until(
    world: &Arc<World>,
    villagers: &[&Arc<VillagerEntity>],
    ticks: i32,
    mut reached: impl FnMut() -> bool,
) -> bool {
    for _ in 0..ticks {
        let now = world.game_time();
        world.level_data.write().set_game_time(now + 1);
        for villager in villagers {
            villager.base_tick();
            villager.tick();
        }
        if reached() {
            return true;
        }
    }
    false
}

fn chasing(villager: &Arc<VillagerEntity>) -> Option<i32> {
    brain(villager)
        .get_memory(memory_module_types::INTERACTION_TARGET)
        .map(|target| target.id())
}

/// Two children in a village end up playing. This is the whole path: the
/// babies sensor filling `VISIBLE_VILLAGER_BABIES` off the visible-mobs memory,
/// the schedule putting a baby in PLAY, and the behavior being in that package
/// at all.
///
/// The ten blocks between them are the point. `InteractWith` -- the ordinary
/// play round that walks up to another villager -- reaches eight blocks and
/// sets the same `INTERACTION_TARGET`, so from any closer the two behaviors
/// would be indistinguishable from the outside. The babies sensor reaches the
/// villager's whole follow range, so at ten only tag can start anything.
#[test]
fn two_children_in_a_village_end_up_chasing_each_other() {
    let world = play_world("villager_play_tag");
    let one = spawn_villager(&world, DVec3::new(4.5, 64.0, 8.5), true);
    let other = spawn_villager(&world, DVec3::new(14.5, 64.0, 8.5), true);

    let mut saw_a_playmate = false;
    let playing = run_until(&world, &[&one, &other], TICKS_TO_START_PLAYING, || {
        saw_a_playmate |= brain(&one)
            .has_memory_value(memory_module_types::VISIBLE_VILLAGER_BABIES.id())
            || brain(&other).has_memory_value(memory_module_types::VISIBLE_VILLAGER_BABIES.id());
        chasing(&one) == Some(other.id()) || chasing(&other) == Some(one.id())
    });

    assert_eq!(
        active_activity(&one),
        Some(Activity::Play),
        "the clock should have put the children at play"
    );
    assert!(
        saw_a_playmate,
        "the babies sensor should have found the other child"
    );
    assert!(
        playing,
        "two children left alone in a village should have started a game"
    );
}

/// A grown villager is not another child. Without the age half of the sensor's
/// filter the village's adults would be dragged into the game, and a lone baby
/// would spend its afternoon chasing its parents.
///
/// The memory is what is watched rather than the chase: a child with no
/// playmates falls through to the ordinary play round, which quite correctly
/// walks up to an adult and looks at it -- that is `InteractWith`, and it sets
/// the same `INTERACTION_TARGET` tag does.
#[test]
fn a_child_does_not_count_the_grown_ups_as_playmates() {
    let world = play_world("villager_play_no_adults");
    let child = spawn_villager(&world, DVec3::new(8.5, 64.0, 8.5), true);
    let adult = spawn_villager(&world, DVec3::new(9.5, 64.0, 8.5), false);

    let saw_a_playmate = run_until(&world, &[&child, &adult], TICKS_TO_START_PLAYING, || {
        brain(&child).has_memory_value(memory_module_types::VISIBLE_VILLAGER_BABIES.id())
    });

    assert!(
        !saw_a_playmate,
        "the only other villager is an adult, so there is nobody to play tag with"
    );
}

/// Offers `body` rounds of tag until `settled` says it has done something.
///
/// The behavior is driven directly rather than through the brain, because the
/// play package has a second round in it that also sets `INTERACTION_TARGET`:
/// walking up to another villager and looking at it. That round is gated on
/// there being no other children in sight, and a fleeing child can put twenty
/// blocks between itself and the last one it saw -- so a test that watched the
/// memory through a whole brain tick would sometimes be reading the wrong
/// behavior's work.
///
/// Rounds rather than ticks: tag opens with a one-in-ten roll, and a round it
/// declines costs nothing.
fn play_rounds(
    world: &Arc<World>,
    body: &Arc<VillagerEntity>,
    friends: &[&Arc<VillagerEntity>],
    mut settled: impl FnMut() -> bool,
) -> bool {
    let mut tag = PlayTagWithOtherKids;
    for _ in 0..ROUNDS_TO_ROLL {
        brain(body).set_memory(
            memory_module_types::VISIBLE_VILLAGER_BABIES,
            friends
                .iter()
                .map(|friend| utils::remember(&(Arc::clone(friend) as SharedEntity)))
                .collect::<Vec<_>>(),
        );
        let context = BrainContext::new(world, body.as_ref(), brain(body), world.game_time());
        tag.trigger(&context);
        if settled() {
            return true;
        }
    }
    false
}

/// A child nobody is chasing joins the game.
#[test]
fn a_child_nobody_is_chasing_goes_after_somebody() {
    let world = play_world("villager_play_chase");
    let child = spawn_villager(&world, DVec3::new(8.5, 64.0, 8.5), true);
    let other = spawn_villager(&world, DVec3::new(9.5, 64.0, 8.5), true);

    let chose = play_rounds(&world, &child, &[&other], || chasing(&child).is_some());

    assert!(
        chose,
        "a child with nobody after it should have picked a target"
    );
    assert_eq!(
        chasing(&child),
        Some(other.id()),
        "and the only child in sight is the one it went after"
    );
}

/// A child somebody is already chasing runs instead of chasing back. Without
/// that arm the two would lock onto each other and stand still, which is not a
/// game -- and it is the only thing that tells the two halves of tag apart.
///
/// Same setup as the chase above, one memory different: the other child is
/// already after this one.
#[test]
fn a_child_that_is_being_chased_runs_rather_than_chasing_back() {
    let world = play_world("villager_play_flee");
    let chaser = spawn_villager(&world, DVec3::new(8.5, 64.0, 8.5), true);
    let runner = spawn_villager(&world, DVec3::new(9.5, 64.0, 8.5), true);
    brain(&chaser).set_memory(
        memory_module_types::INTERACTION_TARGET,
        utils::remember(&(Arc::clone(&runner) as SharedEntity)),
    );

    let ran = play_rounds(&world, &runner, &[&chaser], || {
        assert_eq!(
            chasing(&runner),
            None,
            "a child being chased runs; it does not turn round and chase back"
        );
        brain(&runner).has_memory_value(memory_module_types::WALK_TARGET.id())
    });

    assert!(
        ran,
        "and it runs somewhere, rather than standing where it was caught"
    );
    let running_to = brain(&runner)
        .get_memory(memory_module_types::WALK_TARGET)
        .and_then(|target| target.target().current_block_position())
        .expect("the walk target it just set is a position");
    assert!(
        world.is_village(running_to),
        "and somewhere still in the village, rather than out into the dark"
    );
}

/// A game of tag does not empty the village. Vanilla only accepts somewhere to
/// run to that is still inside one, which is what keeps its children from
/// scattering into the dark over an afternoon.
///
/// The child stands one chunk east of the bed -- still the village -- with
/// floor carried on well past it, so about half of the twenty blocks it may
/// run to are outside. Twenty rounds are sampled, because one spot landing
/// inside would say nothing.
#[test]
fn a_fleeing_child_stays_inside_the_village() {
    let world = play_world_with_floor("villager_play_village_bound", 3);
    let runner = spawn_villager(&world, DVec3::new(30.5, 64.0, 8.5), true);
    let chaser = spawn_villager(&world, DVec3::new(29.5, 64.0, 8.5), true);
    brain(&chaser).set_memory(
        memory_module_types::INTERACTION_TARGET,
        utils::remember(&(Arc::clone(&runner) as SharedEntity)),
    );
    assert!(
        world.is_village(BlockPos::new(30, 64, 8)),
        "the child starts inside the village"
    );
    assert!(
        !world.is_village(BlockPos::new(45, 64, 8)),
        "and there is ground outside it to run to, if the rule let it"
    );

    let mut runs = 0;
    for _ in 0..ROUNDS_TO_ROLL {
        play_rounds(&world, &runner, &[&chaser], || {
            brain(&runner).has_memory_value(memory_module_types::WALK_TARGET.id())
        });
        let Some(running_to) = brain(&runner)
            .get_memory(memory_module_types::WALK_TARGET)
            .and_then(|target| target.target().current_block_position())
        else {
            continue;
        };
        assert!(
            world.is_village(running_to),
            "a child ran to {running_to:?}, which is outside its village"
        );
        brain(&runner).erase_memory(memory_module_types::WALK_TARGET.id());
        runs += 1;
        if runs == VILLAGE_BOUND_SAMPLES {
            break;
        }
    }

    assert_eq!(
        runs, VILLAGE_BOUND_SAMPLES,
        "the child should have found somewhere to run to twenty times over"
    );
}

/// How many separate spots the village bound is sampled at. Half the ground in
/// reach is outside, so twenty of them landing inside by luck is a one in a
/// million accident.
const VILLAGE_BOUND_SAMPLES: i32 = 20;
