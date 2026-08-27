//! What the morning sun does to an undead mob, and what a hat does about it.
//!
//! `Mob.burnUndead` runs off the mob half of `baseTick` here, so these come in
//! through `Entity::tick` -- the same call the world tick makes.

use super::*;
use crate::entity::entities::ZombieEntity;
use crate::entity::{Entity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::world::World;
use steel_utils::types::UpdateFlags;

/// Ticks to give the sun.
///
/// Vanilla's roll is `random() * 30 < (brightness - 0.4) * 2`, which under an
/// open noon sky is about one tick in twenty-five. Four hundred tries miss
/// roughly once in ten million.
const SUNLIT_TICKS: i32 = 400;

fn open_sky_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    // Something to stand on. A zombie left in an empty chunk falls out of the
    // world part way through, and how many sun rolls it got first is a matter
    // of how loaded the machine is -- which is exactly the sort of test that
    // passes alone and fails in the suite.
    for x in 7..=9 {
        for z in 7..=9 {
            assert!(world.set_block(
                BlockPos::new(x, 63, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

fn zombie_under_the_sun(world: &Arc<World>) -> ZombieEntity {
    ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(world),
    )
}

/// Stands the zombie in the sun until it catches, and reports whether it did.
fn stand_in_the_sun(zombie: &ZombieEntity) -> bool {
    for _ in 0..SUNLIT_TICKS {
        Entity::tick(zombie);
        if zombie.remaining_fire_ticks() > 0 {
            return true;
        }
    }
    false
}

#[test]
fn a_bare_headed_zombie_catches_fire_in_the_open() {
    let world = open_sky_world("daylight_burn_bare_headed");
    let zombie = zombie_under_the_sun(&world);

    assert!(
        stand_in_the_sun(&zombie),
        "an undead mob under an open sky should burn"
    );
}

#[test]
fn a_helmet_keeps_the_sun_off() {
    let world = open_sky_world("daylight_burn_with_a_helmet");
    let zombie = zombie_under_the_sun(&world);
    zombie.set_item_slot(
        EquipmentSlot::Head,
        ItemStack::new(&vanilla_items::IRON_HELMET),
    );

    assert!(
        !stand_in_the_sun(&zombie),
        "a zombie wearing a helmet does not burn"
    );

    let mut helmet_damage = 0;
    zombie.with_equipment_slot(EquipmentSlot::Head, &mut |helmet| {
        helmet_damage = helmet.get_damage_value();
    });
    assert!(
        helmet_damage > 0,
        "the sun should have been spent on the helmet instead"
    );
}

#[test]
fn the_sun_eventually_eats_the_helmet() {
    let world = open_sky_world("daylight_burn_helmet_breaks");
    let zombie = zombie_under_the_sun(&world);
    let mut helmet = ItemStack::new(&vanilla_items::IRON_HELMET);
    let last_point = helmet.get_max_damage() - 1;
    helmet.set_damage_value(last_point);
    zombie.set_item_slot(EquipmentSlot::Head, helmet);

    for _ in 0..SUNLIT_TICKS {
        Entity::tick(&zombie);
        let mut gone = false;
        zombie.with_equipment_slot(EquipmentSlot::Head, &mut |helmet| {
            gone = helmet.is_empty();
        });
        if gone {
            return;
        }
    }

    panic!("a helmet down to its last point should not survive four hundred ticks of sun");
}
