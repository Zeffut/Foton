//! A village under attack, driven in a real world.
//!
//! These are the four things a raid is supposed to do to the people living in
//! the village, and every one of them is inert unless a whole chain holds: the
//! raid manager has to answer at the villager's own block, `SetRaidStatus` has
//! to be in the core package, the `PRE_RAID` / RAID / HIDE packages have to be
//! registered on the brain at all, and the bell has to reach a villager's
//! memory through its block entity.

use std::sync::Arc;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, vanilla_blocks, vanilla_poi_types,
};
use steel_utils::types::{Difficulty, UpdateFlags};
use steel_utils::{BlockPos, ChunkPos, SectionPos};

use super::villagers::{
    STAND, active_activity, advance_time, place_bed, run_ticks, run_ticks_until, set_time_of_day,
    spawn_villager, villager_world,
};
use crate::behavior::blocks::BellBlock;
use crate::entity::Entity as _;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::{Activity, Brain};
use crate::entity::entities::VillagerEntity;
use crate::entity::init_entities;
use crate::entity::mob::Mob;
use crate::raid::Raid;
use crate::test_support::insert_entity_ticking_chunk;
use crate::world::World;

/// Where the bell goes: one block from where the villager is summoned, so
/// `AcquirePoi` can path to it and claim it as the meeting point.
const BELL: BlockPos = BlockPos::new(STAND.x() - 1, STAND.y(), STAND.z());

/// Where the bed goes, on the villager's other side.
const BED: BlockPos = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());

/// A time of day the schedule has an opinion about, so an activity switched by
/// a raid is switched away from something rather than from nothing. 13000 is
/// the REST stretch of `Timelines.VILLAGER_SCHEDULE`.
const REST_HOURS: i64 = 13_000;

/// The WORK stretch of the same timeline. A villager with no workstation cannot
/// start WORK, which is what makes its fallback -- the default activity --
/// observable.
const WORK_HOURS: i64 = 3_000;

/// Long enough for the one-in-twenty `SetRaidStatus` roll several times over,
/// and for the twenty-tick schedule throttle to have let go.
const TICKS_TO_NOTICE_A_RAID: i32 = 200;

fn brain(villager: &Arc<VillagerEntity>) -> &Brain {
    Mob::brain(villager.as_ref()).expect("a villager has a brain")
}

/// A villager that has taken its bed and its bell, standing in the REST hours.
///
/// The two claims matter: without `HOME` the villager has nowhere to hide, and
/// without `MEETING_POINT` there is no bell for `PRE_RAID` to send it to.
fn village_with_one_villager(key: &'static str) -> (Arc<World>, Arc<VillagerEntity>) {
    let world = villager_world(key);
    place_bed(&world, BED);
    assert!(world.set_block(
        BELL,
        vanilla_blocks::BELL.default_state(),
        UpdateFlags::UPDATE_ALL,
    ));

    let villager = spawn_villager(&world);
    assert!(
        run_ticks_until(&world, &villager, 1_200, || {
            let brain = brain(&villager);
            brain.has_memory_value(memory_module_types::HOME.id())
                && brain.has_memory_value(memory_module_types::MEETING_POINT.id())
        }),
        "the villager should have claimed the bed and the bell it is standing between"
    );

    set_time_of_day(&world, REST_HOURS);
    run_ticks(&world, &villager, 60);
    assert_eq!(
        active_activity(&villager),
        Some(Activity::Rest),
        "the villager keeps its ordinary evening until a raid says otherwise"
    );
    (world, villager)
}

/// Files a raid over the village and returns it.
fn start_raid(world: &Arc<World>) -> Arc<Raid> {
    world.raids().insert(Raid::new(2, STAND, Difficulty::Easy))
}

/// The alarm. A raid counting down over the village pulls every villager out of
/// whatever the clock had them doing and into `PRE_RAID` -- which is the activity
/// that runs them to the bell.
#[test]
fn a_raid_counting_down_puts_the_village_on_alert() {
    let (world, villager) = village_with_one_villager("villager_raid_pre_raid");
    let raid = start_raid(&world);
    assert!(
        !raid.has_first_wave_spawned(),
        "the raid is still counting down, so nobody is fighting yet"
    );

    run_ticks(&world, &villager, TICKS_TO_NOTICE_A_RAID);

    assert_eq!(
        active_activity(&villager),
        Some(Activity::PreRaid),
        "a raid over the village outranks the villager's bedtime"
    );
}

/// The raid ending has to hand the day back, and putting IDLE back as the
/// *default* activity is half of that: `SetRaidStatus` made `PRE_RAID` the
/// default, and the default is where the brain lands whenever the scheduled
/// activity's own memory conditions fail.
///
/// The morning is what makes that visible. This villager has no workstation, so
/// the WORK the schedule names cannot start, and the brain falls back to the
/// default -- which is still `PRE_RAID` unless `ResetRaidStatus` has replaced it.
#[test]
fn a_raid_that_stops_gives_the_village_its_day_back() {
    let (world, villager) = village_with_one_villager("villager_raid_reset");
    let raid = start_raid(&world);
    run_ticks(&world, &villager, TICKS_TO_NOTICE_A_RAID);
    assert_eq!(active_activity(&villager), Some(Activity::PreRaid));

    raid.stop();
    set_time_of_day(&world, WORK_HOURS);
    assert!(
        !brain(&villager).has_memory_value(memory_module_types::JOB_SITE.id()),
        "no workstation, so the WORK the clock names cannot start"
    );
    assert!(
        run_ticks_until(&world, &villager, 400, || active_activity(&villager)
            == Some(Activity::Idle)),
        "a stopped raid should leave the villager idling rather than on alert"
    );
}

/// The `PRE_RAID` package is what actually rings the alarm, and a village whose
/// activity switched but whose behaviors never ran would look identical from
/// the outside. This watches for the bell itself: `RingBell` runs in no other
/// package, and the only sign of it here is the `HEARD_BELL_TIME` the rung bell
/// writes back on the villager that pulled it.
#[test]
fn a_village_on_alert_rings_its_bell() {
    let (world, villager) = village_with_one_villager("villager_raid_rings_bell");
    start_raid(&world);
    run_ticks(&world, &villager, TICKS_TO_NOTICE_A_RAID);
    assert_eq!(active_activity(&villager), Some(Activity::PreRaid));
    assert!(
        !brain(&villager).has_memory_value(memory_module_types::HEARD_BELL_TIME.id()),
        "nothing has rung the bell yet"
    );

    // `RingBell` rolls one tick in twenty, and it is a one-shot, so it only gets
    // a roll every other tick. The block event it queues is what carries the
    // ring, so the queue is run alongside the villager.
    let mut rang = false;
    for _ in 0..2_000 {
        advance_time(&world);
        villager.base_tick();
        villager.tick();
        world.run_block_events();
        if brain(&villager).has_memory_value(memory_module_types::HEARD_BELL_TIME.id()) {
            rang = true;
            break;
        }
    }
    assert!(
        rang,
        "a villager on alert beside its bell should have rung it"
    );
}

/// The bell is the other way into hiding, and the only one that works when
/// there is no raid at all -- a bell rung by a player is an alarm the village
/// answers by getting indoors.
///
/// Everything from the block to the brain is on this path: the block event
/// coming back through `BellBlock`, `BellBlockEntity` writing `HEARD_BELL_TIME`
/// on every brain within thirty-two blocks, `ReactToBell` reading it, and
/// `LocateHidingPlace` finding the bed on the villager's other side.
#[test]
fn a_bell_with_no_raid_behind_it_sends_the_village_indoors() {
    let (world, villager) = village_with_one_villager("villager_raid_bell_hide");
    assert!(
        world.get_raid_at(STAND).is_none(),
        "no raid, so the bell is the whole reason to hide"
    );

    assert!(BellBlock::attempt_to_ring(&world, BELL, None, None));
    world.run_block_events();
    assert!(
        brain(&villager).has_memory_value(memory_module_types::HEARD_BELL_TIME.id()),
        "the bell writes the time it rang on every brain within thirty-two blocks"
    );

    assert!(
        run_ticks_until(&world, &villager, 200, || active_activity(&villager)
            == Some(Activity::Hide)),
        "a villager that heard a bell outside a raid should go and hide"
    );
    assert!(
        run_ticks_until(&world, &villager, 200, || brain(&villager)
            .has_memory_value(memory_module_types::HIDING_PLACE.id())),
        "and should have picked somewhere to hide"
    );
}

/// Hiding has to end on its own. `SetHiddenState` is the only thing that gets a
/// villager out of HIDE -- that package has no `UpdateActivityFromSchedule` --
/// so a clock that never ran out would leave the village hiding forever.
#[test]
fn a_village_that_hid_comes_back_out() {
    let (world, villager) = village_with_one_villager("villager_raid_stop_hiding");
    assert!(BellBlock::attempt_to_ring(&world, BELL, None, None));
    world.run_block_events();
    assert!(run_ticks_until(&world, &villager, 200, || active_activity(
        &villager
    ) == Some(
        Activity::Hide
    )));

    // `SetHiddenState` has two clocks of three hundred ticks each -- one from
    // the bell, one counting the time actually spent at the hiding place -- and
    // a villager whose bed is a block away starts both at once, so this watches
    // for whichever lands first rather than trying to tell them apart.
    assert!(
        run_ticks_until(&world, &villager, 600, || active_activity(&villager)
            != Some(Activity::Hide)),
        "the villager should have stopped hiding once its clock ran out"
    );
    let brain = brain(&villager);
    assert!(
        !brain.has_memory_value(memory_module_types::HIDING_PLACE.id()),
        "coming out drops the hiding place"
    );
    assert!(
        !brain.has_memory_value(memory_module_types::HEARD_BELL_TIME.id()),
        "and the bell it heard, so the next one is heard afresh"
    );
}

/// `PRE_RAID` and RAID are not the same alarm: the first is the countdown, and
/// the village only switches to the second once there are raiders on the
/// ground. That distinction is `Raid.hasFirstWaveSpawned`, and nothing smaller
/// than a real wave proves it is read.
#[test]
fn the_first_wave_landing_moves_the_village_from_the_alarm_to_the_raid() {
    let world = villager_world("villager_raid_first_wave");
    // The wave is built through the entity registry, which the villager world
    // has no reason to have initialized.
    init_entities();
    // A wave needs entity-ticking chunks and ground out past the spawn ring,
    // which the ordinary villager world does not have.
    for chunk_x in -4..=4 {
        for chunk_z in -4..=4 {
            // The villager's own chunk is already loaded by `villager_world`,
            // so it is skipped rather than registered twice.
            if (chunk_x, chunk_z) != (0, 0) {
                insert_entity_ticking_chunk(&world, ChunkPos::new(chunk_x, chunk_z));
            }
        }
    }
    // Ground for the wave to stand on, out past where the spawn ring is thrown.
    // `set_block` answers false for a block that was already stone -- the
    // villager world has laid a small platform of its own -- so the floor is
    // read back rather than the write being asserted.
    let stone = vanilla_blocks::STONE.default_state();
    for x in -48..=63 {
        for z in -48..=63 {
            let pos = BlockPos::new(x, STAND.y() - 1, z);
            world.set_block(pos, stone, UpdateFlags::UPDATE_NONE);
            assert_eq!(
                world.get_block_state(pos).get_block(),
                &vanilla_blocks::STONE,
                "the wave needs ground at {pos:?} to spawn on"
            );
        }
    }
    claim_village_bed(&world, BED);
    assert!(
        world.is_village(STAND),
        "a claimed bed makes this a village"
    );

    let villager = spawn_villager(&world);
    set_time_of_day(&world, REST_HOURS);
    let raid = start_raid(&world);

    // The countdown is three hundred ticks; the wave lands shortly after.
    let mut landed = false;
    for _ in 0..600 {
        world.raids().tick(&world);
        run_ticks(&world, &villager, 1);
        if raid.has_first_wave_spawned() {
            landed = true;
            break;
        }
        assert_ne!(
            active_activity(&villager),
            Some(Activity::Raid),
            "before the wave lands the village is on alert, not in the raid"
        );
    }
    assert!(landed, "the countdown should have spawned a wave");
    assert!(raid.total_raiders_alive() > 0);

    run_ticks(&world, &villager, TICKS_TO_NOTICE_A_RAID);
    assert_eq!(
        active_activity(&villager),
        Some(Activity::Raid),
        "with raiders on the ground the village is in the raid proper"
    );
}

/// Claims a bed so the section counts as a village center.
///
/// Vanilla parity: what `AcquirePoi` does for the HOME memory. The first-wave
/// test claims it by hand because the raid has to see a village before the
/// villager has had time to walk over and take the bed itself.
fn claim_village_bed(world: &Arc<World>, pos: BlockPos) {
    place_bed(world, pos);
    let type_id = vanilla_poi_types::HOME.id();
    let tickets = REGISTRY
        .poi_types
        .by_id(type_id)
        .expect("the home POI type must resolve")
        .ticket_count;
    let mut storage = world.poi_storage.lock();
    storage.add(pos, type_id, tickets);
    assert!(storage.reserve_ticket(pos), "the bed should be claimable");
    drop(storage);
    assert_eq!(
        world.sections_to_village(SectionPos::from_block_pos(pos)),
        0
    );
}
