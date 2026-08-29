//! Where a domain's filled maps live and how they reach disk.
//!
//! Vanilla keeps every map in the server's single `SavedDataStorage`, one
//! `data/maps/<id>.dat` per map plus a `data/maps/last_id.dat` counter, so a
//! map made in one dimension is still readable from another. Foton's saved
//! data is per world and its names are `&'static str`, so all of a domain's
//! maps share one `data/maps.bin` written through the domain's default world --
//! the same boundary the scoreboard and the command storage already use.

use std::collections::BTreeMap;
use std::io::{self, Cursor};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use foton_registry::data_components::components::MapId;
use foton_registry::dye_color::DyeColor;
use foton_utils::locks::{AsyncMutex, SyncMutex, SyncRwLock};
use foton_utils::saved_data::names as saved_data_names;
use foton_utils::{BlockPos, Identifier};
use rustc_hash::FxHashMap;
use simdnbt::borrow::read_tag;
use simdnbt::owned::NbtTag;
use text_components::TextComponent;
use wincode::{SchemaRead, SchemaWrite};

use crate::map::markers::{MapBanner, MapFrame};
use crate::map::saved_data::MapItemSavedData;
use crate::server::worlds::WorldMap;
use crate::world::World;

/// A map behind its own lock, shared by every holder being ticked.
pub type SharedMapData = Arc<SyncMutex<MapItemSavedData>>;

/// Every filled map of one Foton domain.
#[derive(Debug, Default)]
pub struct MapStorage {
    maps: SyncRwLock<FxHashMap<i32, SharedMapData>>,
    /// Vanilla parity: `MapIndex.lastMapId`, which starts one below the first
    /// id it will hand out.
    last_id: AtomicI32,
    /// Set when the id counter moves or a map is added; individual maps carry
    /// their own dirty flag.
    layout_dirty: SyncRwLock<bool>,
}

impl MapStorage {
    /// Creates empty map storage for a domain that has never made one.
    #[must_use]
    pub fn new() -> Self {
        Self {
            maps: SyncRwLock::new(FxHashMap::default()),
            last_id: AtomicI32::new(-1),
            layout_dirty: SyncRwLock::new(false),
        }
    }

    /// Vanilla parity: `ServerLevel.getMapData`.
    #[must_use]
    pub fn get(&self, id: MapId) -> Option<SharedMapData> {
        self.maps.read().get(&id.id()).map(Arc::clone)
    }

    /// Vanilla parity: `ServerLevel.setMapData`.
    pub fn set(&self, id: MapId, data: MapItemSavedData) -> SharedMapData {
        let shared = Arc::new(SyncMutex::new(data));
        self.maps.write().insert(id.id(), Arc::clone(&shared));
        *self.layout_dirty.write() = true;
        shared
    }

    /// Vanilla parity: `ServerLevel.getFreeMapId` via `MapIndex.getNextMapId`.
    pub fn next_id(&self) -> MapId {
        let id = self.last_id.fetch_add(1, Ordering::AcqRel) + 1;
        *self.layout_dirty.write() = true;
        MapId::new(id)
    }

    fn pending_save(&self) -> bool {
        if *self.layout_dirty.read() {
            return true;
        }
        self.maps.read().values().any(|map| map.lock().is_dirty())
    }

    fn snapshot(&self) -> PersistentMaps {
        let maps = self.maps.read();
        let mut entries: Vec<PersistentMap> = maps
            .iter()
            .map(|(id, map)| persist_map(*id, &map.lock()))
            .collect();
        entries.sort_unstable_by_key(|entry| entry.id);
        PersistentMaps {
            last_id: self.last_id.load(Ordering::Acquire),
            maps: entries,
        }
    }

    fn mark_saved(&self) {
        *self.layout_dirty.write() = false;
        for map in self.maps.read().values() {
            map.lock().mark_saved();
        }
    }

    /// Encodes and decodes this storage the way a restart would.
    ///
    /// # Panics
    /// Panics if the snapshot does not survive its own encoder, which is the
    /// point of the test that calls this.
    #[cfg(test)]
    #[must_use]
    pub fn round_trip_for_tests(&self) -> Self {
        let bytes = wincode::serialize(&self.snapshot()).expect("map snapshot should encode");
        let persistent = wincode::deserialize_exact(&bytes).expect("map snapshot should decode");
        Self::from_persistent(persistent)
    }

    fn from_persistent(persistent: PersistentMaps) -> Self {
        let mut maps = FxHashMap::default();
        for entry in persistent.maps {
            let id = entry.id;
            maps.insert(id, Arc::new(SyncMutex::new(restore_map(entry))));
        }
        Self {
            maps: SyncRwLock::new(maps),
            last_id: AtomicI32::new(persistent.last_id),
            layout_dirty: SyncRwLock::new(false),
        }
    }
}

/// One `MapStorage` per Foton domain, mirroring `DomainScoreboards`.
#[derive(Debug)]
pub struct DomainMapData {
    domains: BTreeMap<String, Arc<MapStorage>>,
    save_lock: AsyncMutex<()>,
}

impl DomainMapData {
    /// Reads every domain's maps through its default world.
    pub fn load(worlds: &WorldMap) -> io::Result<Self> {
        let mut domain_names = worlds.domain_names().collect::<Vec<_>>();
        domain_names.sort_unstable();
        let mut domains = BTreeMap::new();
        for domain in domain_names {
            let world = domain_default_world(worlds, domain)?;
            let persistent: Option<PersistentMaps> = world
                .saved_data
                .sync_load_wincode(saved_data_names::MAPS)
                .map_err(|error| map_io_error(domain, error))?;
            let storage = persistent.map_or_else(MapStorage::new, MapStorage::from_persistent);
            domains.insert(domain.to_owned(), Arc::new(storage));
        }
        Ok(Self {
            domains,
            save_lock: AsyncMutex::new(()),
        })
    }

    /// Returns the map storage a world's domain shares.
    #[must_use]
    pub fn for_world(&self, world: &World) -> Option<&Arc<MapStorage>> {
        self.domains.get(world.domain())
    }

    /// Returns the map storage of a named domain.
    #[must_use]
    pub fn get(&self, domain: &str) -> Option<&Arc<MapStorage>> {
        self.domains.get(domain)
    }

    /// Writes every domain whose maps changed, returning how many were written.
    pub async fn save(&self, worlds: &WorldMap) -> io::Result<usize> {
        let _save_guard = self.save_lock.lock().await;
        let mut saved = 0;
        for (domain, storage) in &self.domains {
            if !storage.pending_save() {
                continue;
            }
            let world = domain_default_world(worlds, domain)?;
            let snapshot = storage.snapshot();
            world
                .saved_data
                .sync_save_wincode(saved_data_names::MAPS, &snapshot)
                .map_err(|error| map_io_error(domain, error))?;
            storage.mark_saved();
            saved += 1;
        }
        Ok(saved)
    }
}

fn domain_default_world<'a>(worlds: &'a WorldMap, domain: &str) -> io::Result<&'a World> {
    worlds
        .default_world(domain)
        .map(AsRef::as_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("domain '{domain}' has no loaded default world"),
            )
        })
}

fn map_io_error(domain: &str, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("map data for domain '{domain}': {error}"),
    )
}

#[derive(Debug, SchemaWrite, SchemaRead)]
struct PersistentMaps {
    last_id: i32,
    maps: Vec<PersistentMap>,
}

#[derive(Debug, SchemaWrite, SchemaRead)]
struct PersistentMap {
    id: i32,
    dimension: String,
    nether: bool,
    center_x: i32,
    center_z: i32,
    scale: u8,
    tracking_position: bool,
    unlimited_tracking: bool,
    locked: bool,
    colors: Vec<u8>,
    banners: Vec<PersistentMapBanner>,
    frames: Vec<PersistentMapFrame>,
}

#[derive(Debug, SchemaWrite, SchemaRead)]
struct PersistentMapBanner {
    pos: [i32; 3],
    /// `DyeColor.serialized_name`, the form vanilla's codec writes.
    color: String,
    /// The banner's custom name as network NBT, absent when it has none.
    name: Option<Vec<u8>>,
}

#[derive(Debug, SchemaWrite, SchemaRead)]
struct PersistentMapFrame {
    pos: [i32; 3],
    rotation: i32,
    entity_id: i32,
}

fn persist_map(id: i32, map: &MapItemSavedData) -> PersistentMap {
    PersistentMap {
        id,
        dimension: map.dimension.to_string(),
        nether: map.nether(),
        center_x: map.center_x,
        center_z: map.center_z,
        scale: map.scale,
        tracking_position: map.tracking_position(),
        unlimited_tracking: map.unlimited_tracking(),
        locked: map.locked,
        colors: map.colors().to_vec(),
        banners: map
            .banners()
            .map(|banner| PersistentMapBanner {
                pos: [banner.pos.x(), banner.pos.y(), banner.pos.z()],
                color: banner.color.serialized_name().to_owned(),
                name: banner.name.as_ref().map(component_to_nbt_bytes),
            })
            .collect(),
        frames: map
            .frames()
            .map(|frame| PersistentMapFrame {
                pos: [frame.pos.x(), frame.pos.y(), frame.pos.z()],
                rotation: frame.rotation,
                entity_id: frame.entity_id,
            })
            .collect(),
    }
}

fn restore_map(entry: PersistentMap) -> MapItemSavedData {
    let dimension = entry
        .dimension
        .parse::<Identifier>()
        .unwrap_or_else(|_| Identifier::vanilla_static("overworld"));
    let banners = entry
        .banners
        .into_iter()
        .filter_map(|banner| {
            let color = DyeColor::from_serialized_name(&banner.color)?;
            Some(MapBanner::new(
                BlockPos::new(banner.pos[0], banner.pos[1], banner.pos[2]),
                color,
                banner.name.as_deref().and_then(component_from_nbt_bytes),
            ))
        })
        .collect();
    let frames = entry
        .frames
        .into_iter()
        .map(|frame| {
            MapFrame::new(
                BlockPos::new(frame.pos[0], frame.pos[1], frame.pos[2]),
                frame.rotation,
                frame.entity_id,
            )
        })
        .collect();

    MapItemSavedData::from_persisted(
        dimension,
        entry.nether,
        entry.center_x,
        entry.center_z,
        entry.scale,
        &entry.colors,
        entry.tracking_position,
        entry.unlimited_tracking,
        entry.locked,
        banners,
        frames,
    )
}

fn component_to_nbt_bytes(component: &TextComponent) -> Vec<u8> {
    let mut bytes = Vec::new();
    component.clone().to_codec_nbt().write(&mut bytes);
    bytes
}

fn component_from_nbt_bytes(bytes: &[u8]) -> Option<TextComponent> {
    let tag = read_tag(&mut Cursor::new(bytes)).ok()?;
    let owned: NbtTag = tag.as_tag().to_owned();
    TextComponent::from_nbt(&owned)
}
