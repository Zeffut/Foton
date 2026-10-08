//! Driving an import: region by region, chunks converted in parallel and
//! written through Foton's own [`RegionManager`].

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use foton_registry::{init_vanilla_registry, vanilla_dimension_types};
use foton_utils::{ChunkPos, Identifier};
use futures::future::join_all;
use rayon::iter::{IntoParallelIterator as _, ParallelIterator as _};
use simdnbt::owned::{Nbt, NbtCompound, read};
use tokio::runtime::{Builder, Runtime};

use super::ImportError;
use foton_utils::version::WORLD_VERSION;

use super::convert::{ChunkOutcome, ConvertError, Issues, Shape, convert_chunk};
use super::layout::list_region_files;
use super::level::{SourceLevel, read_world_border};
use super::region::{AnvilRegion, RawChunk, RegionError};
use crate::chunk::status::ChunkStatus;
use crate::chunk_saver::{PreparedChunkSave, RegionManager};
use crate::level_data::write_level_data;

/// Chunks converted and written per batch. Bounds memory to a few megabytes
/// whatever the size of the map.
const BATCH_CHUNKS: usize = 32;

/// How many unreadable-chunk messages a report keeps.
const MAX_REPORTED_ERRORS: usize = 10;

/// Chunks per region side.
const REGION_WIDTH: usize = 32;

/// One source dimension and the Foton world it is imported into.
#[derive(Debug, Clone)]
pub struct ImportTarget {
    /// The dimension being read.
    pub dimension: Identifier,
    /// Its folder in the source world, holding `region/` and `entities/`.
    pub source_dir: PathBuf,
    /// The Foton world receiving it, for reporting.
    pub world_key: Identifier,
    /// The Foton world's directory, which holds `level.toml`.
    pub level_dir: PathBuf,
    /// Where that world keeps its region files.
    pub region_dir: PathBuf,
    /// Whether this is the default world of the default domain, which also
    /// receives the world's respawn point.
    pub is_spawn_world: bool,
}

/// What to import.
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// The source world folder, where `level.dat` lives.
    pub source_root: PathBuf,
    /// The dimensions to import.
    pub targets: Vec<ImportTarget>,
    /// Move an existing Foton world aside instead of refusing to touch it.
    pub replace: bool,
    /// Overrides the seed found in the source world.
    pub seed: Option<i64>,
    /// Leave out chunks and entity files still at an older `DataVersion`
    /// instead of refusing the import. They are not converted; Foton generates
    /// those chunks itself.
    pub skip_outdated: bool,
    /// Worker threads for converting and encoding chunks.
    pub threads: usize,
}

/// Progress through one dimension, reported after each batch of chunks.
#[derive(Debug, Clone, Copy)]
pub struct Progress<'a> {
    /// The dimension being imported.
    pub dimension: &'a Identifier,
    /// 1-based index of the region being read.
    pub region: usize,
    /// Region files in the dimension.
    pub region_count: usize,
    /// Chunks written so far in the dimension.
    pub imported: u64,
}

/// What happened to one dimension.
#[derive(Debug, Clone)]
pub struct DimensionReport {
    /// The source dimension.
    pub dimension: Identifier,
    /// The Foton world it went to.
    pub world_key: Identifier,
    /// Region files read.
    pub regions: usize,
    /// Chunks the region files list.
    pub chunks_found: u64,
    /// Chunks written as already generated.
    pub imported: u64,
    /// Chunks vanilla never finished generating, left for Foton to generate,
    /// by the status they stopped at.
    pub not_full: BTreeMap<String, u64>,
    /// Chunks that could not be read or converted, left for Foton to generate.
    pub unreadable: u64,
    /// The first few reasons for [`Self::unreadable`].
    pub unreadable_examples: Vec<String>,
    /// Entity chunks that could not be read; their chunks were imported without entities.
    pub unreadable_entity_chunks: u64,
    /// Chunks left out because they were still at an older `DataVersion`.
    pub outdated_chunks: u64,
    /// Entity chunks left out for the same reason; their chunks were imported without entities.
    pub outdated_entity_chunks: u64,
    /// Entities written, passengers included in their vehicles' count only once.
    pub entities: u64,
    /// Block entities written.
    pub block_entities: u64,
    /// Everything dropped or substituted.
    pub issues: Issues,
}

impl DimensionReport {
    fn new(target: &ImportTarget) -> Self {
        Self {
            dimension: target.dimension.clone(),
            world_key: target.world_key.clone(),
            regions: 0,
            chunks_found: 0,
            imported: 0,
            not_full: BTreeMap::new(),
            unreadable: 0,
            unreadable_examples: Vec::new(),
            unreadable_entity_chunks: 0,
            outdated_chunks: 0,
            outdated_entity_chunks: 0,
            entities: 0,
            block_entities: 0,
            issues: Issues::default(),
        }
    }

    fn note_unreadable(&mut self, pos: ChunkPos, reason: &str) {
        self.unreadable += 1;
        if self.unreadable_examples.len() < MAX_REPORTED_ERRORS {
            self.unreadable_examples
                .push(format!("chunk ({}, {}): {reason}", pos.0.x, pos.0.y));
        }
    }
}

/// What an import did.
#[derive(Debug, Clone)]
pub struct ImportReport {
    /// One entry per imported dimension.
    pub dimensions: Vec<DimensionReport>,
    /// The seed written to the new `level.toml` files, when they were written.
    pub seed: Option<i64>,
    /// Whether `level.toml` was written; false when the source has no `level.dat`
    /// and no seed was given.
    pub level_written: bool,
    /// Wall-clock time of the whole import.
    pub elapsed: Duration,
}

/// The vertical extent of a vanilla dimension.
fn shape_of(dimension: &Identifier) -> Option<Shape> {
    let dimension_type = match (&*dimension.namespace, &*dimension.path) {
        ("minecraft", "overworld") => &vanilla_dimension_types::OVERWORLD,
        ("minecraft", "the_nether") => &vanilla_dimension_types::THE_NETHER,
        ("minecraft", "the_end") => &vanilla_dimension_types::THE_END,
        _ => return None,
    };
    Some(Shape {
        min_y: dimension_type.min_y,
        height: dimension_type.height,
    })
}

/// A region directory is free if it is absent or empty.
fn directory_is_free(directory: &Path) -> bool {
    fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_none()) || !directory.exists()
}

fn staging_dir(region_dir: &Path) -> PathBuf {
    let mut name = region_dir.file_name().unwrap_or_default().to_os_string();
    name.push(".importing");
    region_dir.with_file_name(name)
}

/// Checks every target before any work starts and returns each one's shape.
fn validate_targets(request: &ImportRequest) -> Result<Vec<Shape>, ImportError> {
    let mut shapes = Vec::with_capacity(request.targets.len());
    for target in &request.targets {
        let shape = shape_of(&target.dimension).ok_or_else(|| {
            ImportError::Layout(format!(
                "{} is not a vanilla dimension; only the overworld, the nether and the end can be imported",
                target.dimension
            ))
        })?;
        shapes.push(shape);
        if !target.source_dir.join("region").is_dir() {
            return Err(ImportError::Layout(format!(
                "{} has no region folder",
                target.source_dir.display()
            )));
        }
        let occupied =
            !directory_is_free(&target.region_dir) || target.level_dir.join("level.toml").exists();
        if occupied && !request.replace {
            return Err(ImportError::TargetOccupied(target.level_dir.clone()));
        }
    }
    Ok(shapes)
}

/// The `level.toml` content for one target, or `None` with no seed to write.
fn render_level(
    level: &SourceLevel,
    seed: Option<i64>,
    target: &ImportTarget,
) -> Result<Option<String>, ImportError> {
    let Some(seed) = seed else {
        return Ok(None);
    };
    let border = read_world_border(&target.source_dir)?;
    let data = level.level_data(seed, &target.dimension, border, target.is_spawn_world);
    toml::to_string_pretty(&data)
        .map(Some)
        .map_err(|error| ImportError::Level(format!("level.toml: {error}")))
}

/// Imports the requested dimensions.
///
/// Chunks are written to a staging folder beside each target's `region/` and
/// moved into place only after every dimension has been read, so a refused or
/// failed import leaves the Foton world exactly as it was.
///
/// # Errors
/// [`ImportError::DataVersion`] if the world, or any chunk of it, was written by
/// another Minecraft version; [`ImportError::TargetOccupied`] if a target world
/// already holds data and `replace` is not set; I/O errors otherwise.
pub fn import_world(
    request: &ImportRequest,
    progress: &(dyn Fn(Progress<'_>) + Sync),
) -> Result<ImportReport, ImportError> {
    let started = Instant::now();
    init_vanilla_registry();

    if request.targets.is_empty() {
        return Err(ImportError::Layout(
            "no dimension with a region folder was found to import".to_owned(),
        ));
    }
    let level = SourceLevel::read(&request.source_root)?;
    let seed = request
        .seed
        .or_else(|| level.as_ref().and_then(|level| level.seed));
    if level.is_some() && seed.is_none() {
        return Err(ImportError::Level(
            "the world's seed was not found in data/minecraft/world_gen_settings.dat; \
             pass it with --seed"
                .to_owned(),
        ));
    }

    let shapes = validate_targets(request)?;
    // Rendered before anything is written, so a bad border file cannot strand
    // a half-committed import.
    let source_level = level.unwrap_or_default();
    let level_tomls = request
        .targets
        .iter()
        .map(|target| render_level(&source_level, seed, target))
        .collect::<Result<Vec<_>, _>>()?;

    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| ImportError::io(Path::new("<tokio runtime>"), error))?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(request.threads.max(1))
        .thread_name(|index| format!("anvil-import-{index}"))
        .build()
        .map_err(|error| ImportError::io(Path::new("<thread pool>"), io::Error::other(error)))?;

    let mut staged: Vec<(PathBuf, DimensionReport)> = Vec::new();
    let mut failure = None;
    for (target, shape) in request.targets.iter().zip(shapes) {
        let staging = staging_dir(&target.region_dir);
        let outcome = prepare_staging(&staging).and_then(|()| {
            import_dimension(
                target,
                &staging,
                shape,
                request.skip_outdated,
                &runtime,
                &pool,
                progress,
            )
        });
        match outcome {
            Ok(report) => staged.push((staging, report)),
            Err(error) => {
                failure = Some((staging, error));
                break;
            }
        }
    }
    if let Some((failed_staging, error)) = failure {
        for staging in staged
            .iter()
            .map(|(staging, _)| staging)
            .chain(Some(&failed_staging))
        {
            let _ = fs::remove_dir_all(staging);
        }
        return Err(error);
    }

    let mut level_written = false;
    for ((target, (staging, _)), level_toml) in
        request.targets.iter().zip(&staged).zip(&level_tomls)
    {
        commit(
            target,
            staging,
            request.replace,
            level_toml.as_deref(),
            &runtime,
        )?;
        level_written |= level_toml.is_some();
    }

    Ok(ImportReport {
        dimensions: staged.into_iter().map(|(_, report)| report).collect(),
        seed,
        level_written,
        elapsed: started.elapsed(),
    })
}

fn prepare_staging(staging: &Path) -> Result<(), ImportError> {
    if staging.exists() {
        fs::remove_dir_all(staging).map_err(|error| ImportError::io(staging, error))?;
    }
    fs::create_dir_all(staging).map_err(|error| ImportError::io(staging, error))
}

/// Moves a finished staging folder into place, first setting aside what the
/// target held when a replacement was requested.
fn commit(
    target: &ImportTarget,
    staging: &Path,
    replace: bool,
    level_toml: Option<&str>,
    runtime: &Runtime,
) -> Result<(), ImportError> {
    let level_path = target.level_dir.join("level.toml");
    if replace && (!directory_is_free(&target.region_dir) || level_path.exists()) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let backup = target.level_dir.join(format!("pre-import-{stamp}"));
        fs::create_dir_all(&backup).map_err(|error| ImportError::io(&backup, error))?;
        if target.region_dir.exists() {
            let moved = backup.join("region");
            fs::rename(&target.region_dir, &moved)
                .map_err(|error| ImportError::io(&target.region_dir, error))?;
        }
        if level_path.exists() {
            fs::rename(&level_path, backup.join("level.toml"))
                .map_err(|error| ImportError::io(&level_path, error))?;
        }
    }
    if target.region_dir.exists() {
        fs::remove_dir(&target.region_dir)
            .map_err(|error| ImportError::io(&target.region_dir, error))?;
    }
    if let Some(parent) = target.region_dir.parent() {
        fs::create_dir_all(parent).map_err(|error| ImportError::io(parent, error))?;
    }
    fs::rename(staging, &target.region_dir)
        .map_err(|error| ImportError::io(&target.region_dir, error))?;
    if let Some(content) = level_toml {
        runtime
            .block_on(write_level_data(&level_path, content))
            .map_err(|error| ImportError::io(&level_path, error))?;
    }
    Ok(())
}

fn import_dimension(
    target: &ImportTarget,
    staging: &Path,
    shape: Shape,
    skip_outdated: bool,
    runtime: &Runtime,
    pool: &rayon::ThreadPool,
    progress: &(dyn Fn(Progress<'_>) + Sync),
) -> Result<DimensionReport, ImportError> {
    let region_dir = target.source_dir.join("region");
    let entity_dir = target.source_dir.join("entities");
    let regions =
        list_region_files(&region_dir).map_err(|error| ImportError::io(&region_dir, error))?;
    let manager = RegionManager::new_for_bulk_write(staging);
    let mut report = DimensionReport::new(target);
    report.regions = regions.len();

    for (index, (region_x, region_z, path)) in regions.iter().enumerate() {
        let entity_path = entity_dir.join(path.file_name().unwrap_or_default());
        let context = RegionJob {
            target,
            shape,
            region: (*region_x, *region_z),
            path,
            entity_path: &entity_path,
            progress_index: (index + 1, regions.len()),
            skip_outdated,
        };
        import_region(&context, &manager, runtime, pool, &mut report, progress)?;
    }
    runtime
        .block_on(manager.close_all())
        .map_err(|error| ImportError::io(staging, error))?;
    Ok(report)
}

struct RegionJob<'a> {
    target: &'a ImportTarget,
    shape: Shape,
    region: (i32, i32),
    path: &'a Path,
    entity_path: &'a Path,
    progress_index: (usize, usize),
    skip_outdated: bool,
}

/// One chunk's bytes as read from disk, before any decoding.
struct ChunkInput {
    pos: ChunkPos,
    chunk: Result<RawChunk, RegionError>,
    entities: Result<Option<RawChunk>, RegionError>,
}

enum Processed {
    Converted(Box<super::convert::ConvertedChunk>, Issues),
    NotFull(String),
    WrongVersion(Option<i32>),
    /// Left out on request because it is at an older version.
    Outdated,
    Failed(String),
    /// The chunk converted, but its entity chunk is at an older version and was left out.
    EntitiesOutdated(Box<super::convert::ConvertedChunk>, Issues),
    /// The chunk converted, but its entity chunk could not be read.
    EntitiesLost(Box<super::convert::ConvertedChunk>, Issues, String),
}

fn parse_nbt(bytes: &[u8]) -> Result<NbtCompound, String> {
    match read(&mut Cursor::new(bytes)) {
        Ok(Nbt::Some(root)) => Ok(root.as_compound()),
        Ok(Nbt::None) => Err("empty NBT".to_owned()),
        Err(error) => Err(format!("invalid NBT: {error}")),
    }
}

fn decode(raw: &RawChunk) -> Result<NbtCompound, String> {
    let bytes = raw.decompress().map_err(|error| error.to_string())?;
    parse_nbt(&bytes)
}

fn process(input: ChunkInput, shape: Shape, skip_outdated: bool) -> Processed {
    let raw = match input.chunk {
        Ok(raw) => raw,
        Err(error) => return Processed::Failed(error.to_string()),
    };
    let root = match decode(&raw) {
        Ok(root) => root,
        Err(error) => return Processed::Failed(error),
    };
    let mut entity_problem = None;
    let mut entity_root = match input.entities {
        Ok(Some(raw_entities)) => decode(&raw_entities)
            .map_err(|error| entity_problem = Some(format!("entities: {error}")))
            .ok(),
        Ok(None) => None,
        Err(error) => {
            entity_problem = Some(format!("entities: {error}"));
            None
        }
    };
    let mut entities_outdated = false;
    if let Some(version) = entity_root
        .as_ref()
        .map(|root| root.int("DataVersion"))
        .filter(|version| *version != Some(WORLD_VERSION))
    {
        if !skip_outdated {
            return Processed::WrongVersion(version);
        }
        entity_root = None;
        entities_outdated = true;
    }

    let mut issues = Issues::default();
    match convert_chunk(
        &root,
        entity_root.as_ref(),
        input.pos,
        shape,
        raw.timestamp,
        &mut issues,
    ) {
        Ok(ChunkOutcome::Converted(chunk)) => match entity_problem {
            Some(problem) => Processed::EntitiesLost(chunk, issues, problem),
            None if entities_outdated => Processed::EntitiesOutdated(chunk, issues),
            None => Processed::Converted(chunk, issues),
        },
        Ok(ChunkOutcome::NotFull(status)) => Processed::NotFull(status),
        Err(ConvertError::DataVersion(_)) if skip_outdated => Processed::Outdated,
        Err(ConvertError::DataVersion(found)) => Processed::WrongVersion(found),
        Err(ConvertError::Malformed(message)) => Processed::Failed(message),
    }
}

fn import_region(
    job: &RegionJob<'_>,
    manager: &RegionManager,
    runtime: &Runtime,
    pool: &rayon::ThreadPool,
    report: &mut DimensionReport,
    progress: &(dyn Fn(Progress<'_>) + Sync),
) -> Result<(), ImportError> {
    let mut blocks =
        AnvilRegion::open(job.path).map_err(|error| ImportError::region(job.path, error))?;
    let mut entities = job
        .entity_path
        .is_file()
        .then(|| AnvilRegion::open(job.entity_path))
        .transpose()
        .map_err(|error| ImportError::region(job.entity_path, error))?;

    let present: Vec<(usize, usize)> = (0..REGION_WIDTH)
        .flat_map(|z| (0..REGION_WIDTH).map(move |x| (x, z)))
        .filter(|&(x, z)| blocks.contains(x, z))
        .collect();
    report.chunks_found += present.len() as u64;
    let Some(&(first_x, first_z)) = present.first() else {
        return Ok(());
    };
    let chunk_pos = |(local_x, local_z): (usize, usize)| {
        ChunkPos::new(
            job.region.0 * REGION_WIDTH as i32 + local_x as i32,
            job.region.1 * REGION_WIDTH as i32 + local_z as i32,
        )
    };

    // Holding one chunk of the region open keeps its file handle, and defers
    // the location-table write to a single one when the region is released.
    let anchor = chunk_pos((first_x, first_z));
    runtime
        .block_on(manager.acquire_chunk(anchor))
        .map_err(|error| ImportError::io(job.path, error))?;
    let result = present.chunks(BATCH_CHUNKS).try_for_each(|batch| {
        let inputs: Vec<ChunkInput> = batch
            .iter()
            .filter_map(|&(local_x, local_z)| {
                let chunk = blocks.read_raw(local_x, local_z).transpose()?;
                let entity_chunk = entities
                    .as_mut()
                    .map_or(Ok(None), |region| region.read_raw(local_x, local_z));
                Some(ChunkInput {
                    pos: chunk_pos((local_x, local_z)),
                    chunk,
                    entities: entity_chunk,
                })
            })
            .collect();
        write_batch(job, inputs, manager, runtime, pool, report)?;
        progress(Progress {
            dimension: &job.target.dimension,
            region: job.progress_index.0,
            region_count: job.progress_index.1,
            imported: report.imported,
        });
        Ok(())
    });
    runtime
        .block_on(manager.release_chunk(anchor))
        .map_err(|error| ImportError::io(job.path, error))?;
    result
}

fn write_batch(
    job: &RegionJob<'_>,
    inputs: Vec<ChunkInput>,
    manager: &RegionManager,
    runtime: &Runtime,
    pool: &rayon::ThreadPool,
    report: &mut DimensionReport,
) -> Result<(), ImportError> {
    let shape = job.shape;
    let skip_outdated = job.skip_outdated;
    let positions: Vec<ChunkPos> = inputs.iter().map(|input| input.pos).collect();
    let processed: Vec<Processed> = pool.install(|| {
        inputs
            .into_par_iter()
            .map(|input| process(input, shape, skip_outdated))
            .collect()
    });

    let mut prepared = Vec::with_capacity(processed.len());
    for (pos, outcome) in positions.into_iter().zip(processed) {
        let (chunk, issues) = match outcome {
            Processed::Converted(chunk, issues) => (chunk, issues),
            Processed::EntitiesLost(chunk, issues, problem) => {
                report.unreadable_entity_chunks += 1;
                if report.unreadable_examples.len() < MAX_REPORTED_ERRORS {
                    report
                        .unreadable_examples
                        .push(format!("chunk ({}, {}): {problem}", pos.0.x, pos.0.y));
                }
                (chunk, issues)
            }
            Processed::EntitiesOutdated(chunk, issues) => {
                report.outdated_entity_chunks += 1;
                (chunk, issues)
            }
            Processed::NotFull(status) => {
                *report.not_full.entry(status).or_default() += 1;
                continue;
            }
            Processed::Outdated => {
                report.outdated_chunks += 1;
                continue;
            }
            Processed::WrongVersion(found) => {
                return Err(ImportError::DataVersion {
                    what: format!(
                        "chunk ({}, {}) of {} in {}",
                        pos.0.x,
                        pos.0.y,
                        job.target.dimension,
                        job.path.display()
                    ),
                    found,
                    skippable: true,
                });
            }
            Processed::Failed(reason) => {
                report.note_unreadable(pos, &reason);
                continue;
            }
        };
        report.issues.merge(issues);
        report.entities += chunk.entity_count as u64;
        report.block_entities += chunk.block_entity_count as u64;
        prepared.push(PreparedChunkSave {
            pos,
            status: ChunkStatus::Full,
            persistent: chunk.persistent,
            handled_runtime_entity_ids: Vec::new(),
        });
    }

    let count = prepared.len() as u64;
    let writes = prepared
        .into_iter()
        .map(|save| manager.save_chunk_data(save, pool));
    for outcome in runtime.block_on(join_all(writes)) {
        outcome.map_err(|error| ImportError::io(job.path, error))?;
    }
    report.imported += count;
    Ok(())
}
