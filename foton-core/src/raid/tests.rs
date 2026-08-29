//! Tests for the raid manager.
//!
//! The unit half covers the two things a reader gets wrong from the vanilla
//! source: the pre-incremented id and what survives a save. The world half
//! runs a real raid on a real village and watches a wave arrive, because
//! nothing smaller proves the POI index, the countdown, the spawn ring and the
//! boss bar are wired to each other.

use std::sync::Arc;

use foton_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, init_vanilla_registry, vanilla_blocks,
    vanilla_poi_types,
};
use foton_utils::types::{Difficulty, UpdateFlags};
use foton_utils::{BlockPos, ChunkPos, SectionPos};

use super::raid::{Raid, RaidPhase};
use super::raids::Raids;
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::init_entities;
use crate::test_support::{fresh_test_world, insert_entity_ticking_chunk};
use crate::world::World;

const CENTER: BlockPos = BlockPos::new(8, 64, 8);

/// Vanilla pre-increments its counter, so the first raid a world runs is filed
/// under two. The number reaches a save file through every raider's `RaidId`,
/// so a post-increment here would break a world shared with vanilla.
#[test]
fn the_first_raid_a_world_runs_is_filed_under_two() {
    let raids = Raids::new();

    assert_eq!(raids.next_unique_id(), 2);
    assert_eq!(raids.next_unique_id(), 3);
}

/// The saved form is what survives a restart. A field lost to serde would come
/// back as a default -- a raid centered on the world origin, or one that has
/// already spawned every wave -- and the raid would end on the tick after the
/// reload rather than carrying on.
#[test]
fn a_saved_raid_comes_back_with_its_counters_and_its_center() {
    let raids = Raids::new();
    let id = raids.next_unique_id();
    let raid = Raid::new(id, BlockPos::new(-137, 71, 4096), Difficulty::Hard);
    raid.set_raid_omen_level(3);
    raids.insert(raid);

    let encoded = toml::to_string(&raids.to_persistent()).expect("raids should encode");
    let decoded = toml::from_str(&encoded).expect("raids should decode");
    let restored = Raids::from_persistent(decoded);
    let raid = restored
        .get(id)
        .expect("the raid should survive a round trip");

    assert_eq!(raid.center(), BlockPos::new(-137, 71, 4096));
    assert_eq!(raid.num_groups(), 7);
    assert_eq!(raid.raid_omen_level(), 3);
    assert_eq!(raid.phase(), RaidPhase::Ongoing);
    assert!(raid.is_active());
    assert_eq!(
        restored.next_unique_id(),
        id + 1,
        "the counter is persisted, so a reloaded world does not reissue an id"
    );
}

/// A stopped raid has to report itself through the same three flags a raider's
/// goals read, or the survivors of a cancelled raid would go on celebrating a
/// village that never fell.
#[test]
fn stopping_a_raid_clears_the_flags_its_raiders_read() {
    let raid = Raid::new(2, CENTER, Difficulty::Normal);
    assert!(raid.status().active);

    raid.stop();

    let status = raid.status();
    assert!(!status.active);
    assert!(!status.loss);
    assert!(!status.over);
    assert!(raid.is_stopped());
}

/// A raid on nothing is not a raid. Vanilla stops one whose center is not a
/// village before it has spawned anything, which is what keeps `/raid start` in
/// the wilderness from leaving a permanent empty bar on the screen.
#[test]
fn a_raid_with_no_village_under_it_stops_before_its_first_wave() {
    let world = raid_test_world("test_raid_no_village");
    let raid = world
        .raids()
        .insert(Raid::new(2, CENTER, Difficulty::Normal));

    world.raids().tick(&world);
    assert!(raid.is_stopped());

    // Vanilla's iterator has already passed a raid that stopped inside its own
    // tick, so it is dropped on the next one rather than this one.
    world.raids().tick(&world);
    assert!(world.raids().get(2).is_none(), "a stopped raid is dropped");
}

/// The whole machine in one run: an occupied bed makes the section a village,
/// the raid counts its three hundred ticks down, and a wave of illagers is
/// standing in the world when it reaches zero. Everything between -- the POI
/// query, the spawn ring, `finalizeSpawn`, the wave table -- is on this path.
#[test]
fn a_raid_over_a_village_spawns_its_first_wave_when_the_countdown_runs_out() {
    let world = raid_test_world("test_raid_first_wave");
    claim_village_bed(&world, CENTER);
    assert!(world.is_village(CENTER), "a claimed bed is a village");

    let raid = world.raids().insert(Raid::new(2, CENTER, Difficulty::Easy));
    assert_eq!(raid.total_raiders_alive(), 0);

    for _ in 0..super::DEFAULT_PRE_RAID_TICKS + 20 {
        world.raids().tick(&world);
    }

    assert!(
        !raid.is_stopped(),
        "the raid should still be running over its village"
    );
    assert!(
        raid.is_started(),
        "the countdown should have spawned a wave"
    );
    assert_eq!(raid.groups_spawned(), 1);
    assert!(
        raid.total_raiders_alive() > 0,
        "wave one is four pillagers, so somebody should be standing there"
    );

    let raiders = raid.all_raider_ids();
    let first = world
        .get_entity_by_id(raiders[0])
        .expect("a spawned raider should be in the world");
    let raider = first.as_raider().expect("a wave member is a raider");
    assert_eq!(raider.wave(), 1);
    assert!(
        raider
            .current_raid_status()
            .is_some_and(|status| status.active),
        "a spawned raider reads its own raid back"
    );
}

/// A dead raider has to leave its wave, or the raid would wait forever for mobs
/// that are not there and never reach its next wave.
#[test]
fn a_raider_that_dies_leaves_its_wave() {
    let world = raid_test_world("test_raid_death_leaves_wave");
    claim_village_bed(&world, CENTER);
    let raid = world.raids().insert(Raid::new(2, CENTER, Difficulty::Easy));

    for _ in 0..super::DEFAULT_PRE_RAID_TICKS + 20 {
        world.raids().tick(&world);
    }
    let before = raid.total_raiders_alive();
    assert!(before > 0);

    let victim = raid.all_raider_ids()[0];
    raid.remove_from_raid(&world, victim, false);

    assert_eq!(raid.total_raiders_alive(), before - 1);
    let entity = world
        .get_entity_by_id(victim)
        .expect("the mob is still in the world, just out of the raid");
    assert!(
        entity
            .as_raider()
            .expect("still a raider")
            .current_raid_status()
            .is_none()
    );
}

/// Builds a world a raid can actually run on: loaded chunks out past the spawn
/// ring, and solid ground for the wave to stand on.
///
/// The ring is thrown `0.22 * secondsRemaining - 0.24` of thirty-two blocks out,
/// so it starts a hundred blocks away and closes in as the countdown runs down.
/// A world with no floor is not a failure of the raid: `findRandomSpawnPos`
/// rejects every position and the raid gives up after six tries, which is what
/// vanilla does over a void too.
fn raid_test_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    init_entities();
    let world = fresh_test_world(key);
    for chunk_x in -4..=4 {
        for chunk_z in -4..=4 {
            insert_entity_ticking_chunk(&world, ChunkPos::new(chunk_x, chunk_z));
        }
    }

    let stone = vanilla_blocks::STONE.default_state();
    for x in -48..=63 {
        for z in -48..=63 {
            assert!(world.set_block(
                BlockPos::new(x, CENTER.y() - 1, z),
                stone,
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

/// Claims a bed at `pos` so the section counts as a village center.
///
/// Vanilla parity: what a villager's `AcquirePoi` behavior does for the REST
/// memory. A bed holds one ticket, so one sleeper is a whole village as far as
/// `PoiManager.isVillageCenter` is concerned.
fn claim_village_bed(world: &Arc<World>, pos: BlockPos) {
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
