//! Whole-import tests against a small synthetic vanilla world on disk.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::{env, process};

use flate2::Compression as Level;
use flate2::write::{GzEncoder, ZlibEncoder};
use foton_utils::version::WORLD_VERSION;
use foton_utils::{ChunkPos, Identifier};
use simdnbt::owned::{Nbt, NbtCompound, NbtList, NbtTag};
use tokio::runtime::Builder;

use super::*;
use crate::chunk_saver::RegionManager;

fn temp_dir(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("foton-anvil-import-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("test directory");
    dir
}

fn root_bytes(compound: NbtCompound) -> Vec<u8> {
    let mut bytes = Vec::new();
    Nbt::new("".into(), compound).write(&mut bytes);
    bytes
}

fn zlib(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Level::default());
    encoder.write_all(bytes).expect("in-memory write");
    encoder.finish().expect("in-memory finish")
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Level::default());
    encoder.write_all(bytes).expect("in-memory write");
    encoder.finish().expect("in-memory finish")
}

fn entry(key: &str, tag: NbtTag) -> NbtCompound {
    let mut compound = NbtCompound::new();
    compound.insert(key, tag);
    compound
}

/// A chunk of one air section; enough to be a valid `Full` chunk.
fn chunk(pos: ChunkPos, version: i32, status: &str) -> Vec<u8> {
    let mut palette_entry = NbtCompound::new();
    palette_entry.insert("Name", "minecraft:stone");
    let block_states = entry(
        "palette",
        NbtTag::List(NbtList::Compound(vec![palette_entry])),
    );
    let mut section = NbtCompound::new();
    section.insert("Y", NbtTag::Byte(0));
    section.insert("block_states", NbtTag::Compound(block_states));

    let mut root = NbtCompound::new();
    root.insert("DataVersion", version);
    root.insert("xPos", pos.0.x);
    root.insert("zPos", pos.0.y);
    root.insert("yPos", -4);
    root.insert("Status", status);
    root.insert("isLightOn", NbtTag::Byte(1));
    root.insert("sections", NbtTag::List(NbtList::Compound(vec![section])));
    root_bytes(root)
}

/// How one chunk is stored in a test region.
enum Stored {
    Inline(u8, Vec<u8>),
    External(u8, Vec<u8>),
}

fn write_region(directory: &Path, x: i32, z: i32, chunks: &[((usize, usize), Stored)]) {
    fs::create_dir_all(directory).expect("test directory");
    let mut file = vec![0u8; 8192];
    let mut sector = 2u32;
    for ((local_x, local_z), stored) in chunks {
        let (id, payload) = match stored {
            Stored::Inline(id, payload) => (*id, payload.clone()),
            Stored::External(id, payload) => {
                let chunk_x = x * 32 + *local_x as i32;
                let chunk_z = z * 32 + *local_z as i32;
                fs::write(
                    directory.join(format!("c.{chunk_x}.{chunk_z}.mcc")),
                    payload,
                )
                .expect("external chunk");
                (id | 0x80, Vec::new())
            }
        };
        let sectors = (5 + payload.len()).div_ceil(4096) as u32;
        let slot = local_z * 32 + local_x;
        file[slot * 4..slot * 4 + 4].copy_from_slice(&((sector << 8) | sectors).to_be_bytes());
        file.extend_from_slice(&(payload.len() as i32 + 1).to_be_bytes());
        file.push(id);
        file.extend_from_slice(&payload);
        file.resize(8192 + (sector - 2 + sectors) as usize * 4096, 0);
        sector += sectors;
    }
    fs::write(directory.join(format!("r.{x}.{z}.mca")), file).expect("region file");
}

fn write_level(root: &Path, version: i32, seed: i64) {
    let mut data = NbtCompound::new();
    data.insert("DataVersion", version);
    data.insert("Time", NbtTag::Long(1234));
    let level = root_bytes(entry("Data", NbtTag::Compound(data)));
    fs::write(root.join("level.dat"), gzip(&level)).expect("level.dat");

    let mut settings = NbtCompound::new();
    settings.insert("seed", NbtTag::Long(seed));
    let settings = root_bytes(entry("data", NbtTag::Compound(settings)));
    let directory = root.join("data/minecraft");
    fs::create_dir_all(&directory).expect("saved data folder");
    fs::write(directory.join("world_gen_settings.dat"), gzip(&settings)).expect("seed file");
}

struct Fixture {
    source: PathBuf,
    foton: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = temp_dir(name);
        let source = root.join("source");
        let foton = root.join("foton");
        fs::create_dir_all(&source).expect("source");
        Self { source, foton }
    }

    fn request(&self) -> ImportRequest {
        ImportRequest {
            source_root: self.source.clone(),
            targets: vec![ImportTarget {
                dimension: Identifier::vanilla_static("overworld"),
                source_dir: self.source.clone(),
                world_key: Identifier::vanilla_static("overworld"),
                level_dir: self.foton.clone(),
                region_dir: self.foton.join("region"),
                is_spawn_world: true,
            }],
            replace: false,
            seed: None,
            skip_outdated: false,
            threads: 2,
        }
    }

    fn region_dir(&self) -> PathBuf {
        self.source.join("region")
    }

    fn exists_in_foton(&self, pos: ChunkPos) -> bool {
        let manager = RegionManager::new(self.foton.join("region"));
        Builder::new_current_thread()
            .build()
            .expect("runtime")
            .block_on(manager.chunk_exists(pos))
            .expect("chunk table readable")
    }

    fn cleanup(self) {
        let _ = fs::remove_dir_all(self.source.parent().expect("fixture root"));
    }
}

fn run(request: &ImportRequest) -> Result<ImportReport, ImportError> {
    import_world(request, &|_| {})
}

#[test]
fn a_current_world_is_imported_and_unfinished_chunks_are_left_out() {
    let fixture = Fixture::new("current");
    write_level(&fixture.source, WORLD_VERSION, 99);
    write_region(
        &fixture.region_dir(),
        0,
        0,
        &[
            (
                (0, 0),
                Stored::Inline(
                    2,
                    zlib(&chunk(ChunkPos::new(0, 0), WORLD_VERSION, "minecraft:full")),
                ),
            ),
            (
                (3, 1),
                Stored::Inline(
                    2,
                    zlib(&chunk(ChunkPos::new(3, 1), WORLD_VERSION, "minecraft:full")),
                ),
            ),
            (
                (4, 4),
                Stored::Inline(
                    2,
                    zlib(&chunk(
                        ChunkPos::new(4, 4),
                        WORLD_VERSION,
                        "minecraft:noise",
                    )),
                ),
            ),
            // Oversized chunks live beside the region, gzip-compressed here.
            (
                (9, 9),
                Stored::External(
                    1,
                    gzip(&chunk(ChunkPos::new(9, 9), WORLD_VERSION, "minecraft:full")),
                ),
            ),
        ],
    );

    let report = run(&fixture.request()).expect("import");
    let dimension = &report.dimensions[0];
    assert_eq!((dimension.chunks_found, dimension.imported), (4, 3));
    assert_eq!(dimension.not_full.get("minecraft:noise"), Some(&1));
    assert_eq!(report.seed, Some(99));

    for (x, z, expected) in [(0, 0, true), (3, 1, true), (9, 9, true), (4, 4, false)] {
        assert_eq!(
            fixture.exists_in_foton(ChunkPos::new(x, z)),
            expected,
            "chunk {x} {z}"
        );
    }
    let level = fs::read_to_string(fixture.foton.join("level.toml")).expect("level.toml");
    assert!(level.contains("seed = 99") && level.contains("game_time = 1234"));
    assert!(
        !fixture.foton.join("region.importing").exists(),
        "the staging folder is gone once the import is committed"
    );
    fixture.cleanup();
}

#[test]
fn a_world_from_another_version_is_refused_before_anything_is_written() {
    let fixture = Fixture::new("stale-level");
    write_level(&fixture.source, WORLD_VERSION - 1, 1);
    write_region(
        &fixture.region_dir(),
        0,
        0,
        &[(
            (0, 0),
            Stored::Inline(
                2,
                zlib(&chunk(ChunkPos::new(0, 0), WORLD_VERSION, "minecraft:full")),
            ),
        )],
    );

    let error = run(&fixture.request()).expect_err("level.dat is at the wrong version");
    assert!(matches!(&error, ImportError::DataVersion { what, .. } if what == "level.dat"));
    assert!(
        error.to_string().contains("--forceUpgrade"),
        "the message tells the user what to do: {error}"
    );
    assert!(!fixture.foton.exists(), "nothing was created");
    fixture.cleanup();
}

#[test]
fn one_chunk_from_another_version_aborts_the_whole_import_cleanly() {
    let fixture = Fixture::new("stale-chunk");
    write_level(&fixture.source, WORLD_VERSION, 1);
    write_region(
        &fixture.region_dir(),
        0,
        0,
        &[
            (
                (0, 0),
                Stored::Inline(
                    2,
                    zlib(&chunk(ChunkPos::new(0, 0), WORLD_VERSION, "minecraft:full")),
                ),
            ),
            (
                (1, 0),
                Stored::Inline(
                    2,
                    zlib(&chunk(
                        ChunkPos::new(1, 0),
                        WORLD_VERSION - 50,
                        "minecraft:full",
                    )),
                ),
            ),
        ],
    );

    let error = run(&fixture.request()).expect_err("a chunk is at the wrong version");
    assert!(matches!(error, ImportError::DataVersion { .. }));
    assert!(
        !fixture.foton.join("region").exists(),
        "no half-imported world"
    );
    assert!(
        !fixture.foton.join("region.importing").exists(),
        "no staging left behind"
    );
    assert!(!fixture.foton.join("level.toml").exists());
    fixture.cleanup();
}

#[test]
fn an_existing_world_is_only_replaced_when_asked_and_is_kept_aside() {
    let fixture = Fixture::new("replace");
    write_level(&fixture.source, WORLD_VERSION, 5);
    write_region(
        &fixture.region_dir(),
        0,
        0,
        &[(
            (0, 0),
            Stored::Inline(
                2,
                zlib(&chunk(ChunkPos::new(0, 0), WORLD_VERSION, "minecraft:full")),
            ),
        )],
    );
    run(&fixture.request()).expect("first import");

    let mut again = fixture.request();
    assert!(matches!(run(&again), Err(ImportError::TargetOccupied(_))));

    again.replace = true;
    run(&again).expect("replacing import");
    let backups: Vec<_> = fs::read_dir(&fixture.foton)
        .expect("world folder")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("pre-import-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert!(backups[0].path().join("region").is_dir());
    assert!(backups[0].path().join("level.toml").is_file());
    assert!(fixture.exists_in_foton(ChunkPos::new(0, 0)));
    fixture.cleanup();
}

#[test]
fn a_damaged_chunk_is_reported_and_the_rest_are_imported() {
    let fixture = Fixture::new("damaged");
    write_level(&fixture.source, WORLD_VERSION, 5);
    write_region(
        &fixture.region_dir(),
        0,
        0,
        &[
            (
                (0, 0),
                Stored::Inline(
                    2,
                    zlib(&chunk(ChunkPos::new(0, 0), WORLD_VERSION, "minecraft:full")),
                ),
            ),
            ((1, 0), Stored::Inline(2, b"this is not zlib".to_vec())),
        ],
    );

    let report = run(&fixture.request()).expect("import");
    let dimension = &report.dimensions[0];
    assert_eq!((dimension.imported, dimension.unreadable), (1, 1));
    assert!(dimension.unreadable_examples[0].contains("chunk (1, 0)"));
    fixture.cleanup();
}

#[test]
fn a_world_without_a_seed_asks_for_one_instead_of_guessing() {
    let fixture = Fixture::new("no-seed");
    write_level(&fixture.source, WORLD_VERSION, 5);
    fs::remove_file(fixture.source.join("data/minecraft/world_gen_settings.dat")).expect("remove");
    write_region(
        &fixture.region_dir(),
        0,
        0,
        &[(
            (0, 0),
            Stored::Inline(
                2,
                zlib(&chunk(ChunkPos::new(0, 0), WORLD_VERSION, "minecraft:full")),
            ),
        )],
    );

    let error = run(&fixture.request()).expect_err("the seed cannot be found");
    assert!(error.to_string().contains("--seed"), "{error}");

    let mut with_seed = fixture.request();
    with_seed.seed = Some(-7);
    assert_eq!(run(&with_seed).expect("import").seed, Some(-7));
    fixture.cleanup();
}

#[test]
fn outdated_chunks_and_entity_files_are_left_out_only_when_asked() {
    let fixture = Fixture::new("outdated");
    write_level(&fixture.source, WORLD_VERSION, 5);
    write_region(
        &fixture.region_dir(),
        0,
        0,
        &[
            (
                (0, 0),
                Stored::Inline(
                    2,
                    zlib(&chunk(ChunkPos::new(0, 0), WORLD_VERSION, "minecraft:full")),
                ),
            ),
            (
                (1, 0),
                Stored::Inline(
                    2,
                    zlib(&chunk(
                        ChunkPos::new(1, 0),
                        WORLD_VERSION - 50,
                        "minecraft:full",
                    )),
                ),
            ),
        ],
    );
    // The entity file of the current chunk is the one left behind.
    let mut stale_entities = NbtCompound::new();
    stale_entities.insert("DataVersion", WORLD_VERSION - 50);
    stale_entities.insert("Position", NbtTag::IntArray(vec![0, 0]));
    stale_entities.insert("Entities", NbtTag::List(NbtList::Compound(Vec::new())));
    write_region(
        &fixture.source.join("entities"),
        0,
        0,
        &[((0, 0), Stored::Inline(2, zlib(&root_bytes(stale_entities))))],
    );

    let mut request = fixture.request();
    let error = run(&request).expect_err("outdated data refuses the import by default");
    assert!(matches!(
        &error,
        ImportError::DataVersion {
            skippable: true,
            ..
        }
    ));
    assert!(error.to_string().contains("--skip-outdated"), "{error}");
    assert!(!fixture.foton.join("region").exists());

    request.skip_outdated = true;
    let report = run(&request).expect("import that skips what is outdated");
    let dimension = &report.dimensions[0];
    assert_eq!(
        (
            dimension.imported,
            dimension.outdated_chunks,
            dimension.outdated_entity_chunks
        ),
        (1, 1, 1)
    );
    assert!(fixture.exists_in_foton(ChunkPos::new(0, 0)));
    assert!(!fixture.exists_in_foton(ChunkPos::new(1, 0)));
    fixture.cleanup();
}
