use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::{init_vanilla_registry, vanilla_blocks};
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::test_support::{TestLevel, fresh_test_world};

use super::{FireBlock, can_burn};

const POS: BlockPos = BlockPos::new(0, 64, 0);

fn level_with_support(support_state: BlockStateId) -> TestLevel {
    TestLevel::default()
        .with_min_y(0)
        .with_block(POS.below(), support_state)
}

#[test]
fn get_state_selects_soul_fire_on_soul_fire_base_block() {
    init_vanilla_registry();

    let level = level_with_support(vanilla_blocks::SOUL_SAND.default_state());

    assert_eq!(
        FireBlock::get_state(&level, POS).get_block(),
        &vanilla_blocks::SOUL_FIRE
    );
    assert!(FireBlock::selected_fire_can_survive_at(&level, POS));
}

#[test]
fn get_state_selects_regular_fire_otherwise() {
    init_vanilla_registry();

    let level = level_with_support(vanilla_blocks::STONE.default_state());

    assert_eq!(
        FireBlock::get_state(&level, POS).get_block(),
        &vanilla_blocks::FIRE
    );
}

/// Fire standing on a solid floor draws no side faces; fire hanging in the air
/// draws one against each neighbour that can burn. That is the whole difference
/// between a campfire-looking flame and one clinging to the side of a house.
#[test]
fn fire_with_nothing_under_it_leans_on_what_can_burn() {
    init_vanilla_registry();

    let on_stone = level_with_support(vanilla_blocks::STONE.default_state());
    let grounded = FireBlock::placement_state(&on_stone, POS);
    assert!(!grounded.get_value(&BlockStateProperties::NORTH));
    assert!(!grounded.get_value(&BlockStateProperties::UP));

    let hanging = TestLevel::default()
        .with_min_y(0)
        .with_block(POS.north(), vanilla_blocks::OAK_PLANKS.default_state())
        .with_block(POS.above(), vanilla_blocks::OAK_PLANKS.default_state());
    let leaning = FireBlock::placement_state(&hanging, POS);
    assert!(leaning.get_value(&BlockStateProperties::NORTH));
    assert!(leaning.get_value(&BlockStateProperties::UP));
    assert!(!leaning.get_value(&BlockStateProperties::SOUTH));
}

/// `FireBlock.canSurvive` accepts either a sturdy floor or one burnable
/// neighbour, which is why fire spreads up the side of a wall with nothing
/// beneath it.
#[test]
fn fire_survives_on_a_burnable_neighbour_alone() {
    init_vanilla_registry();

    let bare = TestLevel::default().with_min_y(0);
    assert!(!FireBlock::can_survive_at(&bare, POS));

    let beside_planks = TestLevel::default()
        .with_min_y(0)
        .with_block(POS.east(), vanilla_blocks::OAK_PLANKS.default_state());
    assert!(FireBlock::can_survive_at(&beside_planks, POS));
}

/// A waterlogged block is wet, whatever it is made of.
#[test]
fn water_in_the_block_stops_it_catching() {
    init_vanilla_registry();

    let dry = vanilla_blocks::GLOW_LICHEN
        .default_state()
        .set_value(&BlockStateProperties::WATERLOGGED, false);
    let wet = dry.set_value(&BlockStateProperties::WATERLOGGED, true);

    assert!(can_burn(dry));
    assert!(!can_burn(wet));
}

/// `getIgniteOdds(LevelReader, BlockPos)` answers for the space fire would
/// appear in, not for the fuel around it: a filled position never catches.
#[test]
fn only_empty_space_takes_an_ignite_roll() {
    init_vanilla_registry();

    let level = TestLevel::default()
        .with_min_y(0)
        .with_block(POS.east(), vanilla_blocks::OAK_PLANKS.default_state());
    assert_eq!(FireBlock::ignite_odds_at(&level, POS), 5);

    let occupied = level.with_block(POS, vanilla_blocks::STONE.default_state());
    assert_eq!(FireBlock::ignite_odds_at(&occupied, POS), 0);
}

/// The nether-rack rule: rain and old age are both skipped when the dimension's
/// `infiniburn` tag holds the block underneath.
#[test]
fn infiniburn_is_read_from_the_dimension_tag() {
    init_vanilla_registry();

    let world = fresh_test_world("fire_infiniburn");

    assert!(FireBlock::burns_forever_on(
        &world,
        vanilla_blocks::NETHERRACK.default_state()
    ));
    assert!(!FireBlock::burns_forever_on(
        &world,
        vanilla_blocks::STONE.default_state()
    ));
    // Bedrock only burns forever in the end.
    assert!(!FireBlock::burns_forever_on(
        &world,
        vanilla_blocks::BEDROCK.default_state()
    ));
}

/// `getStateWithAge` keeps the age it was handed, but only for real fire --
/// soul fire has no age to keep.
#[test]
fn spreading_carries_its_age_except_onto_soul_sand() {
    init_vanilla_registry();

    let on_stone = level_with_support(vanilla_blocks::STONE.default_state());
    let aged = FireBlock::state_with_age(&on_stone, POS, 7);
    assert_eq!(aged.get_block(), &vanilla_blocks::FIRE);
    assert_eq!(aged.get_value(&BlockStateProperties::AGE_15), 7);

    let on_soul_sand = level_with_support(vanilla_blocks::SOUL_SAND.default_state());
    let soul = FireBlock::state_with_age(&on_soul_sand, POS, 7);
    assert_eq!(soul.get_block(), &vanilla_blocks::SOUL_FIRE);
}

/// The direction map fire draws its faces from has no `DOWN`: the floor is
/// never one of the sides.
#[test]
fn fire_never_draws_a_face_downwards() {
    assert!(super::face_property(Direction::Down).is_none());
    for direction in [
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
    ] {
        assert!(super::face_property(direction).is_some());
    }
}
