//! `foton import-anvil`: fills the configured world storage from a vanilla world.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use foton_core::chunk_saver::anvil::{
    ImportRequest, Progress, SourceDimension, discover_dimensions, import_world,
    parse_dimension_name, resolve_targets,
};
use foton_utils::Identifier;

use crate::args::ImportArgs;
use crate::config;

/// Runs the import and prints what it did.
///
/// # Errors
/// Returns the message to show the user if the world cannot be read, is not at
/// the targeted `DataVersion`, or cannot be written.
pub fn run(args: ImportArgs) -> Result<(), String> {
    let foton_config = config::load_or_create(Path::new("config/config.toml"))?;

    let mut wanted: Vec<Identifier> = Vec::new();
    for name in &args.dimensions {
        wanted.push(parse_dimension_name(name).ok_or_else(|| {
            format!(
                "`{name}` is not a dimension name; use overworld, the_nether, the_end or a full id"
            )
        })?);
    }
    let found = discover_dimensions(&args.source)
        .map_err(|error| format!("cannot read {}: {error}", args.source.display()))?;
    if found.is_empty() {
        return Err(format!(
            "no dimension with a region folder in {}; expected dimensions/<namespace>/<path>/region, \
             or region/, DIM-1/region and DIM1/region",
            args.source.display()
        ));
    }
    let dimensions: Vec<SourceDimension> = if wanted.is_empty() {
        found
    } else {
        for key in &wanted {
            if found.iter().all(|dimension| &dimension.key != key) {
                return Err(format!(
                    "{} has no dimension {key}; it has: {}",
                    args.source.display(),
                    found
                        .iter()
                        .map(|dimension| dimension.key.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        found
            .into_iter()
            .filter(|dimension| wanted.contains(&dimension.key))
            .collect()
    };

    let targets = resolve_targets(&foton_config.worlds, dimensions, args.world.as_deref())
        .map_err(|error| error.to_string())?;
    for target in &targets {
        println!(
            "importing {} from {} into {}",
            target.dimension,
            target.source_dir.display(),
            target.region_dir.display()
        );
    }

    let request = ImportRequest {
        source_root: args.source,
        targets,
        replace: args.replace,
        seed: args.seed,
        skip_outdated: args.skip_outdated,
        threads: thread::available_parallelism().map_or(2, usize::from),
    };
    let last_region = AtomicUsize::new(0);
    let report = import_world(&request, &|progress: Progress<'_>| {
        if last_region.swap(progress.region, Ordering::Relaxed) != progress.region {
            println!(
                "  {}: region {}/{}, {} chunks imported",
                progress.dimension, progress.region, progress.region_count, progress.imported
            );
        }
    })
    .map_err(|error| error.to_string())?;
    print!("{report}");
    Ok(())
}
