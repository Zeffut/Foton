use std::collections::BTreeSet;

use steel_registry::init_vanilla_registry;
use steel_utils::Identifier;

use super::player_advancements::PlayerAdvancements;
use super::player_advancements::TabSelection;
use super::tree::ADVANCEMENT_TREE;
use super::{TRIGGER_INDEX, visibility};

fn node(key: &'static str) -> usize {
    ADVANCEMENT_TREE
        .index_of(&Identifier::vanilla_static(key))
        .unwrap_or_else(|| panic!("{key} should be in the tree"))
}

/// The tree has to hold every registered advancement. Vanilla drops entries
/// whose parent never arrives, and a dropped subtree is an invisible tab.
#[test]
fn the_tree_holds_every_registered_advancement() {
    init_vanilla_registry();

    assert_eq!(ADVANCEMENT_TREE.len(), 1_688);
    assert_eq!(
        ADVANCEMENT_TREE.roots().len(),
        6,
        "five drawn tabs plus the undrawn recipe root"
    );
}

/// Every node's `root` has to be the end of its parent chain, because that is
/// the key the dirty-visibility set is bucketed by.
#[test]
fn every_node_resolves_to_the_root_of_its_own_chain() {
    init_vanilla_registry();

    for index in ADVANCEMENT_TREE.indices() {
        let mut walker = index;
        while let Some(parent) = ADVANCEMENT_TREE.node(walker).parent {
            walker = parent;
        }
        assert_eq!(
            ADVANCEMENT_TREE.node(index).root,
            walker,
            "{} resolved to the wrong root",
            ADVANCEMENT_TREE.node(index).advancement.key
        );
        assert!(
            ADVANCEMENT_TREE.node(walker).parent.is_none(),
            "a root should have no parent"
        );
    }
}

/// A player who has done nothing sees nothing at all -- not even the tabs.
/// Vanilla is the same: the story tab appears when the crafting table does.
#[test]
fn a_player_with_no_progress_sees_nothing() {
    init_vanilla_registry();

    let advancements = PlayerAdvancements::new();
    for &root in ADVANCEMENT_TREE.roots() {
        visibility::evaluate(
            &ADVANCEMENT_TREE,
            root,
            &mut |node| advancements.is_done(node),
            &mut |node, visible| {
                assert!(
                    !visible,
                    "{} should be hidden from a player with no progress",
                    ADVANCEMENT_TREE.node(node).advancement.key
                );
            },
        );
    }
}

/// Completing a root reveals it, its children and its grandchildren -- and
/// stops there. That two-level reach is the whole point of the rule, and a
/// version that revealed everything would pass a weaker test.
#[test]
fn completing_a_root_reveals_exactly_two_levels_below_it() {
    init_vanilla_registry();

    let story_root = node("story/root");
    let mine_stone = node("story/mine_stone");
    let upgrade_tools = node("story/upgrade_tools");
    let smelt_iron = node("story/smelt_iron");

    // The chain is root -> mine_stone -> upgrade_tools -> smelt_iron, so the
    // fourth link is the one that must stay hidden.
    assert_eq!(ADVANCEMENT_TREE.node(mine_stone).parent, Some(story_root));
    assert_eq!(
        ADVANCEMENT_TREE.node(upgrade_tools).parent,
        Some(mine_stone)
    );
    assert_eq!(
        ADVANCEMENT_TREE.node(smelt_iron).parent,
        Some(upgrade_tools)
    );

    let mut visible = BTreeSet::new();
    visibility::evaluate(
        &ADVANCEMENT_TREE,
        story_root,
        &mut |node| node == story_root,
        &mut |node, is_visible| {
            if is_visible {
                visible.insert(node);
            }
        },
    );

    assert!(visible.contains(&story_root), "the finished root shows");
    assert!(visible.contains(&mine_stone), "its child shows");
    assert!(visible.contains(&upgrade_tools), "its grandchild shows");
    assert!(
        !visible.contains(&smelt_iron),
        "its great-grandchild stays hidden"
    );
}

/// A finished advancement deep in a tab reveals the whole chain above it, with
/// no depth limit. That is the asymmetric half of the rule.
#[test]
fn completing_a_deep_advancement_reveals_every_ancestor() {
    init_vanilla_registry();

    let story_root = node("story/root");
    let smelt_iron = node("story/smelt_iron");

    let mut visible = BTreeSet::new();
    visibility::evaluate(
        &ADVANCEMENT_TREE,
        story_root,
        &mut |node| node == smelt_iron,
        &mut |node, is_visible| {
            if is_visible {
                visible.insert(node);
            }
        },
    );

    let mut walker = smelt_iron;
    loop {
        assert!(
            visible.contains(&walker),
            "{} should be revealed by a finished descendant",
            ADVANCEMENT_TREE.node(walker).advancement.key
        );
        match ADVANCEMENT_TREE.node(walker).parent {
            Some(parent) => walker = parent,
            None => break,
        }
    }
    assert_eq!(walker, story_root);
}

/// A flush has to say something the first time and nothing the second, or the
/// client is told the same thing every tick.
#[test]
fn a_flush_reports_once_and_then_goes_quiet() {
    init_vanilla_registry();

    let mut advancements = PlayerAdvancements::new();
    // A player with no progress at all: vanilla sends no packet either, and
    // the first-packet flag is spent regardless.
    assert!(advancements.is_first_packet());
    assert!(advancements.flush_dirty().is_none());
    assert!(!advancements.is_first_packet());

    let story_root = node("story/root");
    let outcome = advancements.award(story_root, "crafting_table", 1_000);
    assert!(outcome.granted && outcome.completed);

    let update = advancements
        .flush_dirty()
        .expect("finishing an advancement has to reach the client");
    assert!(!update.reset, "only the first packet resets the tree");
    assert!(
        update
            .added
            .iter()
            .any(|advancement| advancement.key.path == "story/root"),
        "the finished root becomes visible"
    );
    assert!(
        update
            .progress
            .iter()
            .any(|(key, progress)| key.path == "story/root" && progress.is_done()),
        "and its progress says so"
    );

    assert!(
        advancements.flush_dirty().is_none(),
        "a second flush with nothing new must stay silent"
    );
}

/// Awarding the same criterion twice must not re-announce it, or every
/// inventory change would re-toast an advancement the player already has.
#[test]
fn awarding_a_met_criterion_again_changes_nothing() {
    init_vanilla_registry();

    let mut advancements = PlayerAdvancements::new();
    let story_root = node("story/root");

    let first = advancements.award(story_root, "crafting_table", 1_000);
    assert!(first.granted && first.completed);

    let second = advancements.award(story_root, "crafting_table", 2_000);
    assert!(!second.granted && !second.completed);

    let unknown = advancements.award(story_root, "not_a_criterion", 3_000);
    assert!(!unknown.granted);
}

/// Saved progress has to survive a round trip, and progress for an advancement
/// that no longer exists must be dropped rather than blowing up the load.
#[test]
fn progress_round_trips_through_the_save_shape() {
    init_vanilla_registry();

    let mut advancements = PlayerAdvancements::new();
    let story_root = node("story/root");
    advancements.award(story_root, "crafting_table", 1_234);

    let saved = advancements.save_data();
    assert_eq!(
        saved.len(),
        1,
        "only advancements with progress are written out"
    );
    assert_eq!(saved[0].0.path, "story/root");
    assert_eq!(saved[0].1, vec![("crafting_table", 1_234)]);

    let mut restored = PlayerAdvancements::new();
    let mut with_junk: Vec<(Identifier, Vec<(String, i64)>)> = saved
        .into_iter()
        .map(|(key, criteria)| {
            (
                key,
                criteria
                    .into_iter()
                    .map(|(name, at)| (name.to_owned(), at))
                    .collect(),
            )
        })
        .collect();
    with_junk.push((
        Identifier::vanilla_static("story/no_such_advancement"),
        vec![("whatever".to_owned(), 1)],
    ));
    with_junk.push((
        Identifier::vanilla_static("story/mine_stone"),
        vec![("no_such_criterion".to_owned(), 1)],
    ));

    restored.load(with_junk);
    assert!(restored.is_done(story_root));
    assert!(
        !restored.is_done(node("story/mine_stone")),
        "a criterion the advancement no longer declares must not count"
    );
}

/// Only a drawn root can be a tab, and re-selecting the same tab must not echo
/// a packet back.
#[test]
fn only_a_drawn_root_can_be_the_selected_tab() {
    init_vanilla_registry();

    let mut advancements = PlayerAdvancements::new();
    let story_root = node("story/root");
    let mine_stone = node("story/mine_stone");
    let recipes_root = node("recipes/root");

    let echoed = advancements
        .set_selected_tab(Some(story_root))
        .expect("selecting a new tab echoes it back");
    assert_eq!(
        echoed,
        TabSelection::Selected(Identifier::vanilla_static("story/root"))
    );
    assert!(
        advancements.set_selected_tab(Some(story_root)).is_none(),
        "re-selecting the same tab says nothing"
    );

    // A non-root clears the selection, which is what vanilla does.
    let cleared = advancements
        .set_selected_tab(Some(mine_stone))
        .expect("an invalid tab clears the selection");
    assert_eq!(cleared, TabSelection::Cleared);
    assert_eq!(advancements.selected_tab(), None);

    // The recipe root has no display, so it is not a tab either.
    assert!(advancements.set_selected_tab(Some(recipes_root)).is_none());
    assert_eq!(advancements.selected_tab(), None);
}

/// The trigger index has to cover every criterion, or a trigger that fires
/// finds nothing to award and the advancement is silently unreachable.
#[test]
fn the_trigger_index_covers_every_criterion() {
    init_vanilla_registry();

    let mut indexed = 0_usize;
    let mut declared = 0_usize;
    for node in ADVANCEMENT_TREE.indices() {
        declared += ADVANCEMENT_TREE.node(node).advancement.criteria.len();
    }
    let mut triggers = BTreeSet::new();
    for node in ADVANCEMENT_TREE.indices() {
        for criterion in ADVANCEMENT_TREE.node(node).advancement.criteria {
            triggers.insert(criterion.trigger.trigger_id());
        }
    }
    for trigger in &triggers {
        indexed += TRIGGER_INDEX.criteria_for(trigger).len();
    }

    assert_eq!(indexed, declared);
    assert_eq!(triggers.len(), 54);
    assert!(
        TRIGGER_INDEX
            .criteria_for("minecraft:not_a_trigger")
            .is_empty(),
        "an unknown trigger indexes nothing"
    );

    // The one that carries the whole recipe tree, and the one the in-world
    // test leans on.
    assert_eq!(
        TRIGGER_INDEX
            .criteria_for("minecraft:inventory_changed")
            .len(),
        1_738
    );
}
