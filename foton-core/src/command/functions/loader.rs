//! Datapack discovery and raw resource reading.
//!
//! Vanilla parity: the `FolderRepositorySource` view of `<level>/datapacks`
//! filtered through `FileToIdConverter`. Foton keeps the datapack directory
//! beside the save root because its function library is server-wide, the way
//! vanilla's is, while Foton's worlds each have their own save directory.

use std::{io, path::Path};

use foton_utils::Identifier;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use super::pack_resources::{PackResources, Resource, read_sorted_entries};

/// The file extension a function resource must have.
const FUNCTION_EXTENSION: &str = "mcfunction";
/// `Registries.elementsDirPath(function)`.
const FUNCTION_DIRECTORY: &str = "function";
/// `Registries.tagsDirPath(function)`.
const FUNCTION_TAG_DIRECTORY: [&str; 2] = ["tags", "function"];
/// `Registries.elementsDirPath(predicate)`.
const PREDICATE_DIRECTORY: &str = "predicate";
/// `Registries.elementsDirPath(item_modifier)`: the loot functions `/item modify` runs.
const ITEM_MODIFIER_DIRECTORY: &str = "item_modifier";

/// One entry of a function tag file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TagEntry {
    /// The referenced function, or the referenced tag when `references_tag`.
    pub(super) id: Identifier,
    pub(super) references_tag: bool,
    pub(super) required: bool,
    /// The pack the entry came from, used only for load error messages.
    pub(super) source_pack: String,
}

/// Everything a datapack scan found, already merged across packs.
#[derive(Debug, Default)]
pub(super) struct DatapackContents {
    /// Datapacks that were discovered and accepted for this load, in pack order.
    pub(super) packs: Vec<DatapackInfo>,
    /// Function sources keyed by id. A later pack replaces an earlier one.
    pub(super) functions: FxHashMap<Identifier, FunctionSource>,
    /// Tag entries keyed by tag id, accumulated in pack order.
    pub(super) tags: FxHashMap<Identifier, Vec<TagEntry>>,
    /// Predicate JSON keyed by id. A later pack replaces an earlier one.
    pub(super) predicates: FxHashMap<Identifier, PredicateSource>,
    /// Item modifier JSON keyed by id. A later pack replaces an earlier one.
    pub(super) item_modifiers: FxHashMap<Identifier, FunctionSource>,
    /// Files that could not be read or understood, reported by the caller.
    pub(super) errors: Vec<String>,
}

/// A datapack directory or archive accepted by the active resource loader.
///
/// A pack is reported as compatible only after it has passed the same
/// directory/resource scan used by the function loader. Foton currently has
/// no separate pack-format validator, so accepted packs are the authoritative
/// compatible set rather than an invented version claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DatapackInfo {
    pub(super) name: String,
    pub(super) compatibility: &'static str,
    pub(super) enabled: bool,
}

#[derive(Debug)]
pub(super) struct FunctionSource {
    pub(super) text: String,
    pub(super) source_pack: String,
}

#[derive(Debug)]
pub(super) struct PredicateSource {
    pub(super) text: String,
    pub(super) source_pack: String,
}

#[derive(Deserialize)]
struct RawTagFile {
    #[serde(default)]
    replace: bool,
    values: Vec<RawTagEntry>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawTagEntry {
    Plain(String),
    Detailed {
        id: String,
        #[serde(default = "default_required")]
        required: bool,
    },
}

const fn default_required() -> bool {
    true
}

/// Reads every enabled datapack under `root`: folders with a `data/` directory
/// and `.zip` archives with `pack.mcmeta` and `data/` at their root.
///
/// A missing root is not an error: a server with no datapacks simply has no
/// functions, exactly as an empty `<level>/datapacks` does in vanilla. An
/// archive that cannot be used is skipped with an error naming the file.
pub(super) fn collect(root: &Path) -> DatapackContents {
    let mut contents = DatapackContents::default();
    let entries = match read_sorted_entries(root) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                contents
                    .errors
                    .push(format!("could not read {}: {error}", root.display()));
            }
            return contents;
        }
    };

    for entry in entries {
        let Some(pack_name) = entry
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        let resources = if entry.is_dir() {
            PackResources::directory(&entry)
        } else if entry.is_file()
            && entry
                .extension()
                .is_some_and(|extension| extension == "zip")
        {
            match PackResources::archive(&entry) {
                Ok(resources) => {
                    log::info!("Reading datapack archive {pack_name}");
                    Some(resources)
                }
                Err(reason) => {
                    contents
                        .errors
                        .push(format!("skipped datapack archive {pack_name}: {reason}"));
                    None
                }
            }
        } else {
            None
        };
        let Some(mut resources) = resources else {
            continue;
        };
        contents.packs.push(DatapackInfo {
            name: pack_name.clone(),
            compatibility: "COMPATIBLE",
            enabled: true,
        });
        collect_pack(&mut resources, &pack_name, &mut contents);
    }

    contents
}

fn collect_pack(resources: &mut PackResources, pack_name: &str, contents: &mut DatapackContents) {
    let namespaces = match resources.namespaces() {
        Ok(namespaces) => namespaces,
        Err(error) => {
            contents
                .errors
                .push(format!("could not read data/ of {pack_name}: {error}"));
            return;
        }
    };
    for namespace in namespaces {
        if !Identifier::validate_namespace(&namespace) {
            contents.errors.push(format!(
                "datapack {pack_name} has an invalid namespace directory '{namespace}'"
            ));
            continue;
        }
        collect_functions(resources, &namespace, pack_name, contents);
        collect_tags(resources, &namespace, pack_name, contents);
        collect_predicates(resources, &namespace, pack_name, contents);
        collect_item_modifiers(resources, &namespace, pack_name, contents);
    }
}

fn collect_functions(
    resources: &mut PackResources,
    namespace: &str,
    pack_name: &str,
    contents: &mut DatapackContents,
) {
    let listed = resources.resources(
        namespace,
        &[FUNCTION_DIRECTORY],
        FUNCTION_EXTENSION,
        &mut contents.errors,
    );
    for resource in listed {
        match resources.read(&resource) {
            Ok(text) => {
                contents.functions.insert(
                    resource.id,
                    FunctionSource {
                        text,
                        source_pack: pack_name.to_owned(),
                    },
                );
            }
            Err(error) => contents
                .errors
                .push(read_error(pack_name, &resource, &error)),
        }
    }
}

fn collect_predicates(
    resources: &mut PackResources,
    namespace: &str,
    pack_name: &str,
    contents: &mut DatapackContents,
) {
    let listed = resources.resources(
        namespace,
        &[PREDICATE_DIRECTORY],
        "json",
        &mut contents.errors,
    );
    for resource in listed {
        match resources.read(&resource) {
            Ok(text) => {
                contents.predicates.insert(
                    resource.id,
                    PredicateSource {
                        text,
                        source_pack: pack_name.to_owned(),
                    },
                );
            }
            Err(error) => contents
                .errors
                .push(read_error(pack_name, &resource, &error)),
        }
    }
}

fn collect_item_modifiers(
    resources: &mut PackResources,
    namespace: &str,
    pack_name: &str,
    contents: &mut DatapackContents,
) {
    let listed = resources.resources(
        namespace,
        &[ITEM_MODIFIER_DIRECTORY],
        "json",
        &mut contents.errors,
    );
    for resource in listed {
        match resources.read(&resource) {
            Ok(text) => {
                contents.item_modifiers.insert(
                    resource.id,
                    FunctionSource {
                        text,
                        source_pack: pack_name.to_owned(),
                    },
                );
            }
            Err(error) => contents
                .errors
                .push(read_error(pack_name, &resource, &error)),
        }
    }
}

fn collect_tags(
    resources: &mut PackResources,
    namespace: &str,
    pack_name: &str,
    contents: &mut DatapackContents,
) {
    let listed = resources.resources(
        namespace,
        &FUNCTION_TAG_DIRECTORY,
        "json",
        &mut contents.errors,
    );
    for resource in listed {
        let id = resource.id.clone();
        let text = match resources.read(&resource) {
            Ok(text) => text,
            Err(error) => {
                contents
                    .errors
                    .push(read_error(pack_name, &resource, &error));
                continue;
            }
        };
        let parsed: RawTagFile = match serde_json::from_str(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                contents.errors.push(format!(
                    "could not read function tag {id} from {pack_name}:{}: {error}",
                    resource.location
                ));
                continue;
            }
        };
        let entries = contents.tags.entry(id.clone()).or_default();
        if parsed.replace {
            entries.clear();
        }
        for value in parsed.values {
            let (raw, required) = match value {
                RawTagEntry::Plain(id) => (id, true),
                RawTagEntry::Detailed { id, required } => (id, required),
            };
            let references_tag = raw.starts_with('#');
            let raw_id = if references_tag { &raw[1..] } else { &raw[..] };
            match raw_id.parse::<Identifier>() {
                Ok(entry_id) => entries.push(TagEntry {
                    id: entry_id,
                    references_tag,
                    required,
                    source_pack: pack_name.to_owned(),
                }),
                Err(error) => contents.errors.push(format!(
                    "function tag {id} in {pack_name} has an invalid entry '{raw}': {error}"
                )),
            }
        }
    }
}

fn read_error(pack_name: &str, resource: &Resource, error: &io::Error) -> String {
    format!(
        "could not read {} from datapack {pack_name}: {error}",
        resource.location
    )
}
