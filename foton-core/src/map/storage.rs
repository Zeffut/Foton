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
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

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
    /// Moves when the id counter or map collection changes; individual maps
    /// carry their own dirty flag.
    layout_revision: AtomicU64,
    saved_layout_revision: AtomicU64,
}

struct MapStorageSaveSnapshot {
    layout_revision: u64,
    map_revisions: Vec<(i32, u64)>,
    state: PersistentMaps,
}

impl MapStorage {
    /// Creates empty map storage for a domain that has never made one.
    #[must_use]
    pub fn new() -> Self {
        Self {
            maps: SyncRwLock::new(FxHashMap::default()),
            last_id: AtomicI32::new(-1),
            layout_revision: AtomicU64::new(0),
            saved_layout_revision: AtomicU64::new(0),
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
        self.layout_revision.fetch_add(1, Ordering::Release);
        shared
    }

    /// Vanilla parity: `ServerLevel.getFreeMapId` via `MapIndex.getNextMapId`.
    pub fn next_id(&self) -> MapId {
        let id = self.last_id.fetch_add(1, Ordering::AcqRel) + 1;
        self.layout_revision.fetch_add(1, Ordering::Release);
        MapId::new(id)
    }

    fn pending_save(&self) -> Option<MapStorageSaveSnapshot> {
        let layout_revision = self.layout_revision.load(Ordering::Acquire);
        if layout_revision == self.saved_layout_revision.load(Ordering::Acquire)
            && !self.maps.read().values().any(|map| map.lock().is_dirty())
        {
            return None;
        }
        Some(self.snapshot())
    }

    fn snapshot(&self) -> MapStorageSaveSnapshot {
        let maps = self.maps.read();
        let mut entries: Vec<(PersistentMap, u64)> = maps
            .iter()
            .map(|(id, map)| {
                let map = map.lock();
                (persist_map(*id, &map), map.persistence_revision())
            })
            .collect();
        entries.sort_unstable_by_key(|(entry, _revision)| entry.id);
        let map_revisions = entries
            .iter()
            .map(|(entry, revision)| (entry.id, *revision))
            .collect();
        MapStorageSaveSnapshot {
            layout_revision: self.layout_revision.load(Ordering::Acquire),
            map_revisions,
            state: PersistentMaps {
                last_id: self.last_id.load(Ordering::Acquire),
                maps: entries
                    .into_iter()
                    .map(|(entry, _revision)| entry)
                    .collect(),
            },
        }
    }

    fn mark_saved(&self, snapshot: &MapStorageSaveSnapshot) {
        self.saved_layout_revision
            .fetch_max(snapshot.layout_revision, Ordering::Release);

        let maps = self.maps.read();
        for (id, revision) in &snapshot.map_revisions {
            let Some(map) = maps.get(id) else {
                continue;
            };
            map.lock().mark_saved(*revision);
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
        let bytes = wincode::serialize(&self.snapshot().state).expect("map snapshot should encode");
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
            layout_revision: AtomicU64::new(0),
            saved_layout_revision: AtomicU64::new(0),
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
            let Some(snapshot) = storage.pending_save() else {
                continue;
            };
            let world = domain_default_world(worlds, domain)?;
            world
                .saved_data
                .sync_save_wincode(saved_data_names::MAPS, &snapshot.state)
                .map_err(|error| map_io_error(domain, error))?;
            storage.mark_saved(&snapshot);
            saved += 1;
        }
        Ok(saved)
    }
}

fn domain_default_world(worlds: &WorldMap, domain: &str) -> io::Result<Arc<World>> {
    worlds.default_world(domain).ok_or_else(|| {
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
        dimension: map.dimension().to_string(),
        nether: map.nether(),
        center_x: map.center_x(),
        center_z: map.center_z(),
        scale: map.scale(),
        tracking_position: map.tracking_position(),
        unlimited_tracking: map.unlimited_tracking(),
        locked: map.locked(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_map() -> MapItemSavedData {
        MapItemSavedData::create_fresh(
            0.0,
            0.0,
            0,
            true,
            false,
            Identifier::vanilla_static("overworld"),
            false,
        )
    }

    #[test]
    fn map_mutation_after_snapshot_remains_dirty() {
        let storage = MapStorage::new();
        let id = MapId::new(0);
        let map = storage.set(id, test_map());
        let snapshot = storage.snapshot();

        map.lock().set_color(0, 0, 1);
        storage.mark_saved(&snapshot);

        assert!(
            storage.pending_save().is_some(),
            "a map changed after the saved snapshot must remain dirty"
        );
    }

    #[test]
    fn unchanged_snapshot_is_acknowledged() {
        let storage = MapStorage::new();
        let id = MapId::new(0);
        storage.set(id, test_map());
        let snapshot = storage.snapshot();

        storage.mark_saved(&snapshot);

        assert!(
            storage.pending_save().is_none(),
            "an unchanged snapshot must clear the map's dirty state"
        );
    }

    #[test]
    fn persisted_field_mutation_after_snapshot_remains_dirty() {
        let storage = MapStorage::new();
        let id = MapId::new(0);
        let map = storage.set(id, test_map());
        let snapshot = storage.snapshot();

        map.lock().set_locked(true);
        storage.mark_saved(&snapshot);

        assert!(
            storage.pending_save().is_some(),
            "a persisted field changed after the saved snapshot must remain dirty"
        );
    }

    #[test]
    fn map_aba_mutation_after_snapshot_remains_dirty() {
        let storage = MapStorage::new();
        let id = MapId::new(0);
        let map = storage.set(id, test_map());
        let snapshot = storage.snapshot();

        {
            let mut map = map.lock();
            map.set_color(0, 0, 1);
            map.set_color(0, 0, 0);
        }
        storage.mark_saved(&snapshot);

        assert!(
            storage.pending_save().is_some(),
            "a map changed and restored after the saved snapshot must remain dirty"
        );
    }

    #[test]
    fn layout_mutation_after_snapshot_remains_dirty() {
        let storage = MapStorage::new();
        let _first = storage.next_id();
        let snapshot = storage.snapshot();

        let _second = storage.next_id();
        storage.mark_saved(&snapshot);

        let Some(pending) = storage.pending_save() else {
            panic!("an id allocated after the saved snapshot must remain dirty");
        };
        assert!(pending.layout_revision > snapshot.layout_revision);
    }
}
