use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::{init_vanilla_registry, vanilla_blocks};
use foton_utils::{BlockPos, Direction};

use super::{BlockPattern, BlockPatternBuilder, has_state};
use crate::test_support::TestLevel;

/// The pumpkin the player puts on last, and the origin every search starts at.
const PUMPKIN: BlockPos = BlockPos::new(8, 64, 8);

fn iron_golem_pattern() -> BlockPattern {
    BlockPatternBuilder::start()
        .aisle(&["~^~", "###", "~#~"])
        .where_char(
            '^',
            has_state(|state| state.get_block() == &vanilla_blocks::CARVED_PUMPKIN),
        )
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::IRON_BLOCK),
        )
        .where_char('~', has_state(|state| state.is_air()))
        .build()
}

fn snow_golem_pattern() -> BlockPattern {
    BlockPatternBuilder::start()
        .aisle(&["^", "#", "#"])
        .where_char(
            '^',
            has_state(|state| state.get_block() == &vanilla_blocks::CARVED_PUMPKIN),
        )
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::SNOW_BLOCK),
        )
        .build()
}

/// Builds the vanilla iron golem shape with its arms running along `arm`.
fn iron_golem_level(arm: Direction) -> TestLevel {
    let iron = vanilla_blocks::IRON_BLOCK.default_state();
    TestLevel::default()
        .with_block(PUMPKIN, vanilla_blocks::CARVED_PUMPKIN.default_state())
        .with_block(PUMPKIN.below(), iron)
        .with_block(PUMPKIN.below().relative(arm), iron)
        .with_block(PUMPKIN.below().relative(arm.opposite()), iron)
        .with_block(PUMPKIN.below_n(2), iron)
}

#[test]
fn a_finished_iron_golem_frame_is_found_with_its_pumpkin_and_feet_in_place() {
    init_vanilla_registry();
    let level = iron_golem_level(Direction::East);

    let found = iron_golem_pattern()
        .find(&level, PUMPKIN)
        .expect("the iron golem frame should match");

    assert_eq!(found.block(1, 0, 0).pos(), PUMPKIN);
    assert_eq!(found.block(1, 2, 0).pos(), PUMPKIN.below_n(2));
    assert_eq!(found.width(), 3);
    assert_eq!(found.height(), 3);
    assert_eq!(found.depth(), 1);
}

#[test]
fn an_iron_golem_frame_is_found_whichever_horizontal_axis_its_arms_run_along() {
    init_vanilla_registry();

    for arm in [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ] {
        let level = iron_golem_level(arm);
        let found = iron_golem_pattern()
            .find(&level, PUMPKIN)
            .unwrap_or_else(|| panic!("arms along {arm:?} should still match"));
        assert_eq!(found.block(1, 0, 0).pos(), PUMPKIN);
    }
}

#[test]
fn an_iron_golem_frame_missing_an_arm_does_not_match() {
    init_vanilla_registry();
    let iron = vanilla_blocks::IRON_BLOCK.default_state();
    let level = TestLevel::default()
        .with_block(PUMPKIN, vanilla_blocks::CARVED_PUMPKIN.default_state())
        .with_block(PUMPKIN.below(), iron)
        .with_block(PUMPKIN.below().east(), iron)
        .with_block(PUMPKIN.below_n(2), iron);

    assert!(iron_golem_pattern().find(&level, PUMPKIN).is_none());
}

#[test]
fn an_iron_golem_frame_with_a_blocked_shoulder_does_not_match() {
    init_vanilla_registry();
    let level = iron_golem_level(Direction::East)
        .with_block(PUMPKIN.east(), vanilla_blocks::STONE.default_state());

    assert!(iron_golem_pattern().find(&level, PUMPKIN).is_none());
}

#[test]
fn a_snow_golem_column_is_found_and_a_two_block_stack_is_not() {
    init_vanilla_registry();
    let snow = vanilla_blocks::SNOW_BLOCK.default_state();
    let full = TestLevel::default()
        .with_block(PUMPKIN, vanilla_blocks::CARVED_PUMPKIN.default_state())
        .with_block(PUMPKIN.below(), snow)
        .with_block(PUMPKIN.below_n(2), snow);
    let short = TestLevel::default()
        .with_block(PUMPKIN, vanilla_blocks::CARVED_PUMPKIN.default_state())
        .with_block(PUMPKIN.below(), snow);

    let found = snow_golem_pattern()
        .find(&full, PUMPKIN)
        .expect("a full snow column should match");
    assert_eq!(found.block(0, 2, 0).pos(), PUMPKIN.below_n(2));
    assert!(snow_golem_pattern().find(&short, PUMPKIN).is_none());
}

#[test]
fn matches_rejects_an_up_direction_parallel_to_forwards() {
    init_vanilla_registry();
    let level = iron_golem_level(Direction::East);

    assert!(
        iron_golem_pattern()
            .matches(&level, PUMPKIN, Direction::North, Direction::South)
            .is_none()
    );
}

#[test]
fn a_space_in_an_aisle_matches_anything() {
    init_vanilla_registry();
    let pattern = BlockPatternBuilder::start()
        .aisle(&[" ", "#"])
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::IRON_BLOCK),
        )
        .build();
    let level = TestLevel::default()
        .with_block(PUMPKIN, vanilla_blocks::STONE.default_state())
        .with_block(PUMPKIN.below(), vanilla_blocks::IRON_BLOCK.default_state());

    assert!(pattern.find(&level, PUMPKIN).is_some());
}

#[test]
#[should_panic(expected = "aisle row widths must all match the first aisle")]
fn an_aisle_with_a_ragged_row_is_rejected() {
    let _ = BlockPatternBuilder::start().aisle(&["##", "#"]);
}

#[test]
#[should_panic(expected = "predicates for some pattern characters are missing")]
fn a_character_without_a_predicate_is_rejected() {
    let _ = BlockPatternBuilder::start().aisle(&["?"]).build();
}
