//! Firing a criterion's trigger for one player.
//!
//! Vanilla parity: `CriteriaTriggers` and `SimpleCriterionTrigger.trigger`.
//! Vanilla keeps a per-player listener map and adds or removes entries as
//! criteria are met; Foton's advancement set never changes, so the criteria a
//! trigger *could* award are indexed once for the whole server in
//! [`super::TRIGGER_INDEX`], and the player narrows that to the ones still
//! listening.
//!
//! A trigger nothing calls awards nothing, which is what vanilla does before
//! the trigger is invoked -- it is not the same as a criterion that always
//! passes. The vanilla call sites Foton has not reached are marked with a
//! `// Not implemented:` comment naming the trigger rather than granting it.

pub mod entity;
pub mod inventory;
pub mod item;
pub mod world;

use foton_registry::advancement::TriggerInstance;

use super::predicate::{PredicateContext, Subject};
use super::tree::ADVANCEMENT_TREE;
use crate::entity::Entity as _;
use crate::player::Player;

/// Awards every criterion of `trigger_id` whose own conditions `matches`
/// accepts and whose `player` predicate this player satisfies.
///
/// Vanilla parity: `SimpleCriterionTrigger.trigger`, order included -- the
/// trigger's conditions are tested first, the `player` predicate second, and
/// nothing is awarded until every candidate has been tested.
///
/// The candidates are taken while the progress lock is held and the predicates
/// are evaluated after it is dropped: a predicate reads the world and the
/// player's equipment, and an award takes the lock again.
pub fn fire(
    player: &Player,
    trigger_id: &'static str,
    mut matches: impl FnMut(&'static TriggerInstance) -> bool,
) {
    let pending = player.pending_advancement_criteria(trigger_id);
    if pending.is_empty() {
        return;
    }

    // Vanilla: `EntityPredicate.createContext(player, player)` -- the player as
    // both `THIS_ENTITY` and the source of `ORIGIN`.
    let context = PredicateContext {
        player,
        origin: player.position(),
        subject: Subject::Player,
        block_state: None,
        tool: None,
    };

    let mut awarded = Vec::new();
    for reference in pending {
        let advancement = ADVANCEMENT_TREE.node(reference.node).advancement;
        let criterion = &advancement.criteria[reference.criterion];
        if matches(&criterion.trigger) && context.matches_conditions(criterion.trigger.player()) {
            awarded.push((reference.node, criterion.name));
        }
    }

    for (node, criterion) in awarded {
        player.award_advancement_criterion(node, criterion);
    }
}
