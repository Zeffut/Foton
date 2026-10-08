use std::sync::Arc;

use foton_registry::{REGISTRY, vanilla_blocks};
use foton_utils::{BlockPos, ChunkPos};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

use super::*;
use crate::bootstrap::init_globals_once;
use crate::chunk::full_chunk::FullChunkRef;
use crate::chunk::status::ChunkStatus;
use crate::test_support::fresh_test_world;

const SHAPE: Shape = Shape {
    min_y: -64,
    height: 384,
};

fn compound(entries: Vec<(&str, NbtTag)>) -> NbtCompound {
    let mut compound = NbtCompound::new();
    for (name, tag) in entries {
        compound.insert(name, tag);
    }
    compound
}

/// `insert` appends, so replacing a key means removing it first.
fn replace(compound: &mut NbtCompound, key: &str, tag: NbtTag) {
    let _ = compound.remove(key);
    compound.insert(key, tag);
}

fn strings(values: &[&str]) -> NbtList {
    NbtList::String(values.iter().map(|value| (*value).into()).collect())
}

fn palette_entry(name: &str, properties: &[(&str, &str)]) -> NbtCompound {
    let mut entry = compound(vec![("Name", NbtTag::String(name.into()))]);
    if !properties.is_empty() {
        let properties = compound(
            properties
                .iter()
                .map(|(key, value)| (*key, NbtTag::String((*value).into())))
                .collect(),
        );
        entry.insert("Properties", NbtTag::Compound(properties));
    }
    entry
}

/// Vanilla's `SimpleBitStorage`: entries never span two longs.
fn pack_vanilla(indices: &[u32], bits: usize) -> Vec<i64> {
    let per_long = 64 / bits;
    indices
        .chunks(per_long)
        .map(|word| {
            word.iter().enumerate().fold(0u64, |packed, (slot, index)| {
                packed | (u64::from(*index) << (slot * bits))
            }) as i64
        })
        .collect()
}

fn plains_biomes() -> NbtTag {
    NbtTag::Compound(compound(vec![(
        "palette",
        NbtTag::List(strings(&["minecraft:plains"])),
    )]))
}

fn section(y: i8, palette: Vec<NbtCompound>, indices: Option<Vec<u32>>) -> NbtCompound {
    let bits = MIN_BLOCK_BITS.max(if palette.len() > 1 {
        ceil_log2(palette.len())
    } else {
        0
    });
    let mut block_states = compound(vec![("palette", NbtTag::List(NbtList::Compound(palette)))]);
    if let Some(indices) = indices {
        block_states.insert("data", NbtTag::LongArray(pack_vanilla(&indices, bits)));
    }
    compound(vec![
        ("Y", NbtTag::Byte(y)),
        ("block_states", NbtTag::Compound(block_states)),
        ("biomes", plains_biomes()),
    ])
}

fn chunk_nbt(pos: ChunkPos, sections: Vec<NbtCompound>) -> NbtCompound {
    compound(vec![
        ("DataVersion", NbtTag::Int(WORLD_VERSION)),
        ("xPos", NbtTag::Int(pos.0.x)),
        ("zPos", NbtTag::Int(pos.0.y)),
        ("yPos", NbtTag::Int(-4)),
        ("Status", NbtTag::String("minecraft:full".into())),
        ("isLightOn", NbtTag::Byte(1)),
        ("sections", NbtTag::List(NbtList::Compound(sections))),
    ])
}

fn entity_nbt(pos: ChunkPos, entities: Vec<NbtCompound>) -> NbtCompound {
    compound(vec![
        ("DataVersion", NbtTag::Int(WORLD_VERSION)),
        ("Position", NbtTag::IntArray(vec![pos.0.x, pos.0.y])),
        ("Entities", NbtTag::List(NbtList::Compound(entities))),
    ])
}

fn convert(
    root: &NbtCompound,
    entities: Option<&NbtCompound>,
    pos: ChunkPos,
    issues: &mut Issues,
) -> Result<ChunkOutcome, ConvertError> {
    init_globals_once();
    convert_chunk(root, entities, pos, SHAPE, 1_700_000_000, issues)
}

fn converted(outcome: Result<ChunkOutcome, ConvertError>) -> Box<ConvertedChunk> {
    match outcome {
        Ok(ChunkOutcome::Converted(chunk)) => chunk,
        Ok(ChunkOutcome::NotFull(status)) => panic!("chunk was reported unfinished: {status}"),
        Err(error) => panic!("chunk should convert: {error}"),
    }
}

#[test]
fn vanilla_packing_leaves_the_spare_bits_of_each_long_unused() {
    // 5 bits fit 12 entries in a long, so 4096 entries take 342 longs, not 320.
    let indices: Vec<u32> = (0..4096u32).map(|n| (n * 7) % 32).collect();
    let packed = pack_vanilla(&indices, 5);
    assert_eq!(packed.len(), 342);
    assert_eq!(
        unpack_vanilla(&packed, 5, 4096).expect("matching length"),
        indices
    );

    // The same data read as 4-bit entries is the wrong length, and says so
    // instead of yielding shifted indices.
    assert!(unpack_vanilla(&packed, 4, 4096).is_err());
}

#[test]
fn a_chunk_from_another_version_is_refused() {
    let pos = ChunkPos::new(0, 0);
    let mut issues = Issues::default();

    let mut stale = chunk_nbt(pos, vec![]);
    replace(&mut stale, "DataVersion", NbtTag::Int(WORLD_VERSION - 1));
    assert!(matches!(
        convert(&stale, None, pos, &mut issues),
        Err(ConvertError::DataVersion(Some(found))) if found == WORLD_VERSION - 1
    ));

    let mut unversioned = chunk_nbt(pos, vec![]);
    unversioned.remove("DataVersion");
    assert!(matches!(
        convert(&unversioned, None, pos, &mut issues),
        Err(ConvertError::DataVersion(None))
    ));

    // The entity file is versioned separately and checked the same way.
    let mut old_entities = entity_nbt(pos, vec![]);
    replace(
        &mut old_entities,
        "DataVersion",
        NbtTag::Int(WORLD_VERSION - 100),
    );
    assert!(matches!(
        convert(
            &chunk_nbt(pos, vec![]),
            Some(&old_entities),
            pos,
            &mut issues
        ),
        Err(ConvertError::DataVersion(Some(_)))
    ));
}

#[test]
fn a_chunk_vanilla_never_finished_is_left_to_foton() {
    let pos = ChunkPos::new(2, 3);
    let mut unfinished = chunk_nbt(pos, vec![]);
    replace(
        &mut unfinished,
        "Status",
        NbtTag::String("minecraft:features".into()),
    );
    let mut issues = Issues::default();
    assert!(matches!(
        convert(&unfinished, None, pos, &mut issues),
        Ok(ChunkOutcome::NotFull(status)) if status == "minecraft:features"
    ));
}

#[test]
fn a_chunk_filed_under_the_wrong_coordinates_is_malformed() {
    let mut issues = Issues::default();
    let root = chunk_nbt(ChunkPos::new(1, 1), vec![]);
    assert!(matches!(
        convert(&root, None, ChunkPos::new(0, 0), &mut issues),
        Err(ConvertError::Malformed(_))
    ));
}

#[test]
fn a_dense_section_loads_back_block_for_block() {
    init_globals_once();
    let world = fresh_test_world("anvil_dense_section");
    let pos = ChunkPos::new(0, 0);

    // 40 states, so vanilla stores 6 bits per entry where Foton stores 8.
    let mut palette = vec![palette_entry("minecraft:air", &[])];
    let mut expected_states = vec![REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR)];
    for facing in ["north", "south", "east", "west"] {
        for half in ["top", "bottom"] {
            for shape in [
                "straight",
                "inner_left",
                "inner_right",
                "outer_left",
                "outer_right",
            ] {
                let properties = [
                    ("facing", facing),
                    ("half", half),
                    ("shape", shape),
                    ("waterlogged", "false"),
                ];
                palette.push(palette_entry("minecraft:oak_stairs", &properties));
                expected_states.push(
                    REGISTRY
                        .blocks
                        .state_id_from_properties(
                            &Identifier::vanilla_static("oak_stairs"),
                            &properties,
                        )
                        .expect("stairs state"),
                );
            }
        }
    }
    assert_eq!(palette.len(), 41);
    let indices: Vec<u32> = (0..4096u32)
        .map(|n| (n.wrapping_mul(2_654_435_761)) % 41)
        .collect();

    let root = chunk_nbt(pos, vec![section(0, palette, Some(indices.clone()))]);
    let mut issues = Issues::default();
    let chunk = converted(convert(&root, None, pos, &mut issues));
    assert!(issues.unknown_blocks.is_empty());

    let loaded = ChunkStorage::try_persistent_to_chunk(
        &chunk.persistent,
        pos,
        ChunkStatus::Full,
        SHAPE.min_y,
        SHAPE.height,
        Arc::downgrade(&world),
    )
    .expect("the real loader accepts the converted chunk");

    for (index, palette_index) in indices.iter().enumerate() {
        let (x, z, y) = (
            (index % 16) as i32,
            ((index / 16) % 16) as i32,
            (index / 256) as i32,
        );
        assert_eq!(
            loaded.chunk.get_block_state(BlockPos::new(x, y, z)),
            expected_states[*palette_index as usize],
            "block at {x} {y} {z}"
        );
    }
    // The 23 sections vanilla did not list are air.
    assert_eq!(
        loaded.chunk.get_block_state(BlockPos::new(5, 200, 5)),
        expected_states[0]
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario that needs a whole chunk of content built by hand"
)]
fn block_entities_entities_and_persistent_data_survive_the_real_loader() {
    init_globals_once();
    let world = fresh_test_world("anvil_content");
    let pos = ChunkPos::new(0, 0);

    let chest_pos = BlockPos::new(3, 64, 5);
    let chest = compound(vec![
        ("id", NbtTag::String("minecraft:chest".into())),
        ("x", NbtTag::Int(3)),
        ("y", NbtTag::Int(64)),
        ("z", NbtTag::Int(5)),
        (
            "Items",
            NbtTag::List(NbtList::Compound(vec![compound(vec![
                ("Slot", NbtTag::Byte(0)),
                ("id", NbtTag::String("minecraft:diamond".into())),
                ("count", NbtTag::Int(5)),
            ])])),
        ),
        (
            "PublicBukkitValues",
            NbtTag::Compound(compound(vec![(
                "botw:owner",
                NbtTag::String("alice".into()),
            )])),
        ),
    ]);
    let sign_on_stone = compound(vec![
        ("id", NbtTag::String("minecraft:sign".into())),
        ("x", NbtTag::Int(4)),
        ("y", NbtTag::Int(64)),
        ("z", NbtTag::Int(5)),
    ]);
    let modded = compound(vec![
        ("id", NbtTag::String("modded:gizmo".into())),
        ("x", NbtTag::Int(0)),
        ("y", NbtTag::Int(64)),
        ("z", NbtTag::Int(0)),
    ]);

    // Section 8 (y 64..79): a chest at local (3, 0, 5), stone at local (4, 0, 5).
    let mut indices = vec![0u32; 4096];
    indices[5 * 16 + 3] = 1;
    indices[5 * 16 + 4] = 2;
    let blocks = section(
        4,
        vec![
            palette_entry("minecraft:air", &[]),
            palette_entry("minecraft:chest", &[("facing", "north")]),
            palette_entry("minecraft:stone", &[]),
        ],
        Some(indices),
    );
    let mut root = chunk_nbt(pos, vec![blocks]);
    root.insert(
        "block_entities",
        NbtTag::List(NbtList::Compound(vec![chest, sign_on_stone, modded])),
    );
    root.insert(
        "ChunkBukkitValues",
        NbtTag::Compound(compound(vec![
            ("botw:visited", NbtTag::Byte(1)),
            ("botw:counter", NbtTag::Long(9_000_000_000)),
        ])),
    );

    let stand = |tag: &str, pdc: bool| {
        let mut stand = compound(vec![
            ("id", NbtTag::String("minecraft:armor_stand".into())),
            (
                "UUID",
                NbtTag::IntArray(vec![1, 2, 3, i32::from(tag.len() as u8)]),
            ),
            ("Pos", NbtTag::List(NbtList::Double(vec![3.5, 70.0, 5.5]))),
            ("Tags", NbtTag::List(strings(&[tag, "event"]))),
            ("CustomName", NbtTag::String("Guardian".into())),
        ]);
        if pdc {
            stand.insert(
                "BukkitValues",
                NbtTag::Compound(compound(vec![("botw:team", NbtTag::String("red".into()))])),
            );
        }
        stand
    };
    let mut vehicle = stand("vehicle", true);
    vehicle.insert(
        "Passengers",
        NbtTag::List(NbtList::Compound(vec![stand("rider", false)])),
    );
    let ghost = compound(vec![
        ("id", NbtTag::String("modded:ghost".into())),
        ("Pos", NbtTag::List(NbtList::Double(vec![1.0, 70.0, 1.0]))),
    ]);
    let entities = entity_nbt(pos, vec![vehicle, ghost]);

    let mut issues = Issues::default();
    let chunk = converted(convert(&root, Some(&entities), pos, &mut issues));
    assert_eq!(
        chunk.block_entity_count, 1,
        "only the chest belongs on its block"
    );
    assert_eq!(chunk.entity_count, 1, "the rider travels with its vehicle");
    assert_eq!(issues.unknown_block_entities.get("modded:gizmo"), Some(&1));
    assert_eq!(
        issues.misplaced_block_entities.get("minecraft:sign"),
        Some(&1)
    );
    assert_eq!(issues.unknown_entities.get("modded:ghost"), Some(&1));

    let loaded = ChunkStorage::try_persistent_to_chunk(
        &chunk.persistent,
        pos,
        ChunkStatus::Full,
        SHAPE.min_y,
        SHAPE.height,
        Arc::downgrade(&world),
    )
    .expect("the real loader accepts the converted chunk");

    let pdc = loaded.chunk.bukkit_values();
    assert_eq!(pdc.byte("botw:visited"), Some(1));
    assert_eq!(pdc.long("botw:counter"), Some(9_000_000_000));

    let full = FullChunkRef::from_full_context(&loaded.chunk);
    let Some(chest) = full.get_block_entity(chest_pos) else {
        panic!("the chest should have loaded");
    };
    let saved = chest.save_custom_only();
    assert!(saved.list("Items").is_some());
    assert_eq!(
        saved
            .compound("PublicBukkitValues")
            .and_then(|values| values.string("botw:owner"))
            .map(|owner| owner.to_str().into_owned()),
        Some("alice".to_owned())
    );

    assert_eq!(loaded.pending_entities.len(), 2, "vehicle and rider");
    let tagged = |tag: &str| {
        loaded
            .pending_entities
            .iter()
            .find(|entity| entity.tags().iter().any(|candidate| candidate == tag))
            .map(Arc::clone)
    };
    let Some(vehicle) = tagged("vehicle") else {
        panic!("the vehicle should have loaded");
    };
    assert_eq!(
        vehicle
            .base()
            .save_data()
            .bukkit_values
            .string("botw:team")
            .map(|team| team.to_str().into_owned()),
        Some("red".to_owned())
    );
    assert!(vehicle.base().save_data().custom_name.is_some());
    let Some(rider) = tagged("rider") else {
        panic!("the rider should have loaded");
    };
    assert!(rider.is_passenger());
}

#[test]
fn what_foton_lacks_is_counted_and_never_stops_the_import() {
    let pos = ChunkPos::new(0, 0);
    let palette = vec![
        palette_entry("minecraft:air", &[]),
        palette_entry("modded:marble", &[]),
        // `facing` is valid for stairs but `up` is not one of its values.
        palette_entry("minecraft:oak_stairs", &[("facing", "up"), ("half", "top")]),
    ];
    let indices: Vec<u32> = (0..4096u32).map(|n| n % 3).collect();
    let mut root = chunk_nbt(pos, vec![section(0, palette, Some(indices))]);
    // Two biomes at one bit each fill a single long.
    let biomes = compound(vec![
        (
            "palette",
            NbtTag::List(strings(&["minecraft:plains", "modded:swamp"])),
        ),
        ("data", NbtTag::LongArray(vec![0x5555_5555_5555_5555])),
    ]);
    if let Some(NbtTag::List(NbtList::Compound(sections))) = root.get_mut("sections")
        && let Some(first) = sections.first_mut()
    {
        replace(first, "biomes", NbtTag::Compound(biomes));
    }

    let mut issues = Issues::default();
    let chunk = converted(convert(&root, None, pos, &mut issues));
    assert_eq!(issues.unknown_blocks.get("modded:marble"), Some(&1));
    assert_eq!(
        issues.rejected_block_properties.get("minecraft:oak_stairs"),
        Some(&1)
    );
    assert_eq!(issues.unknown_biomes.get("modded:swamp"), Some(&1));

    // The stairs kept the half vanilla could honor and fell back on the rest.
    let stairs = chunk
        .persistent
        .block_states
        .iter()
        .find(|state| state.name == Identifier::vanilla_static("oak_stairs"))
        .expect("the stairs stay in the palette");
    assert!(stairs.properties.contains(&("half", "top")));
    assert!(!stairs.properties.contains(&("facing", "up")));
}

#[test]
fn light_is_imported_only_when_vanilla_had_lit_the_chunk() {
    let pos = ChunkPos::new(0, 0);
    let with_light = |light_on: bool| {
        let mut lit = section(0, vec![palette_entry("minecraft:air", &[])], None);
        lit.insert("SkyLight", NbtTag::ByteArray(vec![0xFF; 2048]));
        lit.insert("BlockLight", NbtTag::ByteArray(vec![0x11; 2048]));
        // A light-only section below the world, which has no block states.
        let below = compound(vec![
            ("Y", NbtTag::Byte(-5)),
            ("SkyLight", NbtTag::ByteArray(vec![0x22; 2048])),
        ]);
        let mut root = chunk_nbt(pos, vec![lit, below]);
        replace(&mut root, "isLightOn", NbtTag::Byte(i8::from(light_on)));
        root
    };

    let mut issues = Issues::default();
    let lit = converted(convert(&with_light(true), None, pos, &mut issues));
    // Light index 0 is the section under the world, so Y = 0 is index 5.
    let mut sky: Vec<u32> = lit
        .persistent
        .light
        .sky
        .iter()
        .map(PersistentLightSection::section_index)
        .collect();
    sky.sort_unstable();
    assert_eq!(sky, [0, 5]);
    assert_eq!(lit.persistent.light.block.len(), 1);
    assert_eq!(issues.chunks_without_light, 0);

    let dark = converted(convert(&with_light(false), None, pos, &mut issues));
    assert!(dark.persistent.light.sky.is_empty() && dark.persistent.light.block.is_empty());
    assert_eq!(issues.chunks_without_light, 1);
}

#[test]
fn scheduled_ticks_become_chunk_relative_and_strays_are_dropped() {
    let pos = ChunkPos::new(-1, 2);
    let tick = |x: i32, y: i32, z: i32| {
        compound(vec![
            ("i", NbtTag::String("minecraft:sand".into())),
            ("p", NbtTag::Int(-1)),
            ("t", NbtTag::Int(7)),
            ("x", NbtTag::Int(x)),
            ("y", NbtTag::Int(y)),
            ("z", NbtTag::Int(z)),
        ])
    };
    let mut root = chunk_nbt(pos, vec![]);
    root.insert(
        "block_ticks",
        NbtTag::List(NbtList::Compound(vec![
            tick(-3, 70, 35),
            // Outside the chunk, and above the world.
            tick(0, 70, 35),
            tick(-3, 400, 35),
        ])),
    );
    let mut issues = Issues::default();
    let chunk = converted(convert(&root, None, pos, &mut issues));
    let ticks = &chunk.persistent.block_ticks;
    assert_eq!(ticks.len(), 1);
    assert_eq!((ticks[0].x, ticks[0].y, ticks[0].z), (13, 70, 3));
    assert_eq!((ticks[0].delay, ticks[0].priority), (7, -1));
}
