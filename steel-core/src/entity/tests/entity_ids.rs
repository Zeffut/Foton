//! Entity ID allocation.

use std::thread;

use rustc_hash::FxHashSet;

use crate::entity::{next_entity_id, reserve_entity_ids};

#[test]
fn a_reserved_block_is_consecutive_and_the_next_caller_starts_past_it() {
    let first = reserve_entity_ids(9);

    assert_eq!(next_entity_id(), first + 9);
}

#[test]
fn ids_inside_a_reserved_block_are_never_handed_to_a_concurrent_caller() {
    const THREADS: usize = 8;
    const BLOCKS_PER_THREAD: usize = 64;
    const BLOCK: u32 = 9;

    let blocks: Vec<i32> = thread::scope(|scope| {
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                scope.spawn(|| {
                    (0..BLOCKS_PER_THREAD)
                        .map(|_| reserve_entity_ids(BLOCK))
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("reservation thread panicked"))
            .collect()
    });

    let mut seen = FxHashSet::default();
    for first in blocks {
        for id in first..first + BLOCK as i32 {
            assert!(
                seen.insert(id),
                "entity ID {id} was handed out twice across concurrent reservations"
            );
        }
    }

    assert_eq!(seen.len(), THREADS * BLOCKS_PER_THREAD * BLOCK as usize);
}
