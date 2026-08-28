//! The loaded set of command functions and function tags.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use steel_utils::Identifier;

use super::super::execution::{CommandSource, FunctionEntries};

/// One loaded `.mcfunction`, already compiled into one action per command line.
///
/// Vanilla parity: `PlainTextFunction`, which is both the stored
/// `CommandFunction` and the `InstantiatedFunction` it hands to a call.
pub(crate) struct CommandFunction {
    id: Identifier,
    entries: FunctionEntries<CommandSource>,
}

impl CommandFunction {
    pub(crate) const fn new(id: Identifier, entries: FunctionEntries<CommandSource>) -> Self {
        Self { id, entries }
    }

    pub(crate) const fn id(&self) -> &Identifier {
        &self.id
    }

    /// Returns the shared command actions this function runs, in file order.
    pub(crate) fn entries(&self) -> FunctionEntries<CommandSource> {
        Arc::clone(&self.entries)
    }
}

/// Every function and function tag one datapack load produced.
///
/// Vanilla parity: `ServerFunctionLibrary`. A reload builds a whole new library
/// and swaps it in, so a running function keeps the entries it started with.
#[derive(Default)]
pub(crate) struct FunctionLibrary {
    functions: FxHashMap<Identifier, Arc<CommandFunction>>,
    tags: FxHashMap<Identifier, Arc<[Arc<CommandFunction>]>>,
}

impl FunctionLibrary {
    pub(crate) const fn new(
        functions: FxHashMap<Identifier, Arc<CommandFunction>>,
        tags: FxHashMap<Identifier, Arc<[Arc<CommandFunction>]>>,
    ) -> Self {
        Self { functions, tags }
    }

    pub(crate) fn function(&self, id: &Identifier) -> Option<&Arc<CommandFunction>> {
        self.functions.get(id)
    }

    /// Returns a tag's functions, or an empty slice when the tag does not exist.
    ///
    /// A tag whose entries could not all be resolved is absent rather than
    /// empty, matching `TagLoader.build`: an unresolvable tag is dropped with an
    /// error instead of silently becoming a tag that runs nothing.
    pub(crate) fn tag(&self, id: &Identifier) -> &[Arc<CommandFunction>] {
        self.tags.get(id).map_or(&[], |tag| &**tag)
    }

    pub(crate) fn function_names(&self) -> impl Iterator<Item = &Identifier> {
        self.functions.keys()
    }

    pub(crate) fn tag_names(&self) -> impl Iterator<Item = &Identifier> {
        self.tags.keys()
    }

    pub(crate) fn function_count(&self) -> usize {
        self.functions.len()
    }

    pub(crate) fn tag_count(&self) -> usize {
        self.tags.len()
    }
}
