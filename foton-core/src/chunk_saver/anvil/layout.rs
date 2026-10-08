//! Finding the dimensions inside a vanilla world folder.
//!
//! Vanilla 26.x keeps every dimension under `dimensions/<namespace>/<path>/`.
//! Before that the overworld sat at the world root and the other two in
//! `DIM-1` and `DIM1`; Paper additionally splits them across separate world
//! folders, each in the old shape. The legacy shapes are recognized so a world
//! that was already upgraded in place still imports, but only the `DataVersion`
//! decides whether a world is current: see [`super::ImportError::DataVersion`].

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use foton_utils::Identifier;

/// One dimension found in a source world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDimension {
    /// The dimension's registry key, e.g. `minecraft:the_nether`.
    pub key: Identifier,
    /// The folder that holds `region/` and `entities/`.
    pub dir: PathBuf,
}

/// The three dimensions that live at fixed legacy folders.
const LEGACY_FOLDERS: [(&str, &str); 3] = [
    ("", "overworld"),
    ("DIM-1", "the_nether"),
    ("DIM1", "the_end"),
];

/// Lists the dimensions of `source` that have a `region/` folder, in a stable
/// order: the overworld, the nether, the end, then anything else by key.
///
/// # Errors
/// Returns an error if a directory that exists cannot be listed.
pub fn discover_dimensions(source: &Path) -> io::Result<Vec<SourceDimension>> {
    let mut found = Vec::new();

    let dimensions = source.join("dimensions");
    if dimensions.is_dir() {
        for namespace in sorted_subdirectories(&dimensions)? {
            let Some(namespace_name) = namespace.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            for dimension in sorted_subdirectories(&namespace)? {
                let Some(path_name) = dimension.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if !dimension.join("region").is_dir() {
                    continue;
                }
                let key = Identifier::new(namespace_name.to_owned(), path_name.to_owned());
                found.push(SourceDimension {
                    key,
                    dir: dimension,
                });
            }
        }
    }

    for (folder, name) in LEGACY_FOLDERS {
        let dir = if folder.is_empty() {
            source.to_path_buf()
        } else {
            source.join(folder)
        };
        let key = Identifier::vanilla_static(name);
        if dir.join("region").is_dir() && found.iter().all(|dimension| dimension.key != key) {
            found.push(SourceDimension { key, dir });
        }
    }

    found.sort_by_key(|dimension| {
        let rank = LEGACY_FOLDERS
            .iter()
            .position(|(_, name)| dimension.key == Identifier::vanilla_static(name))
            .unwrap_or(LEGACY_FOLDERS.len());
        (rank, dimension.key.to_string())
    });
    Ok(found)
}

/// Lists `r.<x>.<z>.mca` files in `directory` ordered by coordinates, so a run
/// is deterministic and its progress readable.
///
/// # Errors
/// Returns an error if the directory cannot be listed.
pub fn list_region_files(directory: &Path) -> io::Result<Vec<(i32, i32, PathBuf)>> {
    let mut regions = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if let Some((x, z)) = super::region::AnvilRegion::coordinates_from_path(&path) {
            regions.push((x, z, path));
        }
    }
    regions.sort_by_key(|(x, z, _)| (*z, *x));
    Ok(regions)
}

fn sorted_subdirectories(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut directories = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            directories.push(path);
        }
    }
    directories.sort();
    Ok(directories)
}

#[cfg(test)]
mod tests {
    use std::{env, process};

    use super::*;

    fn world_with(folders: &[&str]) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "foton-anvil-layout-{}-{}",
            process::id(),
            folders.join("+").replace(['/', '\\'], "_")
        ));
        let _ = fs::remove_dir_all(&root);
        for folder in folders {
            fs::create_dir_all(root.join(folder).join("region")).expect("test setup");
        }
        fs::create_dir_all(&root).expect("test setup");
        root
    }

    fn keys(dimensions: &[SourceDimension]) -> Vec<String> {
        dimensions
            .iter()
            .map(|dimension| dimension.key.to_string())
            .collect()
    }

    #[test]
    fn the_26x_layout_is_read_from_the_dimensions_folder() {
        let root = world_with(&[
            "dimensions/minecraft/the_end",
            "dimensions/minecraft/overworld",
            "dimensions/botw/arena",
        ]);
        let found = discover_dimensions(&root).expect("listing");
        assert_eq!(
            keys(&found),
            ["minecraft:overworld", "minecraft:the_end", "botw:arena"]
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn the_legacy_folders_map_to_the_vanilla_dimensions() {
        let root = world_with(&["DIM1", "", "DIM-1"]);
        let found = discover_dimensions(&root).expect("listing");
        assert_eq!(
            keys(&found),
            [
                "minecraft:overworld",
                "minecraft:the_nether",
                "minecraft:the_end"
            ]
        );
        assert_eq!(found[1].dir, root.join("DIM-1"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_dimension_without_regions_is_not_listed() {
        let root = world_with(&[]);
        fs::create_dir_all(root.join("dimensions/minecraft/the_nether")).expect("test setup");
        assert!(discover_dimensions(&root).expect("listing").is_empty());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
