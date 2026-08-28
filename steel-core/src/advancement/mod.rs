//! The advancement engine: the tree, per-player progress, and the triggers.
//!
//! Vanilla parity: `net.minecraft.advancements` plus `PlayerAdvancements` and
//! `CriteriaTriggers`. The definitions themselves are generated into
//! `steel_registry::advancement`; this is the part that needs a world.

pub mod player_advancements;
pub mod predicate;
pub mod tree;

pub mod visibility;

use std::sync::LazyLock;

use rustc_hash::FxHashMap;

pub use player_advancements::{
    AdvancementUpdate, AwardOutcome, CriterionRef, PlayerAdvancements, TabSelection,
};
pub use tree::{ADVANCEMENT_TREE, AdvancementNode, AdvancementTree};

/// Every criterion in the tree, grouped by the trigger that can award it.
///
/// Vanilla keeps this per player, adding and removing listeners as criteria are
/// met (`PlayerAdvancements.activeTriggers`). Steel's advancement set never
/// changes, so the index is built once for the whole server and a firing
/// trigger skips the criteria this player has already met -- which is the same
/// set vanilla's bookkeeping would have left registered.
pub struct TriggerIndex {
    by_trigger: FxHashMap<&'static str, Vec<CriterionRef>>,
}

impl TriggerIndex {
    fn build() -> Self {
        let mut by_trigger: FxHashMap<&'static str, Vec<CriterionRef>> = FxHashMap::default();
        for node in ADVANCEMENT_TREE.indices() {
            let advancement = ADVANCEMENT_TREE.node(node).advancement;
            for (criterion, entry) in advancement.criteria.iter().enumerate() {
                by_trigger
                    .entry(entry.trigger.trigger_id())
                    .or_default()
                    .push(CriterionRef { node, criterion });
            }
        }
        Self { by_trigger }
    }

    /// Every criterion one trigger can award.
    #[must_use]
    pub fn criteria_for(&self, trigger_id: &str) -> &[CriterionRef] {
        self.by_trigger
            .get(trigger_id)
            .map_or(&[], |criteria| criteria.as_slice())
    }
}

/// The server-wide criterion index.
pub static TRIGGER_INDEX: LazyLock<TriggerIndex> = LazyLock::new(TriggerIndex::build);

#[cfg(test)]
mod tests;
