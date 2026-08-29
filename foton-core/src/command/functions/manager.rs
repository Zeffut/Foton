//! Load, hold and replace the server's command functions.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use foton_utils::{Identifier, locks::SyncRwLock};
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::CommandDispatcher;
use super::super::execution::CommandSource;
use super::library::{CommandFunction, FunctionLibrary};
use super::loader::{self, TagEntry};
use super::parser::parse_function;

/// The tag whose functions run once after every load.
pub(crate) const LOAD_FUNCTION_TAG: Identifier = Identifier::vanilla_static("load");
/// The tag whose functions run at the start of every normally-running tick.
pub(crate) const TICK_FUNCTION_TAG: Identifier = Identifier::vanilla_static("tick");

/// What one datapack load produced, for the log line and for `/reload`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FunctionReloadReport {
    pub(crate) functions: usize,
    pub(crate) tags: usize,
    pub(crate) errors: Vec<String>,
}

/// The server's command functions and the tags that drive them.
///
/// Vanilla parity: `ServerFunctionManager` over a `ServerFunctionLibrary`. A
/// reload builds a whole new library and swaps it in, so a call that is already
/// running keeps the entries it started with.
pub(crate) struct FunctionManager {
    root: PathBuf,
    state: SyncRwLock<FunctionManagerState>,
}

struct FunctionManagerState {
    library: Arc<FunctionLibrary>,
    ticking: Vec<Arc<CommandFunction>>,
    post_reload: bool,
}

impl FunctionManager {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            state: SyncRwLock::new(FunctionManagerState {
                library: Arc::new(FunctionLibrary::default()),
                ticking: Vec::new(),
                post_reload: false,
            }),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Reads every datapack and replaces the loaded library with the result.
    pub(crate) fn reload(
        &self,
        dispatcher: &CommandDispatcher,
        compilation_source: &CommandSource,
    ) -> FunctionReloadReport {
        let contents = loader::collect(&self.root);
        let mut errors = contents.errors;

        let mut functions: FxHashMap<Identifier, Arc<CommandFunction>> = FxHashMap::default();
        for (id, source) in contents.functions {
            match parse_function(id.clone(), &source.text, dispatcher, compilation_source) {
                Ok(function) => {
                    functions.insert(id, Arc::new(function));
                }
                Err(error) => errors.push(format!(
                    "failed to load function {id} from datapack {}: {error}",
                    source.source_pack
                )),
            }
        }

        let tags = build_tags(&contents.tags, &functions, &mut errors);
        let library = Arc::new(FunctionLibrary::new(functions, tags));
        let report = FunctionReloadReport {
            functions: library.function_count(),
            tags: library.tag_count(),
            errors,
        };

        let ticking = library.tag(&TICK_FUNCTION_TAG).to_vec();
        let mut state = self.state.write();
        state.library = library;
        state.ticking = ticking;
        state.post_reload = true;
        report
    }

    /// Drops every loaded function.
    ///
    /// A parsed function holds the command source it was compiled with, and that
    /// source holds the server, so the loaded library keeps the server alive.
    /// Shutdown breaks that cycle here rather than leaking the whole server.
    pub(crate) fn unload(&self) {
        let mut state = self.state.write();
        state.library = Arc::new(FunctionLibrary::default());
        state.ticking.clear();
        state.post_reload = false;
    }

    pub(crate) fn library(&self) -> Arc<FunctionLibrary> {
        Arc::clone(&self.state.read().library)
    }

    /// Returns the `#minecraft:load` functions once per reload.
    pub(crate) fn take_load_functions(&self) -> Vec<Arc<CommandFunction>> {
        let mut state = self.state.write();
        if !state.post_reload {
            return Vec::new();
        }
        state.post_reload = false;
        state.library.tag(&LOAD_FUNCTION_TAG).to_vec()
    }

    /// Returns the `#minecraft:tick` functions cached by the last reload.
    pub(crate) fn ticking_functions(&self) -> Vec<Arc<CommandFunction>> {
        self.state.read().ticking.clone()
    }
}

/// Resolves raw tag entries into function lists, in dependency order.
///
/// Vanilla parity: `TagLoader.build`. A tag with an unresolved required entry is
/// dropped with an error instead of becoming an empty tag, so a typo in a tag
/// file cannot silently turn `#minecraft:tick` into "run nothing".
pub(super) fn build_tags(
    raw: &FxHashMap<Identifier, Vec<TagEntry>>,
    functions: &FxHashMap<Identifier, Arc<CommandFunction>>,
    errors: &mut Vec<String>,
) -> FxHashMap<Identifier, Arc<[Arc<CommandFunction>]>> {
    let mut built: FxHashMap<Identifier, Arc<[Arc<CommandFunction>]>> = FxHashMap::default();
    let mut failed: FxHashSet<Identifier> = FxHashSet::default();
    let mut visiting: Vec<Identifier> = Vec::new();
    for id in raw.keys() {
        resolve_tag(
            id,
            raw,
            functions,
            &mut built,
            &mut failed,
            &mut visiting,
            errors,
        );
    }
    built
}

fn resolve_tag(
    id: &Identifier,
    raw: &FxHashMap<Identifier, Vec<TagEntry>>,
    functions: &FxHashMap<Identifier, Arc<CommandFunction>>,
    built: &mut FxHashMap<Identifier, Arc<[Arc<CommandFunction>]>>,
    failed: &mut FxHashSet<Identifier>,
    visiting: &mut Vec<Identifier>,
    errors: &mut Vec<String>,
) -> Option<Arc<[Arc<CommandFunction>]>> {
    if let Some(tag) = built.get(id) {
        return Some(Arc::clone(tag));
    }
    if failed.contains(id) {
        return None;
    }
    let entries = raw.get(id)?;
    if visiting.contains(id) {
        errors.push(format!(
            "couldn't load function tag {id}: it is part of a reference cycle"
        ));
        failed.insert(id.clone());
        return None;
    }

    visiting.push(id.clone());
    let mut values: Vec<Arc<CommandFunction>> = Vec::new();
    let mut seen: FxHashSet<Identifier> = FxHashSet::default();
    let mut missing: Vec<String> = Vec::new();

    for entry in entries {
        if entry.references_tag {
            let referenced =
                resolve_tag(&entry.id, raw, functions, built, failed, visiting, errors);
            match referenced {
                Some(referenced) => {
                    for function in referenced.iter() {
                        if seen.insert(function.id().clone()) {
                            values.push(Arc::clone(function));
                        }
                    }
                }
                None if entry.required => {
                    missing.push(format!("#{} (from {})", entry.id, entry.source_pack));
                }
                None => {}
            }
            continue;
        }
        match functions.get(&entry.id) {
            Some(function) => {
                if seen.insert(function.id().clone()) {
                    values.push(Arc::clone(function));
                }
            }
            None if entry.required => {
                missing.push(format!("{} (from {})", entry.id, entry.source_pack));
            }
            None => {}
        }
    }
    visiting.pop();

    if !missing.is_empty() {
        errors.push(format!(
            "couldn't load function tag {id} as it is missing the following references: {}",
            missing.join(", ")
        ));
        failed.insert(id.clone());
        return None;
    }

    let tag: Arc<[Arc<CommandFunction>]> = values.into();
    built.insert(id.clone(), Arc::clone(&tag));
    Some(tag)
}
