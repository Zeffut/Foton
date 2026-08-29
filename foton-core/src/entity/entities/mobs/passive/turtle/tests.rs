use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};
use foton_utils::ChunkPos;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::next_entity_id;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

fn turtle() -> TurtleEntity {
    init_vanilla_registry();
    TurtleEntity::new(
        &vanilla_entities::TURTLE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn a_turtle_carrying_an_egg_will_not_court_again() {
    let turtle = turtle();

    assert!(turtle.can_fall_in_love());

    turtle.set_has_egg(true);

    assert!(!turtle.can_fall_in_love());
}

#[test]
fn starting_and_stopping_a_dig_moves_the_counter_off_and_back_to_zero() {
    // Vanilla's `setLayingEgg` is the only place `layEggCounter` is reset, so a
    // turtle interrupted mid-dig has to start the two hundred ticks over.
    let turtle = turtle();

    assert_eq!(turtle.state.lock().lay_egg_counter, 0);

    turtle.set_laying_egg(true);
    assert!(turtle.is_laying_egg());
    assert_eq!(turtle.state.lock().lay_egg_counter, 1);

    turtle.state.lock().lay_egg_counter = 150;
    turtle.set_laying_egg(false);

    assert!(!turtle.is_laying_egg());
    assert_eq!(turtle.state.lock().lay_egg_counter, 0);
}

#[test]
fn a_turtle_spawns_only_on_sand_within_four_blocks_of_the_waterline() {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("turtle_spawn_rules");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::STONE.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    assert!(!TurtleEntity::check_turtle_spawn_rules(&world, pos));

    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::SAND.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    assert!(TurtleEntity::check_turtle_spawn_rules(&world, pos));

    let too_high = BlockPos::new(8, world.sea_level + SPAWN_HEIGHT_ABOVE_SEA_LEVEL, 8);
    assert!(!TurtleEntity::check_turtle_spawn_rules(&world, too_high));
}
