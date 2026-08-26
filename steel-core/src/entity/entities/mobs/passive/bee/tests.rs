use std::sync::{Arc, Weak};

use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
use steel_utils::ChunkPos;
use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::next_entity_id;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

fn new_bee() -> BeeEntity {
    init_vanilla_registry();
    BeeEntity::new(
        &vanilla_entities::BEE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn a_bee_that_has_stung_will_not_go_home() {
    // Vanilla's `wantsToEnterHive` refuses a bee that has already stung, which is
    // what leaves a dying bee outside instead of sealing it in the hive.
    let bee = new_bee();
    bee.set_has_nectar(true);

    assert!(bee.wants_to_enter_hive());

    bee.set_has_stung(true);

    assert!(!bee.wants_to_enter_hive());
}

#[test]
fn a_pollinating_bee_will_not_go_home_and_a_bee_kept_out_will_not_either() {
    let bee = new_bee();
    bee.set_has_nectar(true);
    assert!(bee.wants_to_enter_hive());

    bee.set_pollinating(true);
    assert!(!bee.wants_to_enter_hive());
    bee.set_pollinating(false);

    bee.set_stay_out_of_hive_countdown(400);
    assert!(!bee.wants_to_enter_hive());
}

#[test]
fn a_bee_only_wants_the_hive_once_it_has_nectar_or_has_given_up_looking() {
    let bee = new_bee();

    assert!(!bee.wants_to_enter_hive());

    // Vanilla's `isTiredOfLookingForNectar` is strictly greater than 3600.
    bee.state.lock().ticks_without_nectar_since_exiting_hive =
        TICKS_WITHOUT_NECTAR_BEFORE_GOING_HOME;
    assert!(!bee.wants_to_enter_hive());

    bee.state.lock().ticks_without_nectar_since_exiting_hive =
        TICKS_WITHOUT_NECTAR_BEFORE_GOING_HOME + 1;
    assert!(bee.wants_to_enter_hive());
}

#[test]
fn taking_on_nectar_restarts_the_hunger_clock() {
    // `setHasNectar(true)` calls `resetTicksWithoutNectarSinceExitingHive`, which
    // is the only thing stopping a bee that just pollinated from immediately
    // counting as tired of looking.
    let bee = new_bee();
    bee.state.lock().ticks_without_nectar_since_exiting_hive = 5000;

    bee.set_has_nectar(true);

    assert_eq!(bee.ticks_without_nectar(), 0);
}

#[test]
fn dropping_off_nectar_clears_the_crop_allowance() {
    let bee = new_bee();
    bee.set_has_nectar(true);
    bee.increment_crops_grown_since_pollination();
    bee.increment_crops_grown_since_pollination();

    bee.drop_off_nectar();

    assert!(!bee.has_nectar());
    assert_eq!(bee.crops_grown_since_pollination(), 0);
}

#[test]
fn the_three_bee_flags_do_not_overwrite_each_other() {
    // All three live in one synced byte, so a careless `set_flag` would clear the
    // other two -- and a bee that lost `HasStung` would never die of its sting.
    let bee = new_bee();

    bee.set_has_nectar(true);
    bee.set_has_stung(true);
    bee.set_rolling(true);

    assert!(bee.has_nectar());
    assert!(bee.has_stung());
    assert!(bee.is_rolling());

    bee.set_rolling(false);

    assert!(bee.has_nectar());
    assert!(bee.has_stung());
    assert!(!bee.is_rolling());
}

#[test]
fn only_the_top_half_of_a_sunflower_attracts_a_bee() {
    init_vanilla_registry();

    let lower = vanilla_blocks::SUNFLOWER.default_state().set_value(
        &BlockStateProperties::DOUBLE_BLOCK_HALF,
        DoubleBlockHalf::Lower,
    );
    let upper = vanilla_blocks::SUNFLOWER.default_state().set_value(
        &BlockStateProperties::DOUBLE_BLOCK_HALF,
        DoubleBlockHalf::Upper,
    );

    assert!(!BeeEntity::attracts_bees(lower));
    assert!(BeeEntity::attracts_bees(upper));
}

#[test]
fn a_plain_flower_attracts_a_bee_and_a_stone_block_does_not() {
    init_vanilla_registry();

    assert!(BeeEntity::attracts_bees(
        vanilla_blocks::DANDELION.default_state()
    ));
    assert!(!BeeEntity::attracts_bees(
        vanilla_blocks::STONE.default_state()
    ));
}

#[test]
fn a_bee_forgets_a_hive_it_has_drifted_too_far_from() {
    // `isTooFarAway` is what stops a bee reaching into an unloaded chunk for its
    // hive; without it `beehive_block_entity` would look up a block entity
    // anywhere in the world.
    let bee = new_bee();
    let near = BlockPos::new(8, 64, 8);
    let far = BlockPos::new(8 + TOO_FAR_DISTANCE, 64, 8);

    assert!(!bee.is_too_far_away(near));
    assert!(bee.is_too_far_away(far));
}

#[test]
fn dropping_a_hive_starts_the_relocation_cooldown_but_clearing_it_does_not() {
    // Vanilla has both: `dropHive` sets the two-hundred-tick cooldown, while the
    // bare `hivePos = null` of `BeeEnterHiveGoal` does not, so a bee turned away
    // by a full hive can look again at once.
    let bee = new_bee();
    bee.set_hive_pos(BlockPos::new(8, 64, 8));

    bee.clear_hive_pos();
    assert!(!bee.has_hive());
    assert_eq!(bee.hive_locate_cooldown(), 0);

    bee.set_hive_pos(BlockPos::new(8, 64, 8));
    bee.drop_hive();

    assert!(!bee.has_hive());
    assert_eq!(
        bee.hive_locate_cooldown(),
        COOLDOWN_BEFORE_LOCATING_NEW_HIVE
    );
}

#[test]
fn a_bee_takes_the_first_hive_that_is_not_blacklisted() {
    let bee = new_bee();
    let first = BlockPos::new(8, 64, 8);
    let second = BlockPos::new(9, 64, 8);

    bee.set_hive_pos(first);
    bee.blacklist_hive(3);
    bee.clear_hive_pos();

    bee.adopt_first_unblacklisted_hive(&[first, second]);

    assert_eq!(bee.hive_pos(), Some(second));
}

#[test]
fn a_bee_wipes_the_blacklist_when_every_nearby_hive_is_on_it() {
    // Otherwise a bee that had been turned away by both hives in range would stay
    // homeless forever.
    let bee = new_bee();
    let first = BlockPos::new(8, 64, 8);
    let second = BlockPos::new(9, 64, 8);

    for hive in [first, second] {
        bee.set_hive_pos(hive);
        bee.blacklist_hive(3);
    }
    bee.clear_hive_pos();

    bee.adopt_first_unblacklisted_hive(&[first, second]);

    assert_eq!(bee.hive_pos(), Some(first));
    assert!(bee.state.lock().blacklisted_hives.is_empty());
}

#[test]
fn the_hive_blacklist_never_holds_more_than_three() {
    let bee = new_bee();
    for x in 0..5 {
        bee.set_hive_pos(BlockPos::new(x, 64, 8));
        bee.blacklist_hive(3);
    }

    let blacklisted = bee.state.lock().blacklisted_hives.clone();

    assert_eq!(blacklisted.len(), 3);
    // The oldest two were dropped, so the newest three survive in order.
    assert_eq!(
        blacklisted,
        vec![
            BlockPos::new(2, 64, 8),
            BlockPos::new(3, 64, 8),
            BlockPos::new(4, 64, 8),
        ]
    );
}

#[test]
fn a_bee_saves_and_reloads_the_two_positions_it_remembers() {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    let bee = new_bee();
    bee.set_hive_pos(BlockPos::new(1, 2, 3));
    bee.set_saved_flower_pos(BlockPos::new(-4, 5, -6));
    bee.set_has_nectar(true);
    bee.set_has_stung(true);
    bee.state.lock().num_crops_grown_since_pollination = 7;

    let mut nbt = NbtCompound::new();
    bee.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("bee nbt should reborrow: {error}"));

    let reloaded = new_bee();
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.hive_pos(), Some(BlockPos::new(1, 2, 3)));
    assert_eq!(reloaded.saved_flower_pos(), Some(BlockPos::new(-4, 5, -6)));
    assert!(reloaded.has_nectar());
    assert!(reloaded.has_stung());
    assert_eq!(reloaded.crops_grown_since_pollination(), 7);
}

#[test]
fn a_bee_ticks_in_a_live_world_without_deadlocking() {
    // The bee reads its own navigation from `tick_path_navigation` and its move
    // control from the pollinate goal. A previous agent deadlocked every
    // pathfinding mob by reading a navigation flag while its lock was held, so
    // this drives a real tick loop rather than asserting on fields.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("bee_ticks");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::STONE.default_state(),
        UpdateFlags::UPDATE_NONE
    ));

    let bee = Arc::new(BeeEntity::new(
        &vanilla_entities::BEE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(bee.clone())
        .unwrap_or_else(|error| panic!("bee should enter the test world: {error:?}"));

    for _ in 0..40 {
        bee.tick();
    }

    assert!(Entity::is_alive(bee.as_ref()));
}
