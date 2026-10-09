//! Where a datapack's files come from: a folder or a `.zip` archive.
//!
//! Vanilla parity: `PathPackResources` for a folder and `FilePackResources` for
//! an archive, both listed through `FolderRepositorySource`. In an archive
//! `pack.mcmeta` and `data/` sit at the archive root, so a zip made by
//! compressing the pack's *folder* (one extra level) is not a datapack, and the
//! error says so rather than just "no data".

use std::{
    fs::{self, File},
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use foton_utils::Identifier;
use zip::ZipArchive;

/// The most text one resource may hold. A zip entry's declared size is
/// attacker-controlled, so reading is capped rather than trusting it.
const MAX_RESOURCE_BYTES: u64 = 16 * 1024 * 1024;

/// One file of a pack that a loader asked for.
pub(super) struct Resource {
    pub(super) id: Identifier,
    /// The file path or archive entry name, which is also what error messages
    /// show and what [`PackResources::read`] opens.
    pub(super) location: String,
}

pub(super) enum PackResources {
    Directory(PathBuf),
    Archive(ArchivePack),
}

pub(super) struct ArchivePack {
    archive: ZipArchive<File>,
    /// Every entry name, sorted, so listings are stable across zip tools.
    names: Vec<String>,
}

impl PackResources {
    /// A folder is a pack when it has a `data/` directory; any other folder is
    /// not a pack and is skipped without comment, as it always has been.
    pub(super) fn directory(path: &Path) -> Option<Self> {
        path.join("data")
            .is_dir()
            .then(|| Self::Directory(path.to_path_buf()))
    }

    /// Opens a `.zip`, or says why it is not a datapack.
    pub(super) fn archive(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("could not open it: {error}"))?;
        let archive =
            ZipArchive::new(file).map_err(|error| format!("it is not a valid zip: {error}"))?;
        let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        names.sort();

        if !names.iter().any(|name| name == "pack.mcmeta") {
            return Err(missing_root_entry("pack.mcmeta", &names));
        }
        if !names.iter().any(|name| name.starts_with("data/")) {
            return Err(missing_root_entry("data/", &names));
        }
        let mut pack = ArchivePack { archive, names };
        let mcmeta = pack
            .read("pack.mcmeta")
            .map_err(|error| format!("could not read pack.mcmeta: {error}"))?;
        serde_json::from_str::<serde_json::Value>(&mcmeta)
            .map_err(|error| format!("pack.mcmeta is not valid JSON: {error}"))?;
        Ok(Self::Archive(pack))
    }

    /// The namespace folders under `data/`, unvalidated and sorted.
    pub(super) fn namespaces(&self) -> io::Result<Vec<String>> {
        match self {
            Self::Directory(root) => Ok(read_sorted_entries(&root.join("data"))?
                .into_iter()
                .filter(|entry| entry.is_dir())
                .filter_map(|entry| entry.file_name()?.to_str().map(str::to_owned))
                .collect()),
            Self::Archive(pack) => {
                let mut namespaces: Vec<String> = pack
                    .names
                    .iter()
                    .filter_map(|name| name.strip_prefix("data/")?.split_once('/'))
                    .map(|(namespace, _)| namespace.to_owned())
                    .collect();
                namespaces.dedup();
                Ok(namespaces)
            }
        }
    }

    /// Lists the files under `data/<namespace>/<directory>` with `extension`.
    ///
    /// Identifiers that are not valid resource paths go to `errors` and are
    /// left out, the way `FileToIdConverter` reports them.
    pub(super) fn resources(
        &self,
        namespace: &str,
        directory: &[&str],
        extension: &str,
        errors: &mut Vec<String>,
    ) -> Vec<Resource> {
        match self {
            Self::Directory(root) => {
                let mut base = root.join("data").join(namespace);
                base.extend(directory);
                directory_resources(&base, namespace, extension, errors)
            }
            Self::Archive(pack) => {
                let prefix = format!("data/{namespace}/{}/", directory.join("/"));
                let suffix = format!(".{extension}");
                let mut found = Vec::new();
                for name in &pack.names {
                    let Some(relative) = name.strip_prefix(&prefix) else {
                        continue;
                    };
                    let Some(stem) = relative.strip_suffix(&suffix) else {
                        continue;
                    };
                    if !Identifier::validate_path(stem) {
                        errors.push(format!(
                            "resource {name} has an invalid identifier path '{stem}'"
                        ));
                        continue;
                    }
                    found.push(Resource {
                        id: Identifier::new(namespace.to_owned(), stem.to_owned()),
                        location: name.clone(),
                    });
                }
                found
            }
        }
    }

    pub(super) fn read(&mut self, resource: &Resource) -> io::Result<String> {
        match self {
            Self::Directory(_) => read_capped(File::open(&resource.location)?),
            Self::Archive(pack) => pack.read(&resource.location),
        }
    }
}

impl ArchivePack {
    fn read(&mut self, name: &str) -> io::Result<String> {
        let entry = self.archive.by_name(name).map_err(io::Error::other)?;
        read_capped(entry)
    }
}

fn read_capped(reader: impl io::Read) -> io::Result<String> {
    let mut text = String::new();
    // One byte past the cap tells "exactly at the cap" from "over it".
    reader
        .take(MAX_RESOURCE_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_RESOURCE_BYTES {
        return Err(io::Error::other(format!(
            "larger than the {MAX_RESOURCE_BYTES} byte limit"
        )));
    }
    Ok(text)
}

/// Explains a missing root entry, and points at the usual cause: a pack zipped
/// together with its folder.
fn missing_root_entry(wanted: &str, names: &[String]) -> String {
    let nested = names.iter().find_map(|name| {
        let (folder, rest) = name.split_once('/')?;
        (rest == "pack.mcmeta" || rest.starts_with("data/")).then_some(folder)
    });
    match nested {
        Some(folder) => format!(
            "no {wanted} at the archive root, but '{folder}/' contains the pack: zip the \
             contents of that folder, not the folder itself"
        ),
        None => format!("no {wanted} at the archive root"),
    }
}

fn directory_resources(
    directory: &Path,
    namespace: &str,
    extension: &str,
    errors: &mut Vec<String>,
) -> Vec<Resource> {
    let mut found = Vec::new();
    if !directory.is_dir() {
        return found;
    }
    let suffix = format!(".{extension}");
    let mut pending = vec![(directory.to_path_buf(), String::new())];
    while let Some((current, prefix)) = pending.pop() {
        let entries = match read_sorted_entries(&current) {
            Ok(entries) => entries,
            Err(error) => {
                errors.push(format!("could not read {}: {error}", current.display()));
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
            let Some(stem) = name.strip_suffix(&suffix) else {
                continue;
            };
            let path = format!("{prefix}{stem}");
            if !Identifier::validate_path(&path) {
                errors.push(format!(
                    "resource {} has an invalid identifier path '{path}'",
                    entry.display()
                ));
                continue;
            }
            found.push(Resource {
                id: Identifier::new(namespace.to_owned(), path),
                location: entry.to_string_lossy().into_owned(),
            });
        }
    }
    found
}

/// A folder's entries in a stable order, hidden files left out.
pub(super) fn read_sorted_entries(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        entries.push(entry.path());
    }
    entries.sort();
    Ok(entries)
}
