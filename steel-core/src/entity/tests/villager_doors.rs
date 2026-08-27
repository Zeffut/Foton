//! A villager working the doors of its village.
//!
//! Two things have to be true before any of this happens, and neither is in
//! `InteractWithDoor` itself: the villager's navigation has to be willing to
//! plan a route through a shut door, and `MoveToTargetSink` has to leave the
//! path it computed in the `PATH` memory. Both are on the path these tests
//! take, which is why they walk a villager through a real doorway rather than
//! handing the behavior a path.

use std::sync::Arc;

use glam::DVec3;
use rustc_hash::FxHashSet;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BlockStateProperties, Direction as BlockDirection, DoubleBlockHalf,
};
use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, ChunkPos, GlobalPos};

use super::villagers::{place_bed, set_time_of_day};
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::ai::brain::behavior::close_doors_behind;
use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::{Brain, ScheduleAttribute};
use crate::entity::entities::VillagerEntity;
use crate::entity::mob::Mob;
use crate::entity::{Entity as _, LivingEntity as _, SharedEntity, init_entities, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// The doorway, in a wall that runs along x = 9.
const DOOR: BlockPos = BlockPos::new(9, 64, 8);
/// Where the villager starts, five blocks west of the door.
const START: DVec3 = DVec3::new(4.5, 64.0, 8.5);
/// Where it is sent, six blocks east of the door -- far enough past it that the
/// door stops being either end of the villager's current path step, which is
/// what lets it be shut again.
const TARGET: BlockPos = BlockPos::new(15, 64, 8);

/// The wall runs from here to here in z, with the doorway in the middle.
const WALL_MIN_Z: i32 = 4;
const WALL_MAX_Z: i32 = 12;

/// A room split in two by a wall with one doorway in it.
fn doorway_world(key: &'static str, door: BlockRefKind) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    init_entities();
    let world = fresh_test_world(key);
    for chunk_x in 0..=1 {
        for chunk_z in 0..=1 {
            insert_ready_full_chunk(&world, ChunkPos::new(chunk_x, chunk_z));
        }
    }

    let stone = vanilla_blocks::STONE.default_state();
    for x in 2..=20 {
        for z in WALL_MIN_Z..=WALL_MAX_Z {
            place(&world, BlockPos::new(x, DOOR.y() - 1, z), stone);
        }
    }
    // Two blocks high, so a villager cannot simply hop over it.
    for z in WALL_MIN_Z..=WALL_MAX_Z {
        if z == DOOR.z() {
            continue;
        }
        for y in 0..=1 {
            place(&world, BlockPos::new(DOOR.x(), DOOR.y() + y, z), stone);
        }
    }
    place_door(&world, door);
    world
}

/// Which door goes in the wall.
#[derive(Clone, Copy)]
enum BlockRefKind {
    /// In `#minecraft:mob_interactable_doors`.
    Oak,
    /// Not in it, and not openable by hand either.
    Iron,
}

fn place_door(world: &Arc<World>, kind: BlockRefKind) {
    let block = match kind {
        BlockRefKind::Oak => &vanilla_blocks::OAK_DOOR,
        BlockRefKind::Iron => &vanilla_blocks::IRON_DOOR,
    };
    let lower = block
        .default_state()
        .set_value(
            &BlockStateProperties::DOUBLE_BLOCK_HALF,
            DoubleBlockHalf::Lower,
        )
        .set_value(
            &BlockStateProperties::HORIZONTAL_FACING,
            BlockDirection::East,
        );
    place(world, DOOR, lower);
    place(
        world,
        DOOR.above(),
        lower.set_value(
            &BlockStateProperties::DOUBLE_BLOCK_HALF,
            DoubleBlockHalf::Upper,
        ),
    );
}

fn place(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
    world.set_block(pos, state, UpdateFlags::UPDATE_NONE);
    assert_eq!(
        world.get_block_state(pos).get_block(),
        state.get_block(),
        "the world should have taken the block at {pos:?}"
    );
}

fn is_open(world: &Arc<World>, pos: BlockPos) -> bool {
    world
        .get_block_state(pos)
        .get_value(&BlockStateProperties::OPEN)
}

fn brain(villager: &Arc<VillagerEntity>) -> &Brain {
    Mob::brain(villager.as_ref()).expect("a villager has a brain")
}

fn spawn_villager(world: &Arc<World>) -> Arc<VillagerEntity> {
    spawn_villager_at(world, START)
}

fn spawn_villager_at(world: &Arc<World>, position: DVec3) -> Arc<VillagerEntity> {
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
    villager
}

/// Walks the villager toward `TARGET`, watching for `reached` on every tick.
///
/// The walk target is renewed each tick because `MoveToTargetSink` clears it
/// once it has been reached or given up on, and what is being watched here is
/// the doorway rather than the villager's own reasons for crossing it.
fn walk_toward_target(
    world: &Arc<World>,
    villager: &Arc<VillagerEntity>,
    ticks: i32,
    reached: impl Fn() -> bool,
) -> bool {
    for _ in 0..ticks {
        let now = world.game_time();
        world.level_data.write().set_game_time(now + 1);
        brain(villager).set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_block(TARGET, 0.6, 1),
        );
        villager.base_tick();
        villager.tick();
        if reached() {
            return true;
        }
    }
    false
}

/// The whole point of the behavior: a shut door is not a wall to a villager.
///
/// Everything from the navigation to the block is on this path -- the node
/// evaluator has to treat a shut wooden door as walkable, `MoveToTargetSink`
/// has to leave its path in the `PATH` memory, and `InteractWithDoor` has to
/// read the node ahead out of it.
#[test]
fn a_villager_opens_the_door_it_is_walking_through() {
    let world = doorway_world("villager_door_opens", BlockRefKind::Oak);
    let villager = spawn_villager(&world);
    assert!(!is_open(&world, DOOR), "the door starts shut");

    assert!(
        walk_toward_target(&world, &villager, 400, || is_open(&world, DOOR)),
        "a villager sent through a doorway should have opened the door"
    );
    assert!(
        brain(&villager).has_memory_value(memory_module_types::DOORS_TO_CLOSE.id()),
        "and should be remembering it as one to shut again"
    );
}

/// The tag is the whole of what tells a villager which doors are its business.
/// This is the same call, from the same two blocks away, as the oak door above
/// -- only the door is different, so a pass here and there together say that
/// the tag is what decided it.
///
/// It matters most for a door standing open: an open iron door is walkable, so
/// it really can end up on a villager's path, and a villager that took charge
/// of it would shut somebody's iron door behind itself.
#[test]
fn an_iron_door_is_not_one_a_villager_will_work() {
    let world = doorway_world("villager_door_iron_untouched", BlockRefKind::Iron);
    let villager = spawn_villager_at(&world, DVec3::new(11.5, 64.0, 8.5));
    open_by_hand(&world);
    remember_door(&world, &villager);

    close_doors_behind(&world, villager.as_ref(), brain(&villager), None, None);

    assert!(
        is_open(&world, DOOR),
        "an iron door is left exactly as it was found"
    );
    assert!(
        !brain(&villager).has_memory_value(memory_module_types::DOORS_TO_CLOSE.id()),
        "and dropped rather than kept for another try"
    );
}

/// A shut iron door cannot be opened by hand and is not in the tag either, so
/// it is a wall as far as a villager is concerned -- which is what makes an
/// iron door worth building.
#[test]
fn a_villager_leaves_an_iron_door_alone() {
    let world = doorway_world("villager_door_iron", BlockRefKind::Iron);
    let villager = spawn_villager(&world);

    assert!(
        !walk_toward_target(&world, &villager, 400, || is_open(&world, DOOR)),
        "an iron door should still be shut"
    );
    assert!(
        villager.position().x < f64::from(DOOR.x()),
        "and the villager should still be on the near side of it"
    );
}

/// The other half: a door this villager opened is shut again once it is done
/// with it. `close_doors_behind` is asked directly, because whether a walking
/// villager happens to be inside the three-block window on one of the twenty-
/// tick rounds `InteractWithDoor` runs is a matter of how fast it walks -- the
/// rule being pinned here is the one that decides, not the walk.
#[test]
fn a_villager_shuts_a_door_it_left_open_nearby() {
    let world = doorway_world("villager_door_shuts_near", BlockRefKind::Oak);
    let villager = spawn_villager_at(&world, DVec3::new(11.5, 64.0, 8.5));
    open_by_hand(&world);
    remember_door(&world, &villager);

    close_doors_behind(&world, villager.as_ref(), brain(&villager), None, None);

    assert!(!is_open(&world, DOOR), "the door should have been shut");
    assert!(
        !brain(&villager).has_memory_value(memory_module_types::DOORS_TO_CLOSE.id()),
        "and forgotten now that there is nothing left to do about it"
    );
}

/// A door left more than three blocks behind is forgotten rather than shut.
/// That is not a shortcut: vanilla will not have a villager reach back across a
/// room to pull a door, and without this a villager would shut a door somebody
/// else was walking through on the other side of the village.
#[test]
fn a_villager_forgets_a_door_it_left_far_behind() {
    let world = doorway_world("villager_door_forgets_far", BlockRefKind::Oak);
    let villager = spawn_villager_at(&world, DVec3::new(15.5, 64.0, 8.5));
    open_by_hand(&world);
    remember_door(&world, &villager);

    close_doors_behind(&world, villager.as_ref(), brain(&villager), None, None);

    assert!(
        is_open(&world, DOOR),
        "five blocks away is out of reach, so the door stays as it was"
    );
    assert!(
        !brain(&villager).has_memory_value(memory_module_types::DOORS_TO_CLOSE.id()),
        "and is dropped rather than kept for later"
    );
}

/// A door the villager is still standing in is left alone, which is what stops
/// it shutting one on itself.
#[test]
fn a_villager_does_not_shut_the_door_it_is_standing_in() {
    let world = doorway_world("villager_door_standing_in", BlockRefKind::Oak);
    let villager = spawn_villager_at(&world, DVec3::new(9.5, 64.0, 8.5));
    open_by_hand(&world);
    remember_door(&world, &villager);

    close_doors_behind(
        &world,
        villager.as_ref(),
        brain(&villager),
        None,
        Some(DOOR),
    );

    assert!(is_open(&world, DOOR), "the doorway it is in stays open");
    assert!(
        brain(&villager).has_memory_value(memory_module_types::DOORS_TO_CLOSE.id()),
        "and is still remembered, to be shut once it is through"
    );
}

/// Opens a door by hand, the way a player would.
fn open_by_hand(world: &Arc<World>) {
    let state = world.get_block_state(DOOR);
    world.set_block(
        DOOR,
        state.set_value(&BlockStateProperties::OPEN, true),
        UpdateFlags::UPDATE_ALL,
    );
    assert!(is_open(world, DOOR), "the door should have opened");
}

/// Puts the doorway in the villager's `DOORS_TO_CLOSE`, the way
/// `InteractWithDoor` does when it walks through one.
fn remember_door(world: &Arc<World>, villager: &Arc<VillagerEntity>) {
    let mut doors = FxHashSet::default();
    doors.insert(GlobalPos::new(world.key.clone(), DOOR));
    brain(villager).set_memory(memory_module_types::DOORS_TO_CLOSE, doors);
}

/// The door a villager shuts most reliably is the one it shuts on its way to
/// bed. Vanilla hangs the same `DOORS_TO_CLOSE` pass off `SleepInBed.start`,
/// because a villager walking home can outrun the three-block window
/// `InteractWithDoor` works in -- getting into bed is the moment it is
/// certainly finished with the doorway.
///
/// The villager is stood on its own bed so that it has nowhere to walk and no
/// path at all, which is what leaves `SleepInBed` as the only thing that could
/// have shut the door.
#[test]
fn a_villager_going_to_bed_shuts_what_it_left_open() {
    let world = doorway_world("villager_door_bedtime", BlockRefKind::Oak);
    place_bed(&world, BED);
    let villager = spawn_villager_at(&world, DVec3::new(11.5, 64.0, 8.5));
    open_by_hand(&world);
    remember_door(&world, &villager);
    brain(&villager).set_memory(
        memory_module_types::HOME,
        GlobalPos::new(world.key.clone(), BED),
    );

    // 12000 onward is the REST stretch of `Timelines.VILLAGER_SCHEDULE`.
    set_time_of_day(&world, 13_000);
    let mut asleep = false;
    for _ in 0..400 {
        let now = world.game_time();
        world.level_data.write().set_game_time(now + 1);
        villager.base_tick();
        villager.tick();
        assert!(
            !brain(&villager).has_memory_value(memory_module_types::PATH.id()),
            "the villager is already on its bed, so it never walks anywhere"
        );
        if villager.is_sleeping() {
            asleep = true;
            break;
        }
    }

    assert!(asleep, "the villager should have got into its bed");
    assert!(
        !is_open(&world, DOOR),
        "and shut the door it was still tracking as it did"
    );
}

/// Where the bed goes: two blocks past the door, inside the three the door can
/// still be reached from.
const BED: BlockPos = BlockPos::new(11, 64, 8);
