//! The brain-driven mobs, driven in a real world.
//!
//! The copper golem is the only one so far, and it is the whole point of the
//! brain: it has no goals at all, so if `Brain::tick` does not reach through
//! sensors, activities and behaviors into the world, the golem does nothing.
//! Like `super::pets`, these run the AI for real, which is what would catch a
//! re-entrant lock between the brain, the navigation and the container guards.

use steel_registry::blocks::BlockRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::{vanilla_blocks, vanilla_items};

use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::entities::CopperGolemEntity;
use crate::entity::entities::mobs::neutral::golem::CopperGolemState;
use crate::entity::{Mob, SharedEntity, next_entity_id};
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

/// Where the golem stands, in blocks.
const STAND: BlockPos = BlockPos::new(8, 64, 8);

/// How long one full pickup or drop-off takes, plus the two ticks the golem
/// spends finding its target and closing on it.
///
/// Vanilla parity: `TransportItemsBetweenContainers.TARGET_INTERACTION_TIME`.
const TICKS_PER_CONTAINER_VISIT: i32 = 62;

fn brain_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    // The test chunk is all air, and a golem in a void neither stands nor
    // paths. One floor tile under everything this test touches is enough.
    for x in (STAND.x() - 2)..=(STAND.x() + 2) {
        for z in (STAND.z() - 2)..=(STAND.z() + 2) {
            assert!(world.set_block(
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

fn spawn_golem(world: &Arc<World>) -> Arc<CopperGolemEntity> {
    let golem = Arc::new(CopperGolemEntity::new(
        &vanilla_entities::COPPER_GOLEM,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&golem) as SharedEntity)
        .expect("the test chunk is loaded, so the golem should attach");
    golem
}

fn place_chest(world: &Arc<World>, pos: BlockPos, block: BlockRef) {
    assert!(world.set_block(pos, block.default_state(), UpdateFlags::UPDATE_NONE));
}

fn container_at(world: &Arc<World>, pos: BlockPos) -> ContainerRef {
    ContainerRef::from_block_entity(
        world
            .get_block_entity(pos)
            .expect("the chest should have created its block entity"),
    )
    .expect("a chest should expose a container capability")
}

fn stack_in(world: &Arc<World>, pos: BlockPos, slot: usize) -> ItemStack {
    let container = container_at(world, pos);
    let guard = ContainerLockGuard::lock_all(&[&container]);
    guard
        .get(container.container_id())
        .expect("the container should be locked")
        .get_item(slot)
        .clone()
}

fn run_ticks(golem: &Arc<CopperGolemEntity>, ticks: i32) {
    for _ in 0..ticks {
        golem.base_tick();
        golem.tick();
    }
}

#[test]
fn a_copper_golem_runs_its_brain_in_a_live_world() {
    let world = brain_world("brain_copper_golem_alive");
    let golem = spawn_golem(&world);

    run_ticks(&golem, 20);

    assert!(
        Entity::is_alive(golem.as_ref()),
        "the golem should still be alive after twenty brain ticks"
    );
    let brain = Mob::brain(golem.as_ref()).expect("a copper golem has a brain");
    assert!(
        brain.is_active(Activity::Idle),
        "CopperGolemAi.updateActivity puts the golem in IDLE every tick"
    );
    assert!(
        !brain.is_brain_dead(),
        "the golem's brain has sensors and behaviors"
    );
}

#[test]
fn a_fresh_copper_golem_waits_out_its_spawn_cooldown_before_working() {
    let world = brain_world("brain_copper_golem_cooldown");
    let golem = spawn_golem(&world);
    let brain = Mob::brain(golem.as_ref()).expect("a copper golem has a brain");

    let cooldown = brain
        .get_memory(memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS)
        .expect("vanilla's constructor seeds a 60 to 100 tick cooldown");
    assert!(
        (60..100).contains(&cooldown),
        "the spawn cooldown should be vanilla's nextInt(60, 100), got {cooldown}"
    );

    run_ticks(&golem, 5);

    let remaining = brain
        .get_memory(memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS)
        .expect("the cooldown should still be running down");
    assert!(
        remaining < cooldown,
        "CountDownCooldownTicks should have spent some of the {cooldown} tick cooldown, \
         but it is still at {remaining}"
    );
}

#[test]
fn a_copper_golem_carries_a_stack_from_a_copper_chest_to_a_chest() {
    let world = brain_world("brain_copper_golem_transport");
    let source = BlockPos::new(9, 64, 8);
    let destination = BlockPos::new(7, 64, 8);
    place_chest(&world, source, &vanilla_blocks::COPPER_CHEST);
    place_chest(&world, destination, &vanilla_blocks::CHEST);

    {
        let container = container_at(&world, source);
        let mut guard = ContainerLockGuard::lock_all(&[&container]);
        guard.set_item(
            container.container_id(),
            0,
            ItemStack::with_count(&vanilla_items::STONE, 5),
        );
    }

    let golem = spawn_golem(&world);
    // Vanilla seeds a 60 to 100 tick cooldown at spawn so a freshly built golem
    // does not sprint off; this test is about the transport itself.
    Mob::brain(golem.as_ref())
        .expect("a copper golem has a brain")
        .erase_memory(memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS.id());

    run_ticks(&golem, TICKS_PER_CONTAINER_VISIT);

    assert!(
        stack_in(&world, source, 0).is_empty(),
        "the golem should have emptied the copper chest's first slot"
    );
    let mut held = ItemStack::empty();
    golem.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
        held = item_stack.clone();
    });
    assert_eq!(
        held.count(),
        5,
        "the golem should be holding the whole stack it took"
    );
    assert!(held.is(&vanilla_items::STONE));

    run_ticks(&golem, TICKS_PER_CONTAINER_VISIT);

    let delivered = stack_in(&world, destination, 0);
    assert_eq!(
        delivered.count(),
        5,
        "the golem should have put the stack down in the ordinary chest"
    );
    assert!(delivered.is(&vanilla_items::STONE));
    golem.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
        held = item_stack.clone();
    });
    assert!(held.is_empty(), "its hands should be empty again");
}

#[test]
fn a_copper_golem_opens_the_chest_it_is_reaching_into_and_closes_it_after() {
    let world = brain_world("brain_copper_golem_opens_chest");
    let source = BlockPos::new(9, 64, 8);
    place_chest(&world, source, &vanilla_blocks::COPPER_CHEST);
    {
        let container = container_at(&world, source);
        let mut guard = ContainerLockGuard::lock_all(&[&container]);
        guard.set_item(
            container.container_id(),
            0,
            ItemStack::new(&vanilla_items::STONE),
        );
    }

    let golem = spawn_golem(&world);
    Mob::brain(golem.as_ref())
        .expect("a copper golem has a brain")
        .erase_memory(memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS.id());

    let chest = world
        .get_block_entity(source)
        .expect("the chest should have created its block entity");

    // Two ticks to find the chest and reach it, and the interaction opens the
    // lid on the first tick the golem spends standing there.
    run_ticks(&golem, 3);
    assert_eq!(
        chest.base().opener_count(),
        1,
        "the golem should have opened the chest it is reaching into"
    );
    assert_eq!(golem.opened_chest_pos(), Some(source));
    assert_eq!(
        golem.state(),
        CopperGolemState::GettingItem,
        "reaching into a chest that has something in it is GETTING_ITEM"
    );

    run_ticks(&golem, TICKS_PER_CONTAINER_VISIT);

    assert_eq!(
        chest.base().opener_count(),
        0,
        "the golem should have closed the chest behind it"
    );
    assert_eq!(golem.opened_chest_pos(), None);
}
