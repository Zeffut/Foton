use std::io::Cursor;
use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};
use simdnbt::borrow::read as read_nbt;
use simdnbt::owned::BaseNbt;

use crate::entity::next_entity_id;

use super::*;

fn pufferfish() -> PufferfishEntity {
    init_vanilla_registry();
    PufferfishEntity::new(
        &vanilla_entities::PUFFERFISH,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn a_pufferfish_grows_through_both_stages_and_then_shrinks_back() {
    // Vanilla's two clocks never run together: the inflate counter drives the
    // way up, and only once it is cleared does the deflate timer count down
    // through the same two stages.
    let fish = pufferfish();
    assert_eq!(fish.puff_state(), STATE_SMALL);

    *fish.inflate_counter.lock() = 1;
    fish.tick_puff_state();
    assert_eq!(fish.puff_state(), STATE_MID);

    *fish.inflate_counter.lock() = MID_TO_FULL_TICKS + 1;
    fish.tick_puff_state();
    assert_eq!(fish.puff_state(), STATE_FULL);

    *fish.inflate_counter.lock() = 0;
    *fish.deflate_timer.lock() = FULL_TO_MID_TICKS + 1;
    fish.tick_puff_state();
    assert_eq!(fish.puff_state(), STATE_MID);

    *fish.deflate_timer.lock() = MID_TO_SMALL_TICKS + 1;
    fish.tick_puff_state();
    assert_eq!(fish.puff_state(), STATE_SMALL);
}

#[test]
fn puffing_up_makes_the_hitbox_grow_with_it() {
    let fish = pufferfish();

    let small = fish.dimensions_for_pose(EntityPose::Standing).width;
    fish.set_puff_state(STATE_MID);
    let mid = fish.dimensions_for_pose(EntityPose::Standing).width;
    fish.set_puff_state(STATE_FULL);
    let full = fish.dimensions_for_pose(EntityPose::Standing).width;

    assert!(small < mid);
    assert!(mid < full);
}

#[test]
fn a_saved_puff_state_is_clamped_to_the_three_vanilla_stages() {
    // Vanilla reads `Math.min(input.getIntOr("PuffState", 0), 2)`, which is the
    // only guard against a hand-edited save turning the fish inside out.
    let fish = pufferfish();
    let mut nbt = NbtCompound::new();
    nbt.insert("PuffState", 7);

    let bytes = {
        let base = BaseNbt::new("", nbt);
        let mut buffer = Vec::new();
        base.write(&mut buffer);
        buffer
    };
    let parsed = read_nbt(&mut Cursor::new(&bytes[..]))
        .expect("nbt should parse")
        .unwrap();
    fish.load_additional(parsed.as_compound());

    assert_eq!(fish.puff_state(), STATE_FULL);
}

#[test]
fn nothing_in_the_not_scary_tag_makes_a_pufferfish_puff_up() {
    init_vanilla_registry();

    // The tag is what keeps a school of pufferfish from inflating at each other.
    assert!(REGISTRY.entity_types.is_in_tag(
        &vanilla_entities::PUFFERFISH,
        &EntityTypeTag::NOT_SCARY_FOR_PUFFERFISH
    ));
    assert!(!REGISTRY.entity_types.is_in_tag(
        &vanilla_entities::PLAYER,
        &EntityTypeTag::NOT_SCARY_FOR_PUFFERFISH
    ));
}
