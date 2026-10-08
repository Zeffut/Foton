//! Import of vanilla Anvil (`.mca`) worlds into Foton's region format.
//!
//! Foton does not read Anvil at runtime. This module is the offline bridge for
//! `foton import-anvil`: it reads a vanilla or Paper world and writes the chunks
//! the way Foton's own chunk saver persists a `Full` chunk, so the server loads
//! them as already generated and never regenerates them.
//!
//! ## No data fixing
//!
//! Foton has no `DataFixer`. The source must already carry the `DataVersion` of
//! the targeted Minecraft version, in `level.dat` and in every chunk, and
//! anything else is refused with instructions to upgrade it once with the
//! vanilla (or Paper) server's `--forceUpgrade`.
//!
//! ## What is carried over
//!
//! Every block state, biomes, block entities (with their items and Paper's
//! `PublicBukkitValues`), entities with scoreboard tags, custom names, custom
//! data and `BukkitValues`, passengers, scheduled block and fluid ticks, pending
//! post-processing, vanilla's light arrays, and the chunk's `ChunkBukkitValues`.
//! From the level data: seed, game time, clocks, weather, difficulty, spawn,
//! game rules and each dimension's world border.
//!
//! ## What is not
//!
//! Structure starts and references (the blocks of a structure are kept, but
//! Foton will not know the structure is there), POI occupancy (villagers
//! reclaim beds and workstations on their own), forced-chunk tickets,
//! scoreboards, command storage, maps and player data. Chunks vanilla had not
//! finished generating are left out so Foton generates them.

mod convert;
mod entities;
mod import;
mod layout;
mod level;
mod region;
mod report;
mod targets;
#[cfg(test)]
mod tests;

use std::io;
use std::path::{Path, PathBuf};

use foton_utils::MC_VERSION;
use foton_utils::version::WORLD_VERSION;
use thiserror::Error;

pub use convert::{ConvertError, Issues};
pub use import::{
    DimensionReport, ImportReport, ImportRequest, ImportTarget, Progress, import_world,
};
pub use layout::{SourceDimension, discover_dimensions};
pub use region::{AnvilRegion, Compression, RawChunk, RegionError};
pub use targets::{parse_dimension_name, resolve_targets};

/// Why an import was refused or failed.
#[derive(Debug, Error)]
pub enum ImportError {
    /// A file could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or folder involved.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// A region file could not be opened.
    #[error("{}: {source}", path.display())]
    Region {
        /// The region file.
        path: PathBuf,
        /// What was wrong with it.
        source: RegionError,
    },
    /// The world was written by another Minecraft version.
    #[error("{}", data_version_message(what, *found, *skippable))]
    DataVersion {
        /// What carried the version: `level.dat`, or a chunk.
        what: String,
        /// The version it carried, if it had one.
        found: Option<i32>,
        /// Whether `--skip-outdated` would get past it: true for a chunk,
        /// false for `level.dat`.
        skippable: bool,
    },
    /// The level data is missing something an import needs.
    #[error("{0}")]
    Level(String),
    /// Nothing to import, or a dimension Foton cannot hold.
    #[error("{0}")]
    Layout(String),
    /// The target Foton world already holds data.
    #[error(
        "{} already holds a Foton world; importing would mix two worlds. \
         Pass --replace to move it aside first",
        .0.display()
    )]
    TargetOccupied(PathBuf),
}

impl ImportError {
    pub(crate) fn io(path: &Path, source: io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn region(path: &Path, source: RegionError) -> Self {
        Self::Region {
            path: path.to_path_buf(),
            source,
        }
    }
}

fn data_version_message(what: &str, found: Option<i32>, skippable: bool) -> String {
    let found = found.map_or_else(
        || "no DataVersion".to_owned(),
        |version| format!("DataVersion {version}"),
    );
    let hint = if skippable {
        " Vanilla's --forceUpgrade can leave a few chunks behind; if only a few are affected, \
         pass --skip-outdated to leave them out and let Foton generate them."
    } else {
        ""
    };
    format!(
        "{what} has {found}, but this Foton targets Minecraft {MC_VERSION} (DataVersion \
         {WORLD_VERSION}). Foton does not convert worlds between versions. Open the world once \
         with the vanilla {MC_VERSION} server using --forceUpgrade, which upgrades it in place \
         (back it up first), then import it again.{hint}"
    )
}
