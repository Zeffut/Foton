//! Datapack discovery and raw resource reading.
//!
//! Vanilla parity: the `FolderRepositorySource` view of `<level>/datapacks`
//! filtered through `FileToIdConverter`. Steel keeps the datapack directory
//! beside the save root because its function library is server-wide, the way
//! vanilla's is, while Steel's worlds each have their own save directory.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use rustc_hash::FxHashMap;
use serde::Deserialize;
use steel_utils::Identifier;

/// The file extension a function resource must have.
const FUNCTION_EXTENSION: &str = "mcfunction";
/// `Registries.elementsDirPath(function)`.
const FUNCTION_DIRECTORY: &str = "function";
/// `Registries.tagsDirPath(function)`.
const FUNCTION_TAG_DIRECTORY: [&str; 2] = ["tags", "function"];

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
    /// Function sources keyed by id. A later pack replaces an earlier one.
    pub(super) functions: FxHashMap<Identifier, FunctionSource>,
    /// Tag entries keyed by tag id, accumulated in pack order.
    pub(super) tags: FxHashMap<Identifier, Vec<TagEntry>>,
    /// Files that could not be read or understood, reported by the caller.
    pub(super) errors: Vec<String>,
}

#[derive(Debug)]
pub(super) struct FunctionSource {
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

/// Reads every enabled datapack under `root`.
///
/// A missing root is not an error: a server with no datapacks simply has no
/// functions, exactly as an empty `<level>/datapacks` does in vanilla.
pub(super) fn collect(root: &Path) -> DatapackContents {
    let mut contents = DatapackContents::default();
    let packs = match pack_directories(root) {
        Ok(packs) => packs,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                contents
                    .errors
                    .push(format!("could not read {}: {error}", root.display()));
            }
            return contents;
        }
    };

    for pack in packs {
        let Some(pack_name) = pack
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        let data = pack.join("data");
        if !data.is_dir() {
            continue;
        }
        let namespaces = match read_sorted_directories(&data) {
            Ok(namespaces) => namespaces,
            Err(error) => {
                contents
                    .errors
                    .push(format!("could not read {}: {error}", data.display()));
                continue;
            }
        };
        for namespace_dir in namespaces {
            let Some(namespace) = namespace_dir.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !Identifier::validate_namespace(namespace) {
                contents.errors.push(format!(
                    "datapack {pack_name} has an invalid namespace directory '{namespace}'"
                ));
                continue;
            }
            collect_functions(
                &namespace_dir.join(FUNCTION_DIRECTORY),
                namespace,
                &pack_name,
                &mut contents,
            );
            let mut tag_dir = namespace_dir.clone();
            for part in FUNCTION_TAG_DIRECTORY {
                tag_dir.push(part);
            }
            collect_tags(&tag_dir, namespace, &pack_name, &mut contents);
        }
    }

    contents
}

fn collect_functions(
    directory: &Path,
    namespace: &str,
    pack_name: &str,
    contents: &mut DatapackContents,
) {
    for (id, path) in resource_files(directory, namespace, FUNCTION_EXTENSION, contents) {
        match fs::read_to_string(&path) {
            Ok(text) => {
                contents.functions.insert(
                    id,
                    FunctionSource {
                        text,
                        source_pack: pack_name.to_owned(),
                    },
                );
            }
            Err(error) => contents
                .errors
                .push(format!("could not read {}: {error}", path.display())),
        }
    }
}

fn collect_tags(
    directory: &Path,
    namespace: &str,
    pack_name: &str,
    contents: &mut DatapackContents,
) {
    for (id, path) in resource_files(directory, namespace, "json", contents) {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                contents
                    .errors
                    .push(format!("could not read {}: {error}", path.display()));
                continue;
            }
        };
        let parsed: RawTagFile = match serde_json::from_str(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                contents.errors.push(format!(
                    "could not read function tag {id} from {}: {error}",
                    path.display()
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

/// Lists a resource directory's files with the wanted extension, deepest last.
fn resource_files(
    directory: &Path,
    namespace: &str,
    extension: &str,
    contents: &mut DatapackContents,
) -> Vec<(Identifier, PathBuf)> {
    let mut found = Vec::new();
    if !directory.is_dir() {
        return found;
    }
    let mut pending = vec![(directory.to_path_buf(), String::new())];
    while let Some((current, prefix)) = pending.pop() {
        let entries = match read_sorted_entries(&current) {
            Ok(entries) => entries,
            Err(error) => {
                contents
                    .errors
                    .push(format!("could not read {}: {error}", current.display()));
                continue;
            }
        };
        for entry in entries {
            let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if entry.is_dir() {
                pending.push((entry.clone(), format!("{prefix}{name}/")));
                continue;
            }
            let Some(stem) = name.strip_suffix(&format!(".{extension}")) else {
                continue;
            };
            let path = format!("{prefix}{stem}");
            if !Identifier::validate_path(&path) {
                contents.errors.push(format!(
                    "resource {} has an invalid identifier path '{path}'",
                    entry.display()
                ));
                continue;
            }
            found.push((Identifier::new(namespace.to_owned(), path), entry.clone()));
        }
    }
    found
}

/// Returns the pack directories under `root` in a stable order.
fn pack_directories(root: &Path) -> io::Result<Vec<PathBuf>> {
    read_sorted_directories(root)
}

fn read_sorted_directories(directory: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(read_sorted_entries(directory)?
        .into_iter()
        .filter(|entry| entry.is_dir())
        .collect())
}

fn read_sorted_entries(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        entries.push(entry.path());
    }
    entries.sort();
    Ok(entries)
}
