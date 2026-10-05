use super::*;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::{
    behavior::init_behaviors, block_entity::init_block_entities, chunk::chunk_holder::ChunkHolder,
};
use foton_registry::potion_brewing::potion_item;
use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_potions};
use foton_utils::ChunkPos;
use std::slice;

fn stand(world: &Arc<World>) -> (Arc<BrewingStandBlockEntity>, Arc<ChunkHolder>) {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let pos = BlockPos::new(2, 64, 3);
    let holder = insert_ready_full_chunk(world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos,
        vanilla_blocks::BREWING_STAND.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    let stand = Arc::new(BrewingStandBlockEntity::new(
        Arc::downgrade(world),
        pos,
        vanilla_blocks::BREWING_STAND.default_state(),
    ));
    let entity: SharedBlockEntity = stand.clone();
    assert!(world.set_block_entity(entity));
    {
        let mut container = stand.container.lock();
        container.items[0] = potion_item(&vanilla_items::POTION, &vanilla_potions::WATER);
        container.items[SLOT_INGREDIENT] = ItemStack::new(&vanilla_items::NETHER_WART);
        container.items[SLOT_FUEL] = ItemStack::new(&vanilla_items::BLAZE_POWDER);
        container.last_bottle_flags = container.bottle_flags();
    }
    (stand, holder)
}

#[test]
fn cancellation_keeps_inputs_but_spends_cycle_fuel_and_allows_inventory_access() {
    let world = fresh_test_world("brew_cancel");
    let (stand, _holder) = stand(&world);
    stand.tick_with_brew_dispatch(
        &world,
        || true,
        |_| panic!("a cycle starts before it completes"),
    );
    for _ in 1..potion_brewing::BREWING_TIME_TICKS {
        stand.tick_with_brew_dispatch(&world, || true, |_| panic!("brew completed too early"));
    }
    let mut called = false;
    stand.tick_with_brew_dispatch(
        &world,
        || true,
        |event| {
            called = true;
            assert_eq!(event.fuel(), potion_brewing::FUEL_USES - 1);
            assert_eq!(event.position(), stand.get_block_pos());
            let mut container = stand
                .container
                .try_lock()
                .expect("event holds no container lock");
            assert_eq!(container.brew_time, 0);
            assert_eq!(
                container.items[0],
                potion_item(&vanilla_items::POTION, &vanilla_potions::WATER)
            );
            assert_eq!(
                event.results()[0],
                potion_item(&vanilla_items::POTION, &vanilla_potions::AWKWARD)
            );
            // Direct inventory edits survive cancellation, as on Paper.
            container.items[SLOT_FUEL] = ItemStack::new(&vanilla_items::BLAZE_POWDER);
            event.set_cancelled(true);
        },
    );
    assert!(called);
    let container = stand.container.lock();
    assert_eq!(container.brew_time, 0);
    assert_eq!(container.fuel, potion_brewing::FUEL_USES - 1);
    assert!(container.items[SLOT_INGREDIENT].is(&vanilla_items::NETHER_WART));
    assert!(container.items[SLOT_FUEL].is(&vanilla_items::BLAZE_POWDER));
    assert_eq!(
        container.items[0],
        potion_item(&vanilla_items::POTION, &vanilla_potions::WATER)
    );
    drop(container);
    stand.tick_with_brew_dispatch(
        &world,
        || true,
        |_| panic!("the next cycle starts without completing"),
    );
    assert_eq!(stand.container.lock().fuel, potion_brewing::FUEL_USES - 2);
    assert_eq!(
        stand.data.brew_time.load(Ordering::Relaxed),
        potion_brewing::BREWING_TIME_TICKS
    );
}

#[test]
fn changed_results_replace_bottles_and_shortened_list_clears_remaining_slots() {
    let world = fresh_test_world("brew_results");
    let (stand, _holder) = stand(&world);
    {
        let mut container = stand.container.lock();
        container.items[1] = container.items[0].clone();
        container.items[2] = container.items[0].clone();
        container.brew_time = 1;
        container.fuel = 7;
        container.last_bottle_flags = container.bottle_flags();
    }
    let result = potion_item(&vanilla_items::LINGERING_POTION, &vanilla_potions::HEALING);
    stand.tick_with_brew_dispatch(
        &world,
        || true,
        |event| {
            event.set_results(vec![result.clone()]);
        },
    );
    let container = stand.container.lock();
    assert_eq!(container.items[0], result);
    assert!(container.items[1].is_empty());
    assert!(container.items[2].is_empty());
    assert!(container.items[SLOT_INGREDIENT].is_empty());
    assert_eq!(container.fuel, 7);
}

#[test]
fn accepted_snapshot_spends_ingredient_and_returns_its_remainder() {
    let world = fresh_test_world("brew_remainder");
    let (stand, _holder) = stand(&world);
    let mut container = stand.container.lock();
    let mut original = ItemStack::new(&vanilla_items::DRAGON_BREATH);
    original.set_count(2);
    container.items[SLOT_INGREDIENT] = original;
    let result = potion_item(&vanilla_items::LINGERING_POTION, &vanilla_potions::HEALING);
    let dropped = container.brew(slice::from_ref(&result));
    assert_eq!(container.items[0], result);
    assert!(container.items[SLOT_INGREDIENT].is(&vanilla_items::DRAGON_BREATH));
    assert_eq!(container.items[SLOT_INGREDIENT].count(), 1);
    assert!(
        dropped
            .expect("remaining dragon breath requires a dropped bottle")
            .is(&vanilla_items::GLASS_BOTTLE)
    );
}
