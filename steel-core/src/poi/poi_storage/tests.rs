//! Query-layer tests for [`PointOfInterestStorage`].
//!
//! Everything here exercises the vanilla `PoiManager` query surface the brain
//! behaviors stand on: nearest-first search, ticket claiming, and the section
//! distance to a village.

use steel_registry::{
    REGISTRY, RegistryEntry, RegistryExt, init_vanilla_registry, vanilla_poi_types,
};
use steel_utils::{BlockPos, SectionPos};

use super::{MAX_VILLAGE_DISTANCE, NO_VILLAGE_DISTANCE, OccupationStatus, PointOfInterestStorage};

/// Resolving a POI type reads `REGISTRY`, and these run inside call arguments,
/// which Rust evaluates before the callee gets to initialize anything. Each one
/// therefore initializes the registry itself rather than relying on some other
/// test in the binary having got there first.
fn home() -> usize {
    init_vanilla_registry();
    vanilla_poi_types::HOME.id()
}

fn meeting() -> usize {
    init_vanilla_registry();
    vanilla_poi_types::MEETING.id()
}

fn lodestone() -> usize {
    init_vanilla_registry();
    vanilla_poi_types::LODESTONE.id()
}

fn vanilla_tickets(type_id: usize) -> u32 {
    REGISTRY
        .poi_types
        .by_id(type_id)
        .expect("vanilla POI type id must resolve")
        .ticket_count
}

/// Builds a storage holding each POI with the ticket count its vanilla type declares.
fn storage_with(pois: &[(BlockPos, usize)]) -> PointOfInterestStorage {
    init_vanilla_registry();
    let mut storage = PointOfInterestStorage::new();
    for &(pos, type_id) in pois {
        storage.add(pos, type_id, vanilla_tickets(type_id));
    }
    storage
}

fn any_type(_type_id: usize) -> bool {
    true
}

fn any_pos(_pos: BlockPos) -> bool {
    true
}

fn sorted_positions(mut positions: Vec<(BlockPos, usize)>) -> Vec<BlockPos> {
    positions.sort_by_key(|(pos, _)| (pos.x(), pos.y(), pos.z()));
    positions.into_iter().map(|(pos, _)| pos).collect()
}

#[test]
fn horizontal_square_query_matches_y_unbounded_vanilla_search() {
    init_vanilla_registry();
    let mut storage = PointOfInterestStorage::new();
    storage.add(BlockPos::new(0, -64, 0), 7, 0);
    storage.add(BlockPos::new(0, 320, 0), 7, 0);
    storage.add(BlockPos::new(2, 64, 0), 7, 0);
    storage.add(BlockPos::new(0, 64, 2), 7, 0);

    let positions = sorted_positions(storage.get_in_horizontal_square(
        &|type_id| type_id == 7,
        BlockPos::new(0, 64, 0),
        1,
        OccupationStatus::Any,
    ));

    assert_eq!(
        positions,
        vec![BlockPos::new(0, -64, 0), BlockPos::new(0, 320, 0)]
    );
}

#[test]
fn range_query_keeps_only_what_the_sphere_covers_of_the_square() {
    // Both corners sit inside the |dx| <= 12, |dz| <= 12 square, but only the
    // near one is within 12 blocks; and the far-above bed is in the square at
    // any Y, because vanilla's square query never bounds Y.
    let near = BlockPos::new(6, 64, 6);
    let square_corner = BlockPos::new(10, 64, 10);
    let far_above = BlockPos::new(0, 200, 0);
    let storage = storage_with(&[(near, home()), (square_corner, home()), (far_above, home())]);
    let center = BlockPos::new(0, 64, 0);

    let square = sorted_positions(storage.get_in_horizontal_square(
        &any_type,
        center,
        12,
        OccupationStatus::Any,
    ));
    assert_eq!(square, vec![far_above, near, square_corner]);

    let in_range =
        sorted_positions(storage.get_in_range(&any_type, center, 12, OccupationStatus::Any));
    assert_eq!(in_range, vec![near]);
    assert_eq!(
        storage.count(&any_type, center, 12, OccupationStatus::Any),
        1
    );
}

#[test]
fn closest_query_beats_a_farther_bed_the_scan_reaches_first() {
    // The scan walks chunks from the lowest Z row upward, so the bed in chunk
    // (-2, -2) is visited before the one in chunk (0, 0). A `find` that stopped
    // at the first hit would answer with the far one.
    let far = BlockPos::new(-20, 64, -20);
    let near = BlockPos::new(4, 64, 4);
    let storage = storage_with(&[(far, home()), (near, home())]);
    let center = BlockPos::new(0, 64, 0);

    assert_eq!(
        storage.find(&any_type, &any_pos, center, 32, OccupationStatus::Any),
        Some(far),
        "find takes the first of the scan, which is the far bed"
    );
    assert_eq!(
        storage.find_closest(&any_type, &any_pos, center, 32, OccupationStatus::Any),
        Some(near),
    );
    assert_eq!(
        storage.find_all_closest_first_with_type(
            &any_type,
            &any_pos,
            center,
            32,
            OccupationStatus::Any
        ),
        vec![(near, home()), (far, home())],
    );
}

#[test]
fn a_section_is_scanned_in_packed_position_order() {
    // Both beds sit in section (0, 4, 0), so nothing but the within-section
    // order decides which one the scan reaches first. Packed order is
    // (x << 8) | (z << 4) | y, which puts the farther bed first -- so `find`
    // and `find_closest` must disagree, and they must disagree the same way
    // on every run. Vanilla walks a HashMap here and promises nothing.
    let lower_packed_but_farther = BlockPos::new(2, 64, 3);
    let higher_packed_but_nearer = BlockPos::new(5, 64, 1);
    let storage = storage_with(&[
        (higher_packed_but_nearer, home()),
        (lower_packed_but_farther, home()),
    ]);
    let center = BlockPos::new(8, 64, 8);

    assert_eq!(
        storage.find(&any_type, &any_pos, center, 16, OccupationStatus::Any),
        Some(lower_packed_but_farther),
    );
    assert_eq!(
        storage.find_closest(&any_type, &any_pos, center, 16, OccupationStatus::Any),
        Some(higher_packed_but_nearer),
    );
}

#[test]
fn claiming_a_bed_spends_its_only_ticket_until_it_is_released() {
    let bed = BlockPos::new(8, 64, 8);
    let mut storage = storage_with(&[(bed, home())]);
    let center = BlockPos::new(8, 64, 8);

    assert_eq!(
        storage.take(&any_type, &|_type_id, _pos| true, center, 16),
        Some(bed)
    );
    assert!(storage.is_occupied(bed));
    assert!(
        storage
            .get_in_range(&any_type, center, 16, OccupationStatus::Free)
            .is_empty(),
        "a claimed bed has no free ticket left"
    );
    assert_eq!(
        storage.get_in_range(&any_type, center, 16, OccupationStatus::Occupied),
        vec![(bed, home())]
    );
    assert_eq!(
        storage.take(&any_type, &|_type_id, _pos| true, center, 16),
        None,
        "the second villager finds no free bed"
    );

    assert!(storage.release_ticket(bed));
    assert!(!storage.is_occupied(bed));
    assert_eq!(
        storage.take(&any_type, &|_type_id, _pos| true, center, 16),
        Some(bed)
    );
}

#[test]
fn claiming_skips_a_bed_the_filter_rejects_without_touching_its_ticket() {
    let rejected = BlockPos::new(2, 64, 2);
    let accepted = BlockPos::new(6, 64, 6);
    let mut storage = storage_with(&[(rejected, home()), (accepted, home())]);
    let center = BlockPos::new(0, 64, 0);

    assert_eq!(
        storage.take(&any_type, &|_type_id, pos| pos != rejected, center, 16),
        Some(accepted)
    );
    assert!(!storage.is_occupied(rejected));
    assert!(storage.is_occupied(accepted));
}

#[test]
fn random_pick_can_reach_every_position_the_filter_accepts() {
    let rejected = BlockPos::new(1, 64, 1);
    let first = BlockPos::new(2, 64, 2);
    let second = BlockPos::new(3, 64, 3);
    let storage = storage_with(&[(rejected, home()), (first, home()), (second, home())]);
    let center = BlockPos::new(0, 64, 0);
    let mut rng = rand::rng();

    let mut seen_first = false;
    let mut seen_second = false;
    for _ in 0..200 {
        let picked = storage
            .get_random(
                &any_type,
                &|pos| pos != rejected,
                OccupationStatus::Any,
                center,
                16,
                &mut rng,
            )
            .expect("two beds pass the filter");
        assert_ne!(picked, rejected);
        seen_first |= picked == first;
        seen_second |= picked == second;
    }
    assert!(
        seen_first && seen_second,
        "the shuffle must be able to return either accepted bed"
    );
}

#[test]
fn village_distance_is_the_chebyshev_section_distance_to_an_occupied_village_poi() {
    // A bell in section (0, 4, 0), claimed so that it counts as occupied.
    let bell = BlockPos::new(8, 64, 8);
    let mut storage = storage_with(&[(bell, meeting())]);
    assert!(storage.reserve_ticket(bell));

    let at = |x: i32, y: i32, z: i32| storage.sections_to_village(SectionPos::new(x, y, z));

    assert_eq!(at(0, 4, 0), 0);
    assert_eq!(at(3, 4, 0), 3);
    assert_eq!(
        at(3, 4, 3),
        3,
        "the tracker steps diagonally, so this is 3 and not 6"
    );
    assert_eq!(at(0, 4 + MAX_VILLAGE_DISTANCE, 0), MAX_VILLAGE_DISTANCE);
    assert_eq!(at(MAX_VILLAGE_DISTANCE + 1, 4, 0), NO_VILLAGE_DISTANCE);
}

#[test]
fn an_unclaimed_bell_is_not_a_village_center() {
    let bell = BlockPos::new(8, 64, 8);
    let storage = storage_with(&[(bell, meeting())]);

    assert_eq!(
        storage.sections_to_village(SectionPos::new(0, 4, 0)),
        NO_VILLAGE_DISTANCE,
        "vanilla only counts a section whose village POI is occupied"
    );
}

#[test]
fn an_occupied_poi_outside_the_village_tag_is_not_a_village_center() {
    init_vanilla_registry();
    let mut storage = PointOfInterestStorage::new();
    // A lodestone declares no tickets, so holding one makes it read as
    // occupied; it is still not tagged `#minecraft:village`.
    storage.add(BlockPos::new(8, 64, 8), lodestone(), 1);

    assert_eq!(
        storage.sections_to_village(SectionPos::new(0, 4, 0)),
        NO_VILLAGE_DISTANCE
    );
}

#[test]
fn exists_answers_for_the_type_actually_stored() {
    let bed = BlockPos::new(8, 64, 8);
    let storage = storage_with(&[(bed, home())]);

    assert!(storage.exists(bed, &|type_id| type_id == home()));
    assert!(!storage.exists(bed, &|type_id| type_id == meeting()));
    assert!(!storage.exists(BlockPos::new(9, 64, 8), &any_type));
}
