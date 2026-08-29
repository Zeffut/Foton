use foton_utils::Identifier;

use super::{AdvancementRequirements, AdvancementType, TriggerInstance};
use crate::advancement::progress::AdvancementProgress;
use crate::vanilla_advancements::VANILLA_ADVANCEMENT_COUNT;
use crate::{REGISTRY, RegistryExt as _, init_vanilla_registry};

/// Every generated advancement has to reach the registry. A registry that has
/// lost entries answers `None` to lookups that should succeed, and a whole tab
/// then silently fails to appear.
#[test]
fn every_generated_advancement_is_registered() {
    init_vanilla_registry();

    // Two separate claims: the generator saw every file, and the registry kept
    // every entry the generator produced.
    assert_eq!(
        VANILLA_ADVANCEMENT_COUNT, 1_688,
        "MC 26.2's built-in datapack holds 1688 advancement files; a different \
         number means the extracted datapack is truncated or the version moved"
    );
    assert_eq!(REGISTRY.advancements.len(), VANILLA_ADVANCEMENT_COUNT);
}

/// The five drawn tabs, their frames and their backgrounds. A root whose
/// background went missing renders as a black tab.
#[test]
fn the_drawn_roots_keep_their_tab_backgrounds() {
    init_vanilla_registry();

    let roots = [
        "story/root",
        "adventure/root",
        "nether/root",
        "end/root",
        "husbandry/root",
    ];
    for key in roots {
        let advancement = REGISTRY
            .advancements
            .by_key(&Identifier::vanilla_static(key))
            .unwrap_or_else(|| panic!("{key} should be registered"));
        assert!(advancement.is_root(), "{key} should have no parent");
        let display = advancement
            .display
            .as_ref()
            .unwrap_or_else(|| panic!("{key} should be drawn"));
        assert!(
            display.background.is_some(),
            "{key} should carry a tab background"
        );
    }

    // The recipe tree hangs off an undrawn root, which is why the recipe book
    // advancements never show up on the advancement screen.
    let recipes = REGISTRY
        .advancements
        .by_key(&Identifier::vanilla_static("recipes/root"))
        .expect("recipes/root should be registered");
    assert!(recipes.is_root());
    assert!(recipes.display.is_none());
}

/// Parent links have to resolve, or `AdvancementTree` drops whole subtrees.
#[test]
fn every_parent_link_resolves_to_a_registered_advancement() {
    init_vanilla_registry();

    for advancement in REGISTRY.advancements.iter() {
        let Some(parent) = advancement.parent.as_ref() else {
            continue;
        };
        assert!(
            REGISTRY.advancements.by_key(parent).is_some(),
            "{} points at the unknown parent {parent}",
            advancement.key
        );
    }
}

/// The requirement matrix has to name exactly the criteria that exist. A
/// mismatch makes an advancement either unearnable or earnable too early, and
/// vanilla rejects the datapack outright for it.
#[test]
fn requirements_name_exactly_the_declared_criteria() {
    init_vanilla_registry();

    for advancement in REGISTRY.advancements.iter() {
        assert!(
            !advancement.criteria.is_empty(),
            "{} has no criteria",
            advancement.key
        );
        assert!(
            !advancement.requirements.groups.is_empty(),
            "{} has empty requirements and could never be earned",
            advancement.key
        );

        let mut referenced: Vec<&str> = advancement.requirements.names().collect();
        referenced.sort_unstable();
        referenced.dedup();
        let mut declared: Vec<&str> = advancement
            .criteria
            .iter()
            .map(|criterion| criterion.name)
            .collect();
        declared.sort_unstable();
        assert_eq!(
            referenced, declared,
            "{} requirements do not match its criteria",
            advancement.key
        );
    }
}

/// The layout has to give every drawn advancement its own cell. Two icons at
/// the same coordinates overlap on the client, and the whole tab collapsing
/// onto (0, 0) is exactly what a missing layout looks like.
#[test]
fn the_tree_layout_gives_every_drawn_advancement_its_own_cell() {
    init_vanilla_registry();

    let mut per_tab: std::collections::BTreeMap<&str, Vec<(i32, i32)>> =
        std::collections::BTreeMap::new();
    let mut drawn = 0_usize;
    for advancement in REGISTRY.advancements.iter() {
        let Some(display) = advancement.display.as_ref() else {
            continue;
        };
        drawn += 1;
        let tab = advancement
            .key
            .path
            .split('/')
            .next()
            .expect("an advancement key always has a first path segment");
        #[expect(
            clippy::cast_possible_truncation,
            reason = "layout coordinates are whole rows and columns"
        )]
        per_tab
            .entry(tab)
            .or_default()
            .push(((display.x * 2.0) as i32, (display.y * 2.0) as i32));
    }

    assert_eq!(
        drawn, 126,
        "the five drawn tabs hold 126 advancements between them"
    );
    for (tab, mut cells) in per_tab {
        let total = cells.len();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(
            cells.len(),
            total,
            "two advancements of the {tab} tab landed on the same cell"
        );
    }
}

/// The three icons that carry a data component patch have to decode with it.
/// The runtime falls back to the bare item when a patch will not decode, so
/// without this the ominous banner would quietly become a white one.
#[test]
fn advancement_icons_decode_with_their_components() {
    init_vanilla_registry();

    let mut with_components = 0_usize;
    for advancement in REGISTRY.advancements.iter() {
        let Some(display) = advancement.display.as_ref() else {
            continue;
        };
        let template = display.icon.template();
        assert_eq!(
            template.item().key.to_string(),
            display.icon.item_id(),
            "{}: the decoded icon changed item",
            advancement.key
        );
        if display.icon.components_snbt().is_empty() {
            assert!(
                template.components().is_empty(),
                "{}: a bare icon gained a component patch",
                advancement.key
            );
            continue;
        }
        with_components += 1;
        assert!(
            !template.components().is_empty(),
            "{}: the icon's component patch did not decode, so it fell back to the bare item",
            advancement.key
        );
    }
    assert_eq!(
        with_components, 3,
        "vanilla decorates exactly three advancement icons with components"
    );
}

/// The `AND of ORs` evaluation, including vanilla's refusal to ever complete
/// an advancement with no requirement groups.
#[test]
fn requirements_are_an_and_of_ors() {
    let none = AdvancementRequirements { groups: &[] };
    assert!(!none.test(|_| true), "empty requirements are never met");

    let and = AdvancementRequirements {
        groups: &[&["a"], &["b"]],
    };
    assert!(and.test(|name| matches!(name, "a" | "b")));
    assert!(!and.test(|name| name == "a"));
    assert_eq!(and.count(|name| name == "a"), 1);

    let or = AdvancementRequirements {
        groups: &[&["a", "b"]],
    };
    assert!(or.test(|name| name == "b"));
    assert!(!or.test(|_| false));
}

/// Progress only means anything once it has been attached to a set of
/// requirements, and attaching has to prune names that no longer exist.
#[test]
fn progress_tracks_only_the_criteria_its_requirements_name() {
    let mut progress = AdvancementProgress::new();
    assert!(!progress.is_done(), "unattached progress is never done");

    progress.update(AdvancementRequirements {
        groups: &[&["first"], &["second"]],
    });
    assert!(!progress.is_done());
    assert!(!progress.has_progress());

    assert!(progress.grant("first", 1_000));
    assert!(!progress.grant("first", 2_000), "re-granting is a no-op");
    assert!(
        !progress.grant("nope", 3_000),
        "unknown criteria are refused"
    );
    assert!(progress.has_progress());
    assert!(!progress.is_done(), "the second group is still unmet");

    assert!(progress.grant("second", 4_000));
    assert!(progress.is_done());
    assert_eq!(progress.first_progress_date(), Some(1_000));

    // Re-attaching to a narrower set drops what it no longer names.
    progress.update(AdvancementRequirements {
        groups: &[&["second"]],
    });
    assert!(progress.criterion("first").is_none());
    assert!(progress.is_done());

    assert!(progress.revoke("second"));
    assert!(!progress.is_done());
    assert!(!progress.revoke("second"), "re-revoking is a no-op");
}

/// The frame ordinals are the wire form, so their order is protocol-visible.
#[test]
fn advancement_frames_keep_their_wire_ordinals() {
    assert_eq!(AdvancementType::Task as u8, 0);
    assert_eq!(AdvancementType::Challenge as u8, 1);
    assert_eq!(AdvancementType::Goal as u8, 2);
}

/// Every criterion the datapack declares must have been lowered into a trigger
/// Foton names. The build script panics on an unknown one, so this is the
/// belt-and-braces check that the panic path is the only way through.
#[test]
fn every_criterion_carries_a_named_trigger() {
    init_vanilla_registry();

    let mut seen = std::collections::BTreeSet::new();
    for advancement in REGISTRY.advancements.iter() {
        for criterion in advancement.criteria {
            let id = criterion.trigger.trigger_id();
            assert!(
                id.starts_with("minecraft:"),
                "{}: {} has an unnamespaced trigger",
                advancement.key,
                criterion.name
            );
            seen.insert(id);
        }
    }
    assert_eq!(
        seen.len(),
        54,
        "vanilla advancement data uses 54 distinct triggers, found {seen:?}"
    );
}

/// `impossible` is the one trigger that must never fire, and vanilla uses it
/// to gate an advancement behind a command.
#[test]
fn the_impossible_trigger_carries_no_conditions() {
    init_vanilla_registry();

    let mut impossible = 0_usize;
    for advancement in REGISTRY.advancements.iter() {
        for criterion in advancement.criteria {
            if matches!(criterion.trigger, TriggerInstance::Impossible) {
                impossible += 1;
                assert!(criterion.trigger.player().is_empty());
            }
        }
    }
    assert_eq!(
        impossible, 1,
        "vanilla has exactly one impossible criterion"
    );
}
