//! One player's progress through the advancement tree.
//!
//! Vanilla parity: `PlayerAdvancements`. The parts that need a live player --
//! granting rewards, broadcasting the chat announcement, sending the packet --
//! are deliberately not here: this type answers what happened and the caller
//! acts on it, so no lock is ever held across a packet send.

use std::collections::BTreeSet;
use std::mem;

use steel_registry::advancement::progress::{AdvancementProgress, CriterionProgress};
use steel_registry::advancement::{Advancement, AdvancementRef};
use steel_utils::Identifier;

use super::TRIGGER_INDEX;
use super::tree::{ADVANCEMENT_TREE, AdvancementTree};
use super::visibility;

/// What awarding a criterion changed.
///
/// Vanilla parity: the two facts `PlayerAdvancements.award` acts on -- whether
/// the criterion moved at all, and whether that completed the advancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AwardOutcome {
    /// Whether the criterion went from unmet to met.
    pub granted: bool,
    /// Whether that was the criterion that finished the advancement.
    pub completed: bool,
}

/// Everything one `ClientboundUpdateAdvancementsPacket` needs to say.
#[derive(Debug, Default)]
pub struct AdvancementUpdate {
    /// Whether the client should throw its tree away first.
    pub reset: bool,
    /// Advancements that became visible.
    pub added: Vec<AdvancementRef>,
    /// Advancements that stopped being visible.
    pub removed: Vec<Identifier>,
    /// Progress for advancements the client can see.
    pub progress: Vec<(Identifier, AdvancementProgress)>,
}

impl AdvancementUpdate {
    /// Whether the update would tell the client nothing.
    ///
    /// Vanilla parity: the `!progress.isEmpty() || !added.isEmpty() ||
    /// !removed.isEmpty()` guard of `flushDirty`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.progress.is_empty()
    }
}

/// What the advancement screen was pointed at, when the selection moved.
///
/// Vanilla parity: the argument of the `ClientboundSelectAdvancementsTabPacket`
/// that `setSelectedTab` sends, which is nullable. The two cases are kept
/// apart from "the selection did not move", which sends nothing at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabSelection {
    /// No tab is selected any more.
    Cleared,
    /// The screen now shows the tab headed by this root advancement.
    Selected(Identifier),
}

/// One criterion of one advancement, by tree position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CriterionRef {
    /// The advancement's index in [`ADVANCEMENT_TREE`].
    pub node: usize,
    /// The criterion's index in that advancement's `criteria` slice.
    pub criterion: usize,
}

/// Everything Steel remembers about one player's advancements.
pub struct PlayerAdvancements {
    /// Progress per tree node, dense so a lookup is an index.
    ///
    /// Vanilla keeps a map and fills it lazily, but `registerListeners` walks
    /// every advancement on load and creates an entry for each, so the map is
    /// dense in practice too. Keeping it dense here means `flushDirty` always
    /// has an entry to send, exactly as vanilla's does.
    progress: Vec<AdvancementProgress>,
    visible: Vec<bool>,
    progress_changed: BTreeSet<usize>,
    roots_to_update: BTreeSet<usize>,
    selected_tab: Option<usize>,
    first_packet: bool,
}

impl PlayerAdvancements {
    /// A player who has earned nothing.
    #[must_use]
    pub fn new() -> Self {
        let tree = &*ADVANCEMENT_TREE;
        let mut progress = Vec::with_capacity(tree.len());
        for index in tree.indices() {
            let mut entry = AdvancementProgress::new();
            entry.update(tree.node(index).advancement.requirements);
            progress.push(entry);
        }
        Self {
            progress,
            visible: vec![false; tree.len()],
            progress_changed: BTreeSet::new(),
            roots_to_update: BTreeSet::new(),
            selected_tab: None,
            first_packet: true,
        }
    }

    /// Throws every bit of progress away and starts the client's tree over.
    ///
    /// Vanilla parity: `PlayerAdvancements.reload`, minus the datapack swap it
    /// exists for. Steel uses it where vanilla builds a fresh
    /// `PlayerAdvancements`: when a player's saved data is applied, and when
    /// they set foot in a domain for the first time.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Restores saved progress.
    ///
    /// Criteria the advancement no longer declares are dropped, which is what
    /// vanilla's `AdvancementProgress.update` does on load. An advancement that
    /// no longer exists is skipped with a warning rather than failing the load.
    ///
    /// Vanilla parity: `PlayerAdvancements.applyFrom`.
    pub fn load(&mut self, saved: impl IntoIterator<Item = (Identifier, Vec<(String, i64)>)>) {
        let tree = &*ADVANCEMENT_TREE;
        for (key, criteria) in saved {
            let Some(node) = tree.index_of(&key) else {
                log::warn!("ignoring saved progress for the unknown advancement {key}");
                continue;
            };
            let advancement = tree.node(node).advancement;
            let mut restored = false;
            for (name, obtained) in criteria {
                // The name has to be matched against the generated criteria so
                // the stored progress borrows a name the advancement still has.
                let Some(criterion) = advancement.criterion(&name) else {
                    log::warn!("ignoring saved progress for {key}'s unknown criterion {name}");
                    continue;
                };
                if self.progress[node].grant(criterion.name, obtained) {
                    restored = true;
                }
            }
            if restored {
                self.progress_changed.insert(node);
                self.roots_to_update.insert(tree.node(node).root);
            }
        }
    }

    /// Every advancement with at least one met criterion, for saving.
    ///
    /// Vanilla parity: `PlayerAdvancements.asData`, which keeps only entries
    /// that `hasProgress`.
    #[must_use]
    pub fn save_data(&self) -> Vec<(Identifier, Vec<(&'static str, i64)>)> {
        let tree = &*ADVANCEMENT_TREE;
        let mut out = Vec::new();
        for node in tree.indices() {
            let progress = &self.progress[node];
            if !progress.has_progress() {
                continue;
            }
            let criteria: Vec<(&'static str, i64)> = progress
                .criteria()
                .filter_map(|(name, entry)| entry.obtained().map(|obtained| (name, obtained)))
                .collect();
            out.push((tree.node(node).advancement.key.clone(), criteria));
        }
        out
    }

    /// Whether one advancement is complete.
    #[must_use]
    pub fn is_done(&self, node: usize) -> bool {
        self.progress[node].is_done()
    }

    /// The progress of one advancement.
    #[must_use]
    pub fn progress(&self, node: usize) -> &AdvancementProgress {
        &self.progress[node]
    }

    /// Every criterion of `trigger_id` this player could still be awarded.
    ///
    /// Vanilla parity: the listener set `PlayerAdvancements.registerListeners`
    /// and `unregisterListeners` keep in `activeTriggers`. A criterion listens
    /// while both it and its advancement are unfinished, which is why a
    /// finished advancement never picks up extra progress on a second
    /// criterion of an `any_of` requirement.
    #[must_use]
    pub fn pending(&self, trigger_id: &str) -> Vec<CriterionRef> {
        TRIGGER_INDEX
            .criteria_for(trigger_id)
            .iter()
            .copied()
            .filter(|&reference| {
                !self.is_done(reference.node) && !self.is_criterion_done(reference)
            })
            .collect()
    }

    /// Whether one criterion has been met.
    #[must_use]
    pub fn is_criterion_done(&self, criterion: CriterionRef) -> bool {
        let advancement = ADVANCEMENT_TREE.node(criterion.node).advancement;
        self.progress[criterion.node]
            .is_criterion_done(advancement.criteria[criterion.criterion].name)
    }

    /// Marks a criterion met.
    ///
    /// Vanilla parity: `PlayerAdvancements.award`, minus the reward grant and
    /// the chat announcement, which the caller does from the returned outcome.
    pub fn award(&mut self, node: usize, criterion: &str, epoch_millis: i64) -> AwardOutcome {
        let was_done = self.progress[node].is_done();
        if !self.progress[node].grant(criterion, epoch_millis) {
            return AwardOutcome::default();
        }

        self.progress_changed.insert(node);
        let completed = !was_done && self.progress[node].is_done();
        if completed {
            self.roots_to_update
                .insert(ADVANCEMENT_TREE.node(node).root);
        }
        AwardOutcome {
            granted: true,
            completed,
        }
    }

    /// Marks a criterion unmet.
    ///
    /// Vanilla parity: `PlayerAdvancements.revoke`.
    pub fn revoke(&mut self, node: usize, criterion: &str) -> bool {
        let was_done = self.progress[node].is_done();
        if !self.progress[node].revoke(criterion) {
            return false;
        }

        self.progress_changed.insert(node);
        if was_done && !self.progress[node].is_done() {
            self.roots_to_update
                .insert(ADVANCEMENT_TREE.node(node).root);
        }
        true
    }

    /// Builds the next update for the client, if there is one.
    ///
    /// Vanilla parity: `PlayerAdvancements.flushDirty`, including its ordering:
    /// visibility is recomputed first, and the advancements it newly reveals
    /// have their progress picked up in the same flush.
    pub fn flush_dirty(&mut self) -> Option<AdvancementUpdate> {
        if !self.first_packet && self.roots_to_update.is_empty() && self.progress_changed.is_empty()
        {
            return None;
        }

        let tree = &*ADVANCEMENT_TREE;
        let mut update = AdvancementUpdate {
            reset: self.first_packet,
            ..AdvancementUpdate::default()
        };

        let roots: Vec<usize> = mem::take(&mut self.roots_to_update).into_iter().collect();
        for root in roots {
            self.update_tree_visibility(tree, root, &mut update);
        }

        let changed: Vec<usize> = mem::take(&mut self.progress_changed).into_iter().collect();
        for node in changed {
            if self.visible[node] {
                update.progress.push((
                    tree.node(node).advancement.key.clone(),
                    self.progress[node].clone(),
                ));
            }
        }

        // Vanilla clears `isFirstPacket` whether or not anything was sent.
        self.first_packet = false;
        if update.is_empty() {
            return None;
        }
        Some(update)
    }

    fn update_tree_visibility(
        &mut self,
        tree: &AdvancementTree,
        root: usize,
        update: &mut AdvancementUpdate,
    ) {
        let progress = &self.progress;
        let visible = &mut self.visible;
        let progress_changed = &mut self.progress_changed;

        visibility::evaluate(
            tree,
            root,
            &mut |node| progress[node].is_done(),
            &mut |node, should_be_visible| {
                if should_be_visible {
                    if !visible[node] {
                        visible[node] = true;
                        update.added.push(tree.node(node).advancement);
                        // The dense progress map always has an entry, so this
                        // is vanilla's `progress.containsKey` branch always
                        // being taken.
                        progress_changed.insert(node);
                    }
                } else if visible[node] {
                    visible[node] = false;
                    update.removed.push(tree.node(node).advancement.key.clone());
                }
            },
        );
    }

    /// Whether the client can currently see an advancement.
    #[must_use]
    pub fn is_visible(&self, node: usize) -> bool {
        self.visible[node]
    }

    /// Points the client's advancement screen at a tab.
    ///
    /// Returns the tab to echo back when the selection actually changed.
    ///
    /// Vanilla parity: `PlayerAdvancements.setSelectedTab`, which clears the
    /// selection for anything that is not a drawn root and only sends a packet
    /// when the value moved.
    pub fn set_selected_tab(&mut self, node: Option<usize>) -> Option<TabSelection> {
        let tree = &*ADVANCEMENT_TREE;
        let accepted = node.filter(|&node| {
            let advancement: &Advancement = tree.node(node).advancement;
            advancement.is_root() && advancement.display.is_some()
        });
        if accepted == self.selected_tab {
            return None;
        }
        self.selected_tab = accepted;
        Some(accepted.map_or(TabSelection::Cleared, |node| {
            TabSelection::Selected(tree.node(node).advancement.key.clone())
        }))
    }

    /// The tab the client last opened.
    #[must_use]
    pub const fn selected_tab(&self) -> Option<usize> {
        self.selected_tab
    }

    /// Whether the next flush will be the first packet this player sees.
    #[must_use]
    pub const fn is_first_packet(&self) -> bool {
        self.first_packet
    }

    /// Marks every criterion of one advancement met.
    ///
    /// Vanilla parity: what `/advancement grant <target> only <advancement>`
    /// does through `award` once per criterion.
    pub fn award_all(&mut self, node: usize, epoch_millis: i64) -> AwardOutcome {
        let advancement = ADVANCEMENT_TREE.node(node).advancement;
        let mut outcome = AwardOutcome::default();
        for criterion in advancement.criteria {
            let step = self.award(node, criterion.name, epoch_millis);
            outcome.granted |= step.granted;
            outcome.completed |= step.completed;
        }
        outcome
    }

    /// Marks every criterion of one advancement unmet.
    pub fn revoke_all(&mut self, node: usize) -> bool {
        let advancement = ADVANCEMENT_TREE.node(node).advancement;
        let mut changed = false;
        for criterion in advancement.criteria {
            changed |= self.revoke(node, criterion.name);
        }
        changed
    }
}

impl Default for PlayerAdvancements {
    fn default() -> Self {
        Self::new()
    }
}

/// The progress of one criterion, for callers that only need the timestamp.
#[must_use]
pub fn criterion_progress(
    advancements: &PlayerAdvancements,
    node: usize,
    criterion: &str,
) -> Option<CriterionProgress> {
    advancements.progress(node).criterion(criterion).copied()
}
