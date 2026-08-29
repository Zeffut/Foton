//! Tests for the map saved data and the color pass that fills it.

use std::sync::Arc;

use foton_registry::blocks::BlockRef;
use foton_registry::data_components::components::MapDecorations;
use foton_registry::map_color::{Brightness, MapColor};
use foton_registry::{
    REGISTRY, init_vanilla_registry, vanilla_blocks,
    vanilla_map_decoration_types as decoration_types,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, ChunkPos, Identifier};
use uuid::Uuid;

use crate::behavior::items::map_item;
use crate::entity::{Entity as _, next_entity_id};
use crate::map::saved_data::{MAP_SIZE, MapPlayerSource, MapPlayerState};
use crate::map::storage::SharedMapData;
use crate::map::{MapItemSavedData, MapStorage};
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use crate::world::World;

/// Where the tests stand their player, and the block column directly below it.
const SPAWN_X: f64 = 8.5;
const SPAWN_Y: f64 = 64.0;
const SPAWN_Z: f64 = 8.5;

/// Nothing else holds this map, so every lookup answers "gone".
struct NoOtherHolders;

impl MapPlayerSource for NoOtherHolders {
    fn holder(&self, _uuid: Uuid) -> Option<MapPlayerState> {
        None
    }
}

/// Answers with exactly the states it was handed.
struct FixedHolders(Vec<MapPlayerState>);

impl MapPlayerSource for FixedHolders {
    fn holder(&self, uuid: Uuid) -> Option<MapPlayerState> {
        self.0.iter().find(|state| state.uuid == uuid).cloned()
    }
}

fn state_of(block: BlockRef) -> BlockStateId {
    REGISTRY.blocks.get_default_state_id(block)
}

fn place(world: &Arc<World>, x: i32, y: i32, z: i32, state: BlockStateId) {
    world.set_block(BlockPos::new(x, y, z), state, UpdateFlags::UPDATE_NONE);
}

/// Fills one column with water down to `depth` blocks, floored with stone.
fn water_column(world: &Arc<World>, x: i32, z: i32, depth: i32) {
    let water = state_of(&vanilla_blocks::WATER);
    let stone = state_of(&vanilla_blocks::STONE);
    let top = 63;
    for y in (top - depth + 1)..=top {
        place(world, x, y, z, water);
    }
    place(world, x, top - depth, z, stone);
}

/// The map pixel a block column at `(x, z)` lands on, at scale zero with the
/// map centered on the origin.
const fn pixel_for(x: i32, z: i32) -> (usize, usize) {
    ((x + 64) as usize, (z + 64) as usize)
}

fn packed(colors: &[u8; MAP_SIZE * MAP_SIZE], (x, y): (usize, usize)) -> (MapColor, Brightness) {
    MapColor::unpack(colors[x + y * MAP_SIZE])
}

fn fresh_map(world: &Arc<World>) -> SharedMapData {
    Arc::new(SyncMutex::new(MapItemSavedData::create_fresh(
        SPAWN_X,
        SPAWN_Z,
        0,
        true,
        false,
        world.key.clone(),
        false,
    )))
}

/// A map made anywhere inside one 128-block cell snaps to the same center, and
/// zooming out re-snaps it to the larger grid. Getting this wrong shifts every
/// pixel of every map by up to half an image.
#[test]
fn a_map_centers_on_the_cell_its_scale_divides_the_world_into() {
    init_vanilla_registry();
    let dimension = Identifier::vanilla_static("overworld");

    let near_origin =
        MapItemSavedData::create_fresh(8.5, 8.5, 0, true, false, dimension.clone(), false);
    assert_eq!((near_origin.center_x, near_origin.center_z), (0, 0));

    let same_cell =
        MapItemSavedData::create_fresh(63.0, -64.0, 0, true, false, dimension.clone(), false);
    assert_eq!((same_cell.center_x, same_cell.center_z), (0, 0));

    let next_cell =
        MapItemSavedData::create_fresh(65.0, 8.5, 0, true, false, dimension.clone(), false);
    assert_eq!((next_cell.center_x, next_cell.center_z), (128, 0));

    // Vanilla parity: `scaled()` re-centers on the coarser grid rather than
    // keeping the old center.
    let zoomed = near_origin.scaled();
    assert_eq!(zoomed.scale, 1);
    assert_eq!((zoomed.center_x, zoomed.center_z), (64, 64));
}

/// The whole point of the item: what the ground is made of has to reach the
/// right pixel in the right shade.
#[test]
fn a_map_over_known_terrain_takes_the_colors_that_terrain_implies() {
    init_vanilla_registry();
    let world = fresh_test_world("map_colors");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    // One stripe of the image is redrawn per pass, picked by `x & 15`; block
    // x = 1 lands in the stripe the first pass draws.
    place(&world, 1, 63, 1, state_of(&vanilla_blocks::STONE));
    place(&world, 1, 63, 2, state_of(&vanilla_blocks::GRASS_BLOCK));
    place(&world, 1, 63, 3, state_of(&vanilla_blocks::OAK_PLANKS));

    let player = TestPlayerBuilder::new(Arc::clone(&world), "MapColors", next_entity_id()).build();
    player
        .try_set_position(glam::DVec3::new(SPAWN_X, SPAWN_Y, SPAWN_Z))
        .expect("test player should be placeable");

    let data = fresh_map(&world);
    map_item::update(&world, &player, &data);

    let map = data.lock();
    let colors = map.colors();
    assert_eq!(packed(colors, pixel_for(1, 1)).0, MapColor::STONE);
    assert_eq!(packed(colors, pixel_for(1, 2)).0, MapColor::GRASS);
    assert_eq!(packed(colors, pixel_for(1, 3)).0, MapColor::WOOD);
}

/// Vanilla shades water by how deep it is, which is the only depth cue a map
/// has. The thresholds are narrow enough that an off-by-one in the downward
/// walk would flip a whole ocean to the wrong shade.
#[test]
fn water_is_drawn_darker_the_deeper_it_gets() {
    init_vanilla_registry();
    let world = fresh_test_world("map_water_depth");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    water_column(&world, 1, 4, 1);
    water_column(&world, 1, 6, 5);
    water_column(&world, 1, 8, 10);

    let player = TestPlayerBuilder::new(Arc::clone(&world), "MapWater", next_entity_id()).build();
    player
        .try_set_position(glam::DVec3::new(SPAWN_X, SPAWN_Y, SPAWN_Z))
        .expect("test player should be placeable");

    let data = fresh_map(&world);
    map_item::update(&world, &player, &data);

    let map = data.lock();
    let colors = map.colors();
    let shallow = packed(colors, pixel_for(1, 4));
    let middling = packed(colors, pixel_for(1, 6));
    let deep = packed(colors, pixel_for(1, 8));

    assert_eq!(shallow.0, MapColor::WATER);
    assert_eq!(middling.0, MapColor::WATER);
    assert_eq!(deep.0, MapColor::WATER);
    assert_eq!(shallow.1, Brightness::High);
    assert_eq!(middling.1, Brightness::Normal);
    assert_eq!(deep.1, Brightness::Low);
}

/// A locked map is frozen: the tick that would redraw it is skipped, and the
/// copy carries the pixels the original had.
#[test]
fn locking_a_map_copies_its_pixels_and_leaves_the_original_alone() {
    init_vanilla_registry();
    let dimension = Identifier::vanilla_static("overworld");
    let mut original = MapItemSavedData::create_fresh(0.0, 0.0, 2, true, false, dimension, false);
    original.set_color(3, 4, MapColor::EMERALD.packed_id(Brightness::Low));

    let locked = original.locked_copy();

    assert!(locked.locked);
    assert!(!original.locked);
    assert_eq!(locked.scale, original.scale);
    assert_eq!(
        locked.colors()[3 + 4 * MAP_SIZE],
        MapColor::EMERALD.packed_id(Brightness::Low)
    );
}

/// The first packet has to carry the whole image, and later ones only the
/// rectangle that moved -- a client that never gets the first full patch shows
/// a blank map forever.
#[test]
fn the_first_update_sends_the_whole_image_and_later_ones_only_what_changed() {
    init_vanilla_registry();
    let uuid = Uuid::from_u128(1);
    let mut map = MapItemSavedData::create_fresh(
        0.0,
        0.0,
        0,
        true,
        false,
        Identifier::vanilla_static("overworld"),
        false,
    );
    map.holding_player_mut(uuid, "Cartographer");

    let first = map
        .update_packet(7, uuid)
        .expect("a new holder is owed the whole image");
    let patch = first.color_patch.expect("the first packet carries pixels");
    assert_eq!((patch.start_x, patch.start_y), (0, 0));
    assert_eq!((patch.width, patch.height), (128, 128));
    assert!(first.decorations.is_some());

    assert!(
        map.update_packet(7, uuid).is_none(),
        "an unchanged map owes nothing"
    );

    map.set_color(10, 20, MapColor::LAPIS.packed_id(Brightness::High));
    let second = map
        .update_packet(7, uuid)
        .expect("a changed pixel is owed to the holder");
    let patch = second.color_patch.expect("the change carries pixels");
    assert_eq!((patch.start_x, patch.start_y), (10, 20));
    assert_eq!((patch.width, patch.height), (1, 1));
    assert_eq!(
        patch.map_colors,
        vec![MapColor::LAPIS.packed_id(Brightness::High)]
    );
}

/// A holder who puts the map away has to lose their arrow, or every player who
/// has ever held the map stays pinned to it.
#[test]
fn a_holder_who_stops_carrying_the_map_loses_their_marker() {
    init_vanilla_registry();
    let dimension = Identifier::vanilla_static("overworld");
    let mut map =
        MapItemSavedData::create_fresh(0.0, 0.0, 0, true, false, dimension.clone(), false);

    let ticking = MapPlayerState {
        uuid: Uuid::from_u128(1),
        name: "Holder".to_owned(),
        x: 0.0,
        z: 0.0,
        y_rot: 0.0,
        dimension: dimension.clone(),
        holds_map: true,
        map_invisible: false,
    };
    let other = MapPlayerState {
        uuid: Uuid::from_u128(2),
        name: "Other".to_owned(),
        x: 16.0,
        z: 16.0,
        y_rot: 90.0,
        dimension,
        holds_map: true,
        map_invisible: false,
    };

    map.holding_player_mut(other.uuid, &other.name);
    let holders = FixedHolders(vec![other.clone()]);
    map.tick_carried_by(&ticking, &holders, None, &MapDecorations::EMPTY, 0);
    assert_eq!(map.decorations().count(), 2);

    let empty_handed = MapPlayerState {
        holds_map: false,
        ..other
    };
    let holders = FixedHolders(vec![empty_handed]);
    map.tick_carried_by(&ticking, &holders, None, &MapDecorations::EMPTY, 0);

    let remaining: Vec<_> = map.decorations().collect();
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0].decoration_type.key,
        decoration_types::PLAYER.key
    );
}

/// A holder standing off the edge of the map gets the flat off-map icon rather
/// than a marker clamped to the border, which is how a player tells "somewhere
/// that way" from "right here".
#[test]
fn a_holder_outside_the_image_is_drawn_with_the_off_map_icon() {
    init_vanilla_registry();
    let dimension = Identifier::vanilla_static("overworld");
    let mut map =
        MapItemSavedData::create_fresh(0.0, 0.0, 0, true, false, dimension.clone(), false);

    let far_away = MapPlayerState {
        uuid: Uuid::from_u128(1),
        name: "FarAway".to_owned(),
        x: 200.0,
        z: 0.0,
        y_rot: 0.0,
        dimension,
        holds_map: true,
        map_invisible: false,
    };
    map.tick_carried_by(&far_away, &NoOtherHolders, None, &MapDecorations::EMPTY, 0);

    let drawn: Vec<_> = map.decorations().collect();
    assert_eq!(drawn.len(), 1);
    assert_eq!(
        drawn[0].decoration_type.key,
        decoration_types::PLAYER_OFF_MAP.key
    );
}

/// Maps survive a restart, and the id counter survives with them -- a counter
/// that reset would hand out an id an existing map already owns.
#[test]
fn a_domains_maps_and_id_counter_round_trip_through_storage() {
    init_vanilla_registry();
    let storage = MapStorage::new();
    let dimension = Identifier::new_static("foton", "overworld");

    let first = storage.next_id();
    let second = storage.next_id();
    assert_eq!((first.id(), second.id()), (0, 1));

    let mut data =
        MapItemSavedData::create_fresh(0.0, 0.0, 1, true, false, dimension.clone(), true);
    data.set_color(5, 6, MapColor::PODZOL.packed_id(Brightness::Lowest));
    storage.set(second, data);

    let restored = MapStorage::round_trip_for_tests(&storage);

    assert_eq!(restored.next_id().id(), 2);
    let map = restored
        .get(second)
        .expect("the stored map should come back");
    let map = map.lock();
    assert_eq!(map.scale, 1);
    assert_eq!(map.dimension, dimension);
    assert!(map.nether());
    assert_eq!(
        map.colors()[5 + 6 * MAP_SIZE],
        MapColor::PODZOL.packed_id(Brightness::Lowest)
    );
    assert!(
        !map.is_dirty(),
        "a map read back from disk has nothing to write"
    );
}

/// The extracted per-state colors are what every pixel is drawn from, so a
/// wrong state offset would recolor whole classes of block.
#[test]
fn extracted_block_map_colors_match_vanilla() {
    init_vanilla_registry();
    for (block, expected) in [
        (&vanilla_blocks::AIR, MapColor::NONE),
        (&vanilla_blocks::STONE, MapColor::STONE),
        (&vanilla_blocks::GRASS_BLOCK, MapColor::GRASS),
        (&vanilla_blocks::WATER, MapColor::WATER),
        (&vanilla_blocks::LAVA, MapColor::FIRE),
        (&vanilla_blocks::OAK_PLANKS, MapColor::WOOD),
        (&vanilla_blocks::SAND, MapColor::SAND),
        (&vanilla_blocks::DEEPSLATE, MapColor::DEEPSLATE),
    ] {
        use foton_registry::blocks::block_state_ext::BlockStateExt as _;
        assert_eq!(
            state_of(block).get_map_color(),
            expected,
            "wrong map color for {}",
            block.key
        );
    }
}
