//! Vanilla chunk NBT to Foton's [`PersistentChunk`].
//!
//! The output is what the chunk saver itself writes for a `Full` chunk, so
//! Foton loads it as already generated. What the loader derives it is left to
//! derive: heightmaps are rebuilt from the sections, and POI tickets start
//! free. Structures are not carried over; see the module docs for why.

use std::collections::BTreeMap;
use std::str::FromStr as _;

use foton_registry::{REGISTRY, RegistryExt as _, vanilla_biomes, vanilla_blocks};
use foton_utils::version::WORLD_VERSION;
use foton_utils::{BlockStateId, ChunkPos, Identifier};
use rustc_hash::FxHashMap;
use simdnbt::owned::{NbtCompound, NbtList};
use thiserror::Error;

use super::entities::{convert_block_entities, convert_entities};
use crate::chunk_saver::bit_pack::{bits_for_palette_len, pack_indices};
use crate::chunk_saver::{
    ChunkStorage, PersistentBiomeData, PersistentBlockState, PersistentChunk, PersistentLightData,
    PersistentLightSection, PersistentSection, PersistentTick,
};

/// Bytes in one light layer of one section: 4096 nibbles.
const LIGHT_LAYER_BYTES: usize = 2048;

/// Blocks in a section.
const BLOCKS_PER_SECTION: usize = 4096;

/// Biome cells in a section.
const BIOMES_PER_SECTION: usize = 64;

/// Vanilla stores no fewer than this many bits per block-state index.
const MIN_BLOCK_BITS: usize = 4;

/// What a dimension looks like vertically.
#[derive(Debug, Clone, Copy)]
pub(super) struct Shape {
    pub(super) min_y: i32,
    pub(super) height: i32,
}

impl Shape {
    pub(super) const fn section_count(self) -> usize {
        (self.height / 16) as usize
    }

    pub(super) const fn min_section(self) -> i32 {
        self.min_y.div_euclid(16)
    }
}

/// Why one chunk could not be converted.
#[derive(Debug, Error)]
pub enum ConvertError {
    /// The chunk was written by another Minecraft version. This stops the whole
    /// import: a world is either upgraded or it is not.
    #[error("DataVersion {0:?}")]
    DataVersion(Option<i32>),
    /// The chunk is damaged, or built for a different world.
    #[error("{0}")]
    Malformed(String),
}

fn malformed(message: impl Into<String>) -> ConvertError {
    ConvertError::Malformed(message.into())
}

/// What the importer had to drop or substitute, counted by cause.
///
/// None of these stop the import: an unknown block becomes air and an unknown
/// entity is skipped, because a world with one modded block is still worth
/// having. The counts are reported so nothing disappears silently.
#[derive(Debug, Default, Clone)]
pub struct Issues {
    /// Block ids Foton does not know, replaced by air.
    pub unknown_blocks: BTreeMap<String, u64>,
    /// Blocks whose property names or values Foton rejects; the property was ignored.
    pub rejected_block_properties: BTreeMap<String, u64>,
    /// Biomes Foton does not know, replaced by plains.
    pub unknown_biomes: BTreeMap<String, u64>,
    /// Block entity types Foton does not know, dropped.
    pub unknown_block_entities: BTreeMap<String, u64>,
    /// Block entities whose block does not accept their type, dropped.
    pub misplaced_block_entities: BTreeMap<String, u64>,
    /// Entity types Foton does not know, dropped.
    pub unknown_entities: BTreeMap<String, u64>,
    /// Entities missing a position or an id, dropped.
    pub malformed_entities: u64,
    /// Full chunks vanilla had not lit; they import without light data.
    pub chunks_without_light: u64,
}

impl Issues {
    pub(super) fn tally(map: &mut BTreeMap<String, u64>, key: &str) {
        *map.entry(key.to_owned()).or_default() += 1;
    }

    /// Adds `other`'s counts to these.
    pub fn merge(&mut self, other: Self) {
        for (target, source) in [
            (&mut self.unknown_blocks, other.unknown_blocks),
            (
                &mut self.rejected_block_properties,
                other.rejected_block_properties,
            ),
            (&mut self.unknown_biomes, other.unknown_biomes),
            (
                &mut self.unknown_block_entities,
                other.unknown_block_entities,
            ),
            (
                &mut self.misplaced_block_entities,
                other.misplaced_block_entities,
            ),
            (&mut self.unknown_entities, other.unknown_entities),
        ] {
            for (key, count) in source {
                *target.entry(key).or_default() += count;
            }
        }
        self.malformed_entities += other.malformed_entities;
        self.chunks_without_light += other.chunks_without_light;
    }
}

/// The result of converting one chunk.
pub(super) enum ChunkOutcome {
    /// A chunk ready to be saved as `Full`.
    Converted(Box<ConvertedChunk>),
    /// A chunk vanilla never finished generating; Foton generates it itself.
    NotFull(String),
}

pub(super) struct ConvertedChunk {
    pub(super) persistent: PersistentChunk<'static>,
    pub(super) entity_count: usize,
    pub(super) block_entity_count: usize,
}

/// Chunk-level block-state palette, deduplicated by Foton state id.
#[derive(Default)]
struct StatePalette {
    entries: Vec<PersistentBlockState<'static>>,
    ids: Vec<BlockStateId>,
    lookup: FxHashMap<BlockStateId, u16>,
}

impl StatePalette {
    fn index_of(&mut self, id: BlockStateId) -> u16 {
        if let Some(&index) = self.lookup.get(&id) {
            return index;
        }
        // The id came from the registry a moment ago, so the lookup cannot miss.
        let Some(block) = REGISTRY.blocks.by_state_id(id) else {
            return self.index_of(air_state());
        };
        let index = self.entries.len() as u16;
        self.entries.push(PersistentBlockState {
            name: block.key.clone(),
            properties: REGISTRY.blocks.get_properties(id),
        });
        self.ids.push(id);
        self.lookup.insert(id, index);
        index
    }
}

/// Chunk-level biome palette.
#[derive(Default)]
struct BiomePalette {
    entries: Vec<Identifier>,
    lookup: FxHashMap<Identifier, u16>,
}

impl BiomePalette {
    fn index_of(&mut self, key: Identifier) -> u16 {
        if let Some(&index) = self.lookup.get(&key) {
            return index;
        }
        let index = self.entries.len() as u16;
        self.entries.push(key.clone());
        self.lookup.insert(key, index);
        index
    }

    /// Resolves a vanilla biome id, substituting plains for one Foton lacks.
    fn resolve(&mut self, name: &str, issues: &mut Issues) -> u16 {
        let known = Identifier::from_str(name)
            .ok()
            .filter(|key| REGISTRY.biomes.id_from_key(key).is_some());
        if let Some(key) = known {
            return self.index_of(key);
        }
        Issues::tally(&mut issues.unknown_biomes, name);
        self.index_of(vanilla_biomes::PLAINS.key.clone())
    }
}

fn air_state() -> BlockStateId {
    REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR)
}

/// Resolves one `{Name, Properties}` palette entry the way vanilla's
/// `NbtUtils.readBlockState` does: properties are applied over the default
/// state, and one the block does not have, or a value it does not accept, is
/// ignored rather than failing the block.
fn resolve_block_state(entry: &NbtCompound, issues: &mut Issues) -> BlockStateId {
    let name = entry
        .string("Name")
        .map(|name| name.to_str().into_owned())
        .unwrap_or_default();
    let Ok(key) = Identifier::from_str(&name) else {
        Issues::tally(&mut issues.unknown_blocks, &name);
        return air_state();
    };
    let Some(block) = REGISTRY.blocks.by_key(&key) else {
        Issues::tally(&mut issues.unknown_blocks, &name);
        return air_state();
    };

    let properties: Vec<(String, String)> = entry
        .compound("Properties")
        .map(|compound| {
            compound
                .iter()
                .filter_map(|(property, value)| {
                    let value = value.string()?;
                    Some((property.to_str().into_owned(), value.to_str().into_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    let borrowed: Vec<(&str, &str)> = properties
        .iter()
        .map(|(property, value)| (property.as_str(), value.as_str()))
        .collect();
    if let Some(id) = REGISTRY.blocks.state_id_from_properties(&key, &borrowed) {
        return id;
    }

    Issues::tally(&mut issues.rejected_block_properties, &name);
    let accepted: Vec<(&str, &str)> = borrowed
        .iter()
        .copied()
        .filter(|property| {
            REGISTRY
                .blocks
                .state_id_from_properties(&key, &[*property])
                .is_some()
        })
        .collect();
    REGISTRY
        .blocks
        .state_id_from_properties(&key, &accepted)
        .unwrap_or_else(|| REGISTRY.blocks.get_default_state_id(block))
}

/// `ceil(log2(n))` for `n >= 2`.
const fn ceil_log2(n: usize) -> usize {
    (n - 1).ilog2() as usize + 1
}

/// Unpacks vanilla's `SimpleBitStorage`: since 1.16 an entry never spans two
/// longs, so each long holds `64 / bits` entries and the rest of it is padding.
pub(super) fn unpack_vanilla(
    data: &[i64],
    bits: usize,
    count: usize,
) -> Result<Vec<u32>, ConvertError> {
    if bits == 0 || bits > 32 {
        return Err(malformed(format!("{bits} bits per entry is not possible")));
    }
    let per_long = 64 / bits;
    let expected = count.div_ceil(per_long);
    if data.len() != expected {
        return Err(malformed(format!(
            "packed data has {} longs, {count} entries of {bits} bits need {expected}",
            data.len()
        )));
    }
    let mask = (1u64 << bits) - 1;
    Ok((0..count)
        .map(|index| {
            let word = data[index / per_long] as u64;
            ((word >> ((index % per_long) * bits)) & mask) as u32
        })
        .collect())
}

/// Chunk-palette indices of one section's cells.
enum Cells {
    Uniform(u16),
    Mixed(Box<[u16]>),
}

impl Cells {
    fn from_vec(cells: Vec<u16>) -> Self {
        match cells.first() {
            Some(&first) if cells.iter().all(|&cell| cell == first) => Self::Uniform(first),
            _ => Self::Mixed(cells.into_boxed_slice()),
        }
    }

    fn get(&self, index: usize) -> u16 {
        match self {
            Self::Uniform(value) => *value,
            Self::Mixed(cells) => cells[index],
        }
    }

    /// Section-local palette, bits per entry and packed data, or `None` when
    /// the section holds a single value.
    fn pack(&self) -> Option<(Vec<u16>, u8, Box<[u64]>)> {
        let Self::Mixed(cells) = self else {
            return None;
        };
        let mut palette: Vec<u16> = Vec::new();
        let mut lookup: FxHashMap<u16, u32> = FxHashMap::default();
        let indices: Vec<u32> = cells
            .iter()
            .map(|cell| {
                *lookup.entry(*cell).or_insert_with(|| {
                    palette.push(*cell);
                    (palette.len() - 1) as u32
                })
            })
            .collect();
        let bits = bits_for_palette_len(palette.len())?;
        Some((palette, bits, pack_indices(&indices, bits)))
    }
}

struct ConvertedSection {
    cells: Cells,
    biomes: Cells,
}

fn convert_block_states(
    tag: &NbtCompound,
    palette: &mut StatePalette,
    issues: &mut Issues,
) -> Result<Cells, ConvertError> {
    let entries = tag
        .list("palette")
        .and_then(NbtList::compounds)
        .filter(|entries| !entries.is_empty())
        .ok_or_else(|| malformed("block_states has no palette"))?;
    let mapped: Vec<u16> = entries
        .iter()
        .map(|entry| palette.index_of(resolve_block_state(entry, issues)))
        .collect();
    if let [only] = mapped.as_slice() {
        return Ok(Cells::Uniform(*only));
    }

    let data = tag
        .long_array("data")
        .ok_or_else(|| malformed("block_states has a palette but no data"))?;
    let bits = MIN_BLOCK_BITS.max(ceil_log2(mapped.len()));
    let indices = unpack_vanilla(data, bits, BLOCKS_PER_SECTION)?;
    let cells = indices
        .into_iter()
        .map(|index| {
            mapped
                .get(index as usize)
                .copied()
                .ok_or_else(|| malformed(format!("block index {index} is outside its palette")))
        })
        .collect::<Result<Vec<u16>, _>>()?;
    Ok(Cells::from_vec(cells))
}

fn convert_biomes(
    tag: Option<&NbtCompound>,
    palette: &mut BiomePalette,
    issues: &mut Issues,
) -> Result<Cells, ConvertError> {
    let Some(tag) = tag else {
        return Ok(Cells::Uniform(
            palette.index_of(vanilla_biomes::PLAINS.key.clone()),
        ));
    };
    let names = tag
        .list("palette")
        .and_then(NbtList::strings)
        .filter(|names| !names.is_empty())
        .ok_or_else(|| malformed("biomes has no palette"))?;
    let mapped: Vec<u16> = names
        .iter()
        .map(|name| palette.resolve(&name.to_str(), issues))
        .collect();
    if let [only] = mapped.as_slice() {
        return Ok(Cells::Uniform(*only));
    }

    let data = tag
        .long_array("data")
        .ok_or_else(|| malformed("biomes has a palette but no data"))?;
    let indices = unpack_vanilla(data, ceil_log2(mapped.len()), BIOMES_PER_SECTION)?;
    let cells = indices
        .into_iter()
        .map(|index| {
            mapped
                .get(index as usize)
                .copied()
                .ok_or_else(|| malformed(format!("biome index {index} is outside its palette")))
        })
        .collect::<Result<Vec<u16>, _>>()?;
    Ok(Cells::from_vec(cells))
}

fn section_to_persistent(section: &ConvertedSection) -> PersistentSection {
    let biomes = match section.biomes.pack() {
        Some((palette, bits_per_entry, biome_data)) => PersistentBiomeData::Heterogeneous {
            palette,
            bits_per_entry,
            biome_data,
        },
        None => PersistentBiomeData::Homogeneous {
            biome: section.biomes.get(0),
        },
    };
    match section.cells.pack() {
        Some((palette, bits_per_entry, block_data)) => PersistentSection::Heterogeneous {
            palette,
            bits_per_entry,
            block_data,
            biomes,
        },
        None => PersistentSection::Homogeneous {
            block_state: section.cells.get(0),
            biomes,
        },
    }
}

/// Reads a vanilla scheduled-tick list: `{i, p, t, x, y, z}` with absolute coordinates.
fn convert_ticks(list: Option<&NbtList>, chunk: ChunkPos, shape: Shape) -> Vec<PersistentTick> {
    let Some(ticks) = list.and_then(NbtList::compounds) else {
        return Vec::new();
    };
    ticks
        .iter()
        .filter_map(|tick| {
            let tick_type = Identifier::from_str(&tick.string("i")?.to_str()).ok()?;
            let x = u8::try_from(tick.int("x")? - chunk.0.x * 16)
                .ok()
                .filter(|x| *x < 16)?;
            let z = u8::try_from(tick.int("z")? - chunk.0.y * 16)
                .ok()
                .filter(|z| *z < 16)?;
            let y = tick.int("y")?;
            if y < shape.min_y || y >= shape.min_y + shape.height {
                return None;
            }
            Some(PersistentTick {
                x,
                y: i16::try_from(y).ok()?,
                z,
                delay: tick.int("t").unwrap_or(0),
                priority: i8::try_from(tick.int("p").unwrap_or(0)).unwrap_or(0),
                tick_type,
            })
        })
        .collect()
}

fn light_layer(
    tag: &NbtCompound,
    name: &str,
    section_index: u32,
) -> Option<PersistentLightSection> {
    let data = tag
        .byte_array(name)
        .filter(|data| data.len() == LIGHT_LAYER_BYTES)?;
    Some(PersistentLightSection::Initialized {
        section_index,
        data: data.to_vec(),
    })
}

/// The block sections of a chunk, with the palettes they share and the light
/// that came with them.
struct SectionData {
    sections: Vec<ConvertedSection>,
    states: StatePalette,
    biomes: BiomePalette,
    light: PersistentLightData,
}

fn convert_sections(
    root: &NbtCompound,
    shape: Shape,
    issues: &mut Issues,
) -> Result<SectionData, ConvertError> {
    let section_count = shape.section_count();
    let light_on = root.byte("isLightOn").is_some_and(|flag| flag != 0);
    let mut states = StatePalette::default();
    let mut biomes = BiomePalette::default();
    let mut sections: Vec<Option<ConvertedSection>> = (0..section_count).map(|_| None).collect();
    let mut light = PersistentLightData::default();

    let vanilla_sections = root
        .list("sections")
        .and_then(NbtList::compounds)
        .unwrap_or(&[]);
    for section in vanilla_sections {
        let Some(y) = section.byte("Y").map(i32::from) else {
            continue;
        };
        // Light sections run one below and one above the block sections.
        if light_on
            && let Ok(light_index) = u32::try_from(y - (shape.min_section() - 1))
            && (light_index as usize) < section_count + 2
        {
            light
                .block
                .extend(light_layer(section, "BlockLight", light_index));
            light
                .sky
                .extend(light_layer(section, "SkyLight", light_index));
        }

        let Ok(index) = usize::try_from(y - shape.min_section()) else {
            continue;
        };
        let (Some(slot), Some(block_states)) =
            (sections.get_mut(index), section.compound("block_states"))
        else {
            continue;
        };
        *slot = Some(ConvertedSection {
            cells: convert_block_states(block_states, &mut states, issues)?,
            biomes: convert_biomes(section.compound("biomes"), &mut biomes, issues)?,
        });
    }
    if !light_on {
        issues.chunks_without_light += 1;
    }

    // Vanilla omits sections that hold only air and no light.
    let air = states.index_of(air_state());
    let plains = biomes.index_of(vanilla_biomes::PLAINS.key.clone());
    let sections = sections
        .into_iter()
        .map(|section| {
            section.unwrap_or(ConvertedSection {
                cells: Cells::Uniform(air),
                biomes: Cells::Uniform(plains),
            })
        })
        .collect();
    Ok(SectionData {
        sections,
        states,
        biomes,
        light,
    })
}

/// Pending post-processing offsets per section; empty when none are pending.
fn convert_postprocessing(root: &NbtCompound, section_count: usize) -> Vec<Vec<u16>> {
    let pending: Vec<Vec<u16>> = root
        .list("PostProcessing")
        .and_then(NbtList::lists)
        .map(|lists| {
            lists
                .iter()
                .take(section_count)
                .map(|list| {
                    list.shorts()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|offset| offset as u16)
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default();
    if pending.iter().all(Vec::is_empty) {
        Vec::new()
    } else {
        pending
    }
}

/// Converts one vanilla chunk, and the entities stored beside it, into the
/// chunk Foton would have saved.
///
/// # Errors
/// [`ConvertError::DataVersion`] if the chunk or its entities were written by
/// another version; [`ConvertError::Malformed`] for a damaged chunk or one built
/// for a different world height.
pub(super) fn convert_chunk(
    root: &NbtCompound,
    entity_root: Option<&NbtCompound>,
    pos: ChunkPos,
    shape: Shape,
    last_modified: u32,
    issues: &mut Issues,
) -> Result<ChunkOutcome, ConvertError> {
    let version = root.int("DataVersion");
    if version != Some(WORLD_VERSION) {
        return Err(ConvertError::DataVersion(version));
    }
    if root.int("xPos") != Some(pos.0.x) || root.int("zPos") != Some(pos.0.y) {
        return Err(malformed(format!(
            "stored at ({}, {}) but says it is at ({:?}, {:?})",
            pos.0.x,
            pos.0.y,
            root.int("xPos"),
            root.int("zPos")
        )));
    }
    let status = root
        .string("Status")
        .map(|status| status.to_str().into_owned())
        .unwrap_or_default();
    if status.strip_prefix("minecraft:").unwrap_or(&status) != "full" {
        return Ok(ChunkOutcome::NotFull(status));
    }
    if root.int("yPos") != Some(shape.min_section()) {
        return Err(malformed(format!(
            "starts at section {:?}, this dimension starts at {}",
            root.int("yPos"),
            shape.min_section()
        )));
    }

    let SectionData {
        sections,
        states,
        biomes: biome_palette,
        light,
    } = convert_sections(root, shape, issues)?;

    let state_at = |local_x: usize, y: i32, local_z: usize| -> Option<BlockStateId> {
        let relative = usize::try_from(y - shape.min_y).ok()?;
        let section = sections.get(relative / 16)?;
        let index = (relative % 16) * 256 + local_z * 16 + local_x;
        states
            .ids
            .get(usize::from(section.cells.get(index)))
            .copied()
    };
    let block_entities = convert_block_entities(
        root.list("block_entities").and_then(NbtList::compounds),
        pos,
        shape,
        &state_at,
        issues,
    );
    let entities = match entity_root {
        Some(entity_root) => convert_entities(entity_root, issues)?,
        None => Vec::new(),
    };

    let postprocessing = convert_postprocessing(root, shape.section_count());

    let persistent_sections: Vec<PersistentSection> =
        sections.iter().map(section_to_persistent).collect();
    let entity_count = entities.len();
    let block_entity_count = block_entities.len();
    let persistent = PersistentChunk {
        last_modified,
        block_states: states.entries,
        biomes: biome_palette.entries,
        sections: persistent_sections,
        block_entities,
        entities,
        block_ticks: convert_ticks(root.list("block_ticks"), pos, shape),
        fluid_ticks: convert_ticks(root.list("fluid_ticks"), pos, shape),
        // Rebuilt from the sections on load.
        heightmaps: Vec::new(),
        light,
        carving_mask: None,
        postprocessing,
        structure_starts: Vec::new(),
        structure_references: Vec::new(),
        pois: Vec::new(),
        bukkit_values_nbt: root
            .compound("ChunkBukkitValues")
            .map(ChunkStorage::compound_to_persistent)
            .unwrap_or_default(),
    };
    Ok(ChunkOutcome::Converted(Box::new(ConvertedChunk {
        persistent,
        entity_count,
        block_entity_count,
    })))
}

#[cfg(test)]
mod tests;
