//! What a named boss bar owes its command and its save file.
//!
//! Two things here are not obvious from the types. A bar's fill is a pair of
//! whole numbers rather than the float the packet carries, so the division is
//! the bar's own job and the clamp is what stops a `value` above `max` from
//! drawing past the end. And a bar has two player sets: logging out has to
//! leave one and not the other, or a bar assigned to somebody is gone the
//! moment they reconnect.

use steel_registry::init_vanilla_registry;

use crate::world::World;

use super::*;
use crate::test_support::{BossBarViewer, OP_ADD, OP_REMOVE, fresh_test_world};

fn prepared_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    fresh_test_world(key)
}

fn bar(id: &str) -> CustomBossEvent {
    CustomBossEvent::new(
        Uuid::new_v4(),
        Identifier::vanilla(id.to_owned()),
        TextComponent::plain("Progress"),
    )
}

/// A bar's fill is `value` out of `max`, and neither number is the float the
/// packet carries.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "an exact fraction of two small whole numbers"
)]
fn the_fill_is_the_two_whole_numbers_divided() {
    let bar = bar("counter");
    // Vanilla starts a named bar empty, unlike a boss's own bar which starts
    // full.
    assert_eq!(bar.value(), 0);
    assert_eq!(bar.max(), DEFAULT_MAX);
    assert_eq!(bar.event().progress(), 0.0);

    bar.set_max(50);
    bar.set_value(25);
    assert_eq!(bar.event().progress(), 0.5);

    // A value past the maximum draws a full bar rather than past the end.
    bar.set_value(500);
    assert_eq!(bar.event().progress(), 1.0);
    assert_eq!(bar.value(), 500, "the number itself is kept as it was set");

    // And raising the maximum under it moves the fill back down.
    bar.set_max(1000);
    assert_eq!(bar.event().progress(), 0.5);
}

/// Logging out takes the bar off the screen and leaves the assignment alone.
/// Vanilla goes out of its way to call `super.removePlayer` for exactly this.
#[test]
fn a_player_who_logs_out_stays_assigned_and_gets_the_bar_back() {
    let world = prepared_world("boss_bar_reconnect");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar("welcome_back");

    bar.add_player(&viewer.player);
    assert_eq!(viewer.take_boss_operations(), vec![OP_ADD]);
    assert_eq!(bar.assigned_players().len(), 1);

    bar.on_player_disconnect(&viewer.player);
    assert_eq!(viewer.take_boss_operations(), vec![OP_REMOVE]);
    assert_eq!(
        bar.assigned_players().len(),
        1,
        "a logout is not the same as being taken off the bar"
    );

    bar.on_player_connect(&viewer.player);
    assert_eq!(viewer.take_boss_operations(), vec![OP_ADD]);
}

/// Being taken off the bar is the other case, and it does unassign.
#[test]
fn a_player_taken_off_the_bar_does_not_get_it_back() {
    let world = prepared_world("boss_bar_removed");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar("goodbye");

    bar.add_player(&viewer.player);
    let _ = viewer.take_boss_operations();

    bar.remove_player(&viewer.player);
    assert_eq!(viewer.take_boss_operations(), vec![OP_REMOVE]);
    assert!(bar.assigned_players().is_empty());

    bar.on_player_connect(&viewer.player);
    assert!(
        viewer.take_boss_operations().is_empty(),
        "a player nobody assigned should not be handed the bar on login"
    );
}

/// `/bossbar set <id> players` fails when it would change nothing, so the
/// answer has to be honest about whether it did.
#[test]
fn setting_the_players_says_whether_anything_changed() {
    let world = prepared_world("boss_bar_set_players");
    let first = BossBarViewer::new(&world, "First", 1);
    let second = BossBarViewer::new(&world, "Second", 2);
    let bar = bar("audience");

    let both = [Arc::clone(&first.player), Arc::clone(&second.player)];
    assert!(bar.set_players(&both));
    assert_eq!(bar.assigned_players().len(), 2);

    assert!(
        !bar.set_players(&both),
        "the same two players are not a change"
    );
    // The order the command names them in is not a change either.
    let swapped = [Arc::clone(&second.player), Arc::clone(&first.player)];
    assert!(!bar.set_players(&swapped));

    assert!(bar.set_players(&[Arc::clone(&first.player)]));
    assert_eq!(bar.assigned_players(), vec![first.player.uuid()]);

    assert!(bar.set_players(&[]));
    assert!(bar.assigned_players().is_empty());
    assert!(
        !bar.set_players(&[]),
        "emptying an empty bar changes nothing"
    );
}

/// Builds a collection holding one fully dressed bar.
fn dressed_collection() -> (CustomBossEvents, Uuid) {
    let events = CustomBossEvents::new();
    let id = Identifier::vanilla("saved".to_owned());
    let bar = events.create(id, TextComponent::plain("Saved Bar"));
    bar.set_max(7);
    bar.set_value(3);
    bar.event().set_color(BossBarColor::Purple);
    bar.event().set_overlay(BossBarOverlay::Notched12);
    bar.event().set_darken_screen(true);
    bar.event().set_create_world_fog(true);
    bar.event().set_visible(false);
    let assigned = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
    bar.assigned.lock().insert(assigned);
    (events, assigned)
}

/// Everything a command can set has to come back off disk. Every value below
/// is deliberately not the one a fresh bar has, so a reader that does nothing
/// cannot pass by agreeing with the defaults.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "an exact fraction of two small whole numbers"
)]
fn a_bar_survives_the_round_trip_through_the_save_file() {
    let (events, assigned) = dressed_collection();
    let Some(snapshot) = events.pending_save() else {
        panic!("a collection with a new bar in it has something to write");
    };

    let Ok(reloaded) = CustomBossEvents::from_persistent(snapshot.state) else {
        panic!("what was just packed should unpack");
    };
    let Some(bar) = reloaded.get(&Identifier::vanilla("saved".to_owned())) else {
        panic!("the bar should come back under its own id");
    };

    assert_eq!(bar.value(), 3);
    assert_eq!(bar.max(), 7);
    assert_eq!(bar.event().progress(), 3.0 / 7.0);
    assert_eq!(bar.event().name(), TextComponent::plain("Saved Bar"));
    assert_eq!(bar.event().color(), BossBarColor::Purple);
    assert_eq!(bar.event().overlay(), BossBarOverlay::Notched12);
    assert!(bar.event().properties().darken_screen);
    assert!(bar.event().properties().create_world_fog);
    assert!(!bar.event().properties().play_boss_music);
    assert!(!bar.event().is_visible());
    assert_eq!(bar.assigned_players(), vec![assigned]);
}

/// A bar that has just been read is not a bar that has just changed.
///
/// Loading goes through the same setters a command does, and every one of them
/// marks the bar dirty. Leaving the flag set would make the first autosave
/// rewrite every file on the disk for no reason.
#[test]
fn a_collection_read_off_disk_has_nothing_to_write() {
    let (events, _) = dressed_collection();
    let Some(snapshot) = events.pending_save() else {
        panic!("a collection with a new bar in it has something to write");
    };
    let Ok(reloaded) = CustomBossEvents::from_persistent(snapshot.state) else {
        panic!("what was just packed should unpack");
    };

    assert!(
        reloaded.pending_save().is_none(),
        "nothing changed between reading the file and asking"
    );
}

/// A change to a bar has to reach the collection that writes it, even though
/// the bar does not know the collection exists.
#[test]
fn a_change_to_a_bar_makes_its_domain_worth_writing() {
    let events = CustomBossEvents::new();
    let bar = events.create(
        Identifier::vanilla("tracked".to_owned()),
        TextComponent::plain("Tracked"),
    );
    let Some(snapshot) = events.pending_save() else {
        panic!("a new bar is a change");
    };
    events.mark_saved(snapshot.revision);
    assert!(events.pending_save().is_none());

    bar.event().set_color(BossBarColor::Red);
    assert!(
        events.pending_save().is_some(),
        "a color set on the bar has to reach the collection that saves it"
    );
}
