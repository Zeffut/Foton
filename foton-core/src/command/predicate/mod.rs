//! Datapack predicates: `data/<namespace>/predicate/*.json` and the
//! `execute if|unless predicate` test that runs them.
//!
//! Vanilla parity: `LootDataType.PREDICATE` holds `LootItemCondition`s, and
//! `ExecuteCommand.checkCustomPredicate` evaluates one against a `LootContext`
//! built from the command source: `ORIGIN` is its position and `THIS_ENTITY`
//! its entity, if any. Foton evaluates them with the advancement predicate
//! engine, which already answers the same entity, location and item questions
//! from a live world; see [`crate::advancement::predicate`].

mod json_nbt;
mod parse;

#[cfg(test)]
mod tests;

use std::sync::{Arc, LazyLock};

use foton_registry::advancement::predicate::ContextAwarePredicate;
use foton_utils::{Identifier, locks::SyncMutex};
use rustc_hash::FxHashMap;

use super::execution::CommandSource;
use crate::advancement::predicate::{PredicateContext, Subject};

/// Every predicate one datapack load produced.
#[derive(Default)]
pub(crate) struct PredicateLibrary {
    predicates: FxHashMap<Identifier, ContextAwarePredicate>,
}

impl PredicateLibrary {
    pub(crate) const fn new(predicates: FxHashMap<Identifier, ContextAwarePredicate>) -> Self {
        Self { predicates }
    }

    pub(crate) fn get(&self, id: &Identifier) -> Option<ContextAwarePredicate> {
        self.predicates.get(id).copied()
    }

    pub(crate) fn names(&self) -> impl Iterator<Item = &Identifier> {
        self.predicates.keys()
    }

    pub(crate) fn len(&self) -> usize {
        self.predicates.len()
    }
}

/// Parsed predicates by their source text.
///
/// The predicate model is `&'static` data, so a parsed file is leaked. Keying
/// by text means a `/reload` of files that did not change reuses the first
/// parse, and only an edited file costs new memory.
static PARSED: LazyLock<SyncMutex<FxHashMap<String, ContextAwarePredicate>>> =
    LazyLock::new(|| SyncMutex::new(FxHashMap::default()));

/// Reads one predicate file.
pub(crate) fn load_predicate(text: &str) -> Result<ContextAwarePredicate, String> {
    if let Some(parsed) = PARSED.lock().get(text) {
        return Ok(parsed);
    }
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("invalid JSON: {error}"))?;
    let parsed = parse::parse_predicate(&json)?;
    PARSED.lock().insert(text.to_owned(), parsed);
    Ok(parsed)
}

/// Vanilla parity: `ExecuteCommand.checkCustomPredicate`.
pub(crate) fn test_predicate(source: &CommandSource, predicate: ContextAwarePredicate) -> bool {
    let entity: Option<Arc<_>> = source.entity().cloned();
    let context = PredicateContext {
        level: Some(Arc::clone(source.world())),
        origin: source.position(),
        subject: entity.as_deref().map_or(Subject::None, Subject::Entity),
        block_state: None,
        tool: None,
    };
    context.matches_conditions(predicate)
}
