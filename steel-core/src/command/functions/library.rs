//! The loaded set of command functions and function tags.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use simdnbt::owned::NbtCompound;
use steel_utils::Identifier;
use text_components::TextComponent;

use super::super::brigadier::CommandDispatcher;
use super::super::execution::{CommandSource, FunctionEntries, SteelCommandRuntime};
use super::parser::FunctionBody;

/// One loaded `.mcfunction`.
///
/// Vanilla parity: `CommandFunction`. A file with no macro lines is already
/// runnable; one with macro lines only becomes runnable once a call supplies
/// its arguments, which is what [`Self::instantiate`] is for.
pub(crate) struct CommandFunction {
    id: Identifier,
    body: FunctionBody<CommandSource>,
}

impl CommandFunction {
    pub(crate) const fn new(id: Identifier, body: FunctionBody<CommandSource>) -> Self {
        Self { id, body }
    }

    pub(crate) const fn id(&self) -> &Identifier {
        &self.id
    }

    /// Returns the command actions this call should run, in file order.
    ///
    /// Vanilla parity: `CommandFunction.instantiate`. The error is the reason a
    /// macro could not be filled in, which the caller wraps in the command
    /// error its own message uses.
    pub(crate) fn instantiate(
        &self,
        arguments: Option<&NbtCompound>,
        dispatcher: &CommandDispatcher<CommandSource, SteelCommandRuntime>,
    ) -> Result<FunctionEntries<CommandSource>, Box<TextComponent>> {
        match &self.body {
            FunctionBody::Plain(entries) => Ok(Arc::clone(entries)),
            FunctionBody::Macro(function) => function.instantiate(&self.id, arguments, dispatcher),
        }
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
