//! What a boss bar must and must not send.
//!
//! The regressions worth catching are all about the player set: a bar that
//! keeps updating a client that stopped watching, and a bar that never comes
//! down when the watcher leaves.

use foton_registry::init_vanilla_registry;

use super::*;
use crate::test_support::{
    BossBarViewer, OP_ADD, OP_REMOVE, OP_UPDATE_PROGRESS, OP_UPDATE_STYLE, fresh_test_world,
};
use crate::world::World;

fn bar() -> ServerBossEvent {
    ServerBossEvent::with_random_id(
        TextComponent::plain("Wither"),
        BossBarColor::Purple,
        BossBarOverlay::Progress,
    )
}

fn prepared_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    fresh_test_world(key)
}

#[test]
fn a_bar_only_reaches_a_client_once_that_client_has_been_added() {
    let world = prepared_world("boss_event_add");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar();

    bar.set_progress(0.5);
    assert!(
        viewer.take_boss_operations().is_empty(),
        "a bar with no viewers must not send anything"
    );

    bar.add_player(&viewer.player);
    assert_eq!(viewer.take_boss_operations(), vec![OP_ADD]);

    bar.set_progress(0.25);
    assert_eq!(viewer.take_boss_operations(), vec![OP_UPDATE_PROGRESS]);
}

#[test]
fn a_removed_viewer_gets_the_bar_taken_down_and_hears_nothing_after() {
    let world = prepared_world("boss_event_remove");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar();

    bar.add_player(&viewer.player);
    viewer.take_boss_operations();

    bar.remove_player(&viewer.player);
    assert_eq!(
        viewer.take_boss_operations(),
        vec![OP_REMOVE],
        "the bar must come down when the viewer stops watching"
    );

    bar.set_progress(0.1);
    bar.set_color(BossBarColor::Red);
    assert!(
        viewer.take_boss_operations().is_empty(),
        "a bar must never keep updating a client that left"
    );
}

#[test]
fn one_viewer_leaving_does_not_take_the_bar_off_the_others() {
    let world = prepared_world("boss_event_two_viewers");
    let leaving = BossBarViewer::new(&world, "Leaving", 1);
    let staying = BossBarViewer::new(&world, "Staying", 2);
    let bar = bar();

    bar.add_player(&leaving.player);
    bar.add_player(&staying.player);
    leaving.take_boss_operations();
    staying.take_boss_operations();

    bar.remove_player(&leaving.player);
    assert_eq!(leaving.take_boss_operations(), vec![OP_REMOVE]);
    assert_eq!(
        staying.take_boss_operations(),
        Vec::<i32>::new(),
        "one viewer leaving must not touch the others"
    );

    bar.set_progress(0.5);
    assert_eq!(staying.take_boss_operations(), vec![OP_UPDATE_PROGRESS]);
    assert!(leaving.take_boss_operations().is_empty());
}

#[test]
fn setting_a_value_it_already_has_sends_nothing() {
    let world = prepared_world("boss_event_no_op");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar();
    bar.add_player(&viewer.player);
    viewer.take_boss_operations();

    bar.set_progress(bar.progress());
    bar.set_color(bar.color());
    bar.set_overlay(bar.overlay());
    bar.set_name(bar.name());
    bar.set_darken_screen(false);

    assert!(
        viewer.take_boss_operations().is_empty(),
        "vanilla only broadcasts a setter that actually changed something"
    );
}

#[test]
fn color_and_overlay_share_the_one_style_update() {
    let world = prepared_world("boss_event_style");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar();
    bar.add_player(&viewer.player);
    viewer.take_boss_operations();

    bar.set_color(BossBarColor::Red);
    bar.set_overlay(BossBarOverlay::Notched10);

    assert_eq!(
        viewer.take_boss_operations(),
        vec![OP_UPDATE_STYLE, OP_UPDATE_STYLE]
    );
}

#[test]
fn hiding_a_bar_takes_it_down_everywhere_and_showing_it_puts_it_back() {
    let world = prepared_world("boss_event_visibility");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar();
    bar.add_player(&viewer.player);
    viewer.take_boss_operations();

    bar.set_visible(false);
    assert_eq!(viewer.take_boss_operations(), vec![OP_REMOVE]);

    bar.set_progress(0.5);
    assert!(
        viewer.take_boss_operations().is_empty(),
        "a hidden bar broadcasts nothing"
    );

    bar.set_visible(true);
    assert_eq!(viewer.take_boss_operations(), vec![OP_ADD]);
}

#[test]
fn a_player_added_twice_is_still_one_viewer() {
    let world = prepared_world("boss_event_double_add");
    let viewer = BossBarViewer::new(&world, "Watcher", 1);
    let bar = bar();

    bar.add_player(&viewer.player);
    bar.add_player(&viewer.player);

    assert_eq!(viewer.take_boss_operations(), vec![OP_ADD]);
    assert_eq!(bar.players().len(), 1);
}

#[test]
fn removing_every_player_clears_the_viewer_set() {
    let world = prepared_world("boss_event_remove_all");
    let first = BossBarViewer::new(&world, "First", 1);
    let second = BossBarViewer::new(&world, "Second", 2);
    let bar = bar();
    bar.add_player(&first.player);
    bar.add_player(&second.player);
    first.take_boss_operations();
    second.take_boss_operations();

    bar.remove_all_players();

    assert_eq!(first.take_boss_operations(), vec![OP_REMOVE]);
    assert_eq!(second.take_boss_operations(), vec![OP_REMOVE]);
    assert!(!bar.has_players());
}
