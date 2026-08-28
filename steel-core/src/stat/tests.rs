use steel_registry::stat::Stat;
use steel_registry::{init_vanilla_registry, vanilla_custom_stats};

use super::StatsCounter;

fn jump() -> Stat {
    Stat::custom(&vanilla_custom_stats::JUMP)
}

fn deaths() -> Stat {
    Stat::custom(&vanilla_custom_stats::DEATHS)
}

/// Vanilla adds in a `long` and clamps, so a counter that is already at the
/// maximum stays there instead of wrapping negative -- which is what a player
/// with a very long-running world would otherwise see on the screen.
#[test]
fn incrementing_saturates_instead_of_wrapping() {
    init_vanilla_registry();

    let mut stats = StatsCounter::new();
    stats.set(jump(), i32::MAX);
    stats.increment(jump(), 5);

    assert_eq!(stats.value(jump()), i32::MAX);
}

/// The dirty set is what decides the packet, so it has to empty as it is read
/// and refill only on a real change.
#[test]
fn the_dirty_set_empties_as_it_is_read() {
    init_vanilla_registry();

    let mut stats = StatsCounter::new();
    stats.increment(jump(), 1);
    stats.increment(deaths(), 2);

    let sent = stats.take_dirty();
    assert_eq!(sent.len(), 2);
    assert!(sent.contains(&(jump(), 1)));
    assert!(sent.contains(&(deaths(), 2)));

    assert!(
        stats.take_dirty().is_empty(),
        "a second read with nothing new must send nothing"
    );

    stats.increment(jump(), 1);
    assert_eq!(stats.take_dirty(), vec![(jump(), 2)]);
}

/// A join re-sends everything, because the client starts with an empty screen.
#[test]
fn marking_all_dirty_resends_what_was_already_counted() {
    init_vanilla_registry();

    let mut stats = StatsCounter::new();
    stats.increment(jump(), 3);
    assert_eq!(stats.take_dirty().len(), 1);

    stats.mark_all_dirty();
    assert_eq!(stats.take_dirty(), vec![(jump(), 3)]);
}

/// An unset statistic reads zero rather than being absent, which is what makes
/// `increment` work on a counter that has never seen the statistic before.
#[test]
fn an_unset_statistic_reads_zero() {
    init_vanilla_registry();

    let stats = StatsCounter::new();
    assert_eq!(stats.value(jump()), 0);
}
