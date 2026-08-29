//! World-level POI storage manager.
//!
//! Tracks special blocks (beds, workstations, bells, nether portals, etc.)
//! so game systems can efficiently query for nearby points of interest
//! without scanning every block. Organized by chunk column for efficient
//! load/unload and spatial queries.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.village.poi.PoiManager`.
//! Vanilla is a `SectionStorage` that loads and unloads POI sections from the
//! `poi/` region files independently of the chunks they describe; Foton rebuilds
//! a column's POIs from its block states when the chunk loads
//! ([`PointOfInterestStorage::scan_and_populate`]) and only persists the ticket
//! counts, so every query here sees exactly the loaded columns.

use foton_registry::{REGISTRY, RegistryExt, TaggedRegistryExt, vanilla_poi_type_tags::PoiTag};
use foton_utils::{BlockPos, BlockStateId, ChunkPos, PackedSectionBlockPos, SectionPos};
use rand::RngExt;
use rustc_hash::FxHashMap;

use super::poi_instance::PointOfInterest;
use super::poi_set::PointOfInterestSet;
use crate::chunk::section::ChunkSection;

/// Section distance past which a position no longer counts as being in a village.
///
/// Vanilla parity: `PoiManager.MAX_VILLAGE_DISTANCE`.
pub const MAX_VILLAGE_DISTANCE: i32 = 6;

/// The distance [`PointOfInterestStorage::sections_to_village`] reports when no
/// village center is within [`MAX_VILLAGE_DISTANCE`].
///
/// Vanilla parity: the `defaultReturnValue((byte)7)` of `PoiManager.DistanceTracker`.
pub const NO_VILLAGE_DISTANCE: i32 = MAX_VILLAGE_DISTANCE + 1;

/// Filter for POI queries based on ticket availability.
///
/// Vanilla parity: `PoiManager.Occupancy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccupationStatus {
    /// Only POIs with at least one free ticket.
    Free,
    /// Only POIs with zero free tickets.
    Occupied,
    /// All POIs regardless of ticket status.
    Any,
}

impl OccupationStatus {
    /// Returns `true` if the given POI matches this status filter.
    #[must_use]
    pub const fn matches(self, poi: &PointOfInterest, max_tickets: u32) -> bool {
        match self {
            Self::Any => true,
            Self::Free => poi.has_space(),
            Self::Occupied => poi.is_occupied(max_tickets),
        }
    }
}

/// Column of POI sets indexed by section Y coordinate.
type PoiColumn = FxHashMap<i32, PointOfInterestSet>;

/// World-level storage for all points of interest.
///
/// Organized as a two-level map: `ChunkPos -> section_y -> PointOfInterestSet`.
/// This structure mirrors chunk lifecycle (load/unload per column) and provides
/// efficient spatial queries by narrowing to relevant columns first.
pub struct PointOfInterestStorage {
    columns: FxHashMap<ChunkPos, PoiColumn>,
}

impl Default for PointOfInterestStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
const fn resolve_pos(pos: BlockPos) -> (ChunkPos, i32, PackedSectionBlockPos) {
    let section_pos = SectionPos::from_block_pos(pos);
    let chunk_pos = ChunkPos::new(section_pos.x(), section_pos.z());
    let packed = PackedSectionBlockPos::from_block_pos(pos);
    (chunk_pos, section_pos.y(), packed)
}

fn max_tickets_for(type_id: usize) -> u32 {
    REGISTRY
        .poi_types
        .by_id(type_id)
        .map_or(0, |t| t.ticket_count)
}

/// Vanilla parity: `BlockPos.distSqr(Vec3i)`, which is exact for two block positions.
fn distance_sq(a: BlockPos, b: BlockPos) -> i64 {
    let dx = i64::from(a.0.x - b.0.x);
    let dy = i64::from(a.0.y - b.0.y);
    let dz = i64::from(a.0.z - b.0.z);
    dx * dx + dy * dy + dz * dz
}

/// Returns `true` if the POI type is tagged `#minecraft:village`.
///
/// Vanilla parity: the `e.is(PoiTypeTags.VILLAGE)` predicate of
/// `PoiManager.isVillageCenter`.
pub(crate) fn is_village_type(type_id: usize) -> bool {
    REGISTRY
        .poi_types
        .by_id(type_id)
        .is_some_and(|poi_type| REGISTRY.poi_types.is_in_tag(poi_type, &PoiTag::VILLAGE))
}

/// Vanilla parity: `Util.shuffle`, the downward Fisher-Yates `toShuffledList` uses.
fn vanilla_shuffle<T>(items: &mut [T], rng: &mut impl rand::Rng) {
    for i in (2..=items.len()).rev() {
        let swap_to = rng.random_range(0..i);
        items.swap(i - 1, swap_to);
    }
}

impl PointOfInterestStorage {
    /// Creates an empty POI storage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            columns: FxHashMap::default(),
        }
    }

    fn get_or_create_set(
        &mut self,
        chunk_pos: ChunkPos,
        section_y: i32,
    ) -> &mut PointOfInterestSet {
        self.columns
            .entry(chunk_pos)
            .or_default()
            .entry(section_y)
            .or_default()
    }

    /// Adds a POI at the given block position.
    ///
    /// Vanilla parity: `PoiManager.add`.
    pub fn add(&mut self, pos: BlockPos, poi_type_id: usize, max_tickets: u32) {
        let (chunk_pos, section_y, packed) = resolve_pos(pos);
        let set = self.get_or_create_set(chunk_pos, section_y);
        set.add(packed, PointOfInterest::new(pos, poi_type_id, max_tickets));
    }

    /// Removes the POI at the given block position.
    ///
    /// Vanilla parity: `PoiManager.remove`.
    pub fn remove(&mut self, pos: BlockPos) {
        let (chunk_pos, section_y, packed) = resolve_pos(pos);
        let Some(column) = self.columns.get_mut(&chunk_pos) else {
            return;
        };
        let Some(set) = column.get_mut(&section_y) else {
            return;
        };

        set.remove(packed);
        if set.is_empty() {
            column.remove(&section_y);
            if column.is_empty() {
                self.columns.remove(&chunk_pos);
            }
        }
    }

    /// Returns the POI type ID at the given position, if any.
    ///
    /// Vanilla parity: `PoiManager.getType`.
    #[must_use]
    pub fn get_type(&self, pos: BlockPos) -> Option<usize> {
        let (chunk_pos, section_y, packed) = resolve_pos(pos);
        self.columns
            .get(&chunk_pos)?
            .get(&section_y)?
            .get(packed)
            .map(|poi| poi.poi_type_id)
    }

    /// Returns `true` if a POI matching `type_predicate` sits at `pos`.
    ///
    /// Vanilla parity: `PoiManager.exists`, and with an equality predicate,
    /// `PoiManager.existsAtPosition`.
    #[must_use]
    pub fn exists(&self, pos: BlockPos, type_predicate: &impl Fn(usize) -> bool) -> bool {
        self.get_type(pos).is_some_and(type_predicate)
    }

    /// Returns `true` if the POI at the given position has all tickets reserved.
    #[must_use]
    pub fn is_occupied(&self, pos: BlockPos) -> bool {
        let (chunk_pos, section_y, packed) = resolve_pos(pos);
        let Some(column) = self.columns.get(&chunk_pos) else {
            return false;
        };
        let Some(set) = column.get(&section_y) else {
            return false;
        };
        let Some(poi) = set.get(packed) else {
            return false;
        };
        poi.is_occupied(max_tickets_for(poi.poi_type_id))
    }

    /// Reserves a ticket at the given position. Returns `true` if successful.
    ///
    /// Vanilla parity: `PoiRecord.acquireTicket`.
    #[must_use]
    pub fn reserve_ticket(&mut self, pos: BlockPos) -> bool {
        let (chunk_pos, section_y, packed) = resolve_pos(pos);
        let Some(set) = self
            .columns
            .get_mut(&chunk_pos)
            .and_then(|c| c.get_mut(&section_y))
        else {
            return false;
        };
        let Some(poi) = set.get_mut(packed) else {
            return false;
        };
        poi.reserve_ticket()
    }

    /// Releases a ticket at the given position. Returns `true` if successful.
    ///
    /// Vanilla parity: `PoiManager.release`. Vanilla throws when no POI is
    /// registered at `pos`; a released job site can legitimately have been
    /// mined between the memory being written and the release, so Foton reports
    /// that as `false` instead of killing the tick.
    #[must_use]
    pub fn release_ticket(&mut self, pos: BlockPos) -> bool {
        let (chunk_pos, section_y, packed) = resolve_pos(pos);
        let Some(set) = self
            .columns
            .get_mut(&chunk_pos)
            .and_then(|c| c.get_mut(&section_y))
        else {
            return false;
        };
        let Some(poi) = set.get_mut(packed) else {
            return false;
        };
        poi.release_ticket(max_tickets_for(poi.poi_type_id))
    }

    /// Returns all matching POIs in a specific chunk column, lowest section first.
    ///
    /// Vanilla parity: `PoiManager.getInChunk`.
    #[must_use]
    pub fn get_in_chunk(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        chunk_pos: ChunkPos,
        status: OccupationStatus,
    ) -> Vec<(BlockPos, usize)> {
        let mut results = Vec::new();
        self.visit_chunk(type_predicate, chunk_pos, status, &mut |poi| {
            results.push((poi.pos, poi.poi_type_id));
        });
        results
    }

    /// Walks one column's matching POIs in vanilla order: sections bottom-up,
    /// then by section-relative position.
    ///
    /// Vanilla walks `byType`, a `HashMap`, so its within-section order is not
    /// defined; Foton sorts instead so `find`, `take` and every distance tie
    /// resolve the same way on every run and after every reload.
    fn visit_chunk(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        chunk_pos: ChunkPos,
        status: OccupationStatus,
        visit: &mut impl FnMut(&PointOfInterest),
    ) {
        let Some(column) = self.columns.get(&chunk_pos) else {
            return;
        };

        let mut section_ys: Vec<i32> = column.keys().copied().collect();
        section_ys.sort_unstable();

        for section_y in section_ys {
            let Some(set) = column.get(&section_y) else {
                continue;
            };
            for poi in set.get_matching(type_predicate, status, &max_tickets_for) {
                visit(poi);
            }
        }
    }

    /// Walks every matching POI in the horizontal square of `radius` around
    /// `center`, in vanilla's chunk iteration order.
    ///
    /// Vanilla parity: `PoiManager.getInSquare`, whose `ChunkPos.rangeClosed`
    /// raster-scans X fastest inside each Z row. Y is deliberately unbounded --
    /// only X and Z are compared against `radius`.
    fn visit_in_square(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
        visit: &mut impl FnMut(&PointOfInterest),
    ) {
        let center_chunk = ChunkPos::from_block_pos(center);
        let chunk_radius = radius.div_euclid(16) + 1;

        for cz in center_chunk.0.y - chunk_radius..=center_chunk.0.y + chunk_radius {
            for cx in center_chunk.0.x - chunk_radius..=center_chunk.0.x + chunk_radius {
                self.visit_chunk(type_predicate, ChunkPos::new(cx, cz), status, &mut |poi| {
                    let dx = (poi.pos.0.x - center.0.x).abs();
                    let dz = (poi.pos.0.z - center.0.z).abs();
                    if dx <= radius && dz <= radius {
                        visit(poi);
                    }
                });
            }
        }
    }

    /// Returns all matching POIs within a vanilla horizontal square centered on `center`.
    ///
    /// Vanilla parity: `PoiManager.getInSquare`. X/Z are constrained by
    /// `radius`, Y is not.
    #[must_use]
    pub fn get_in_horizontal_square(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> Vec<(BlockPos, usize)> {
        let mut results = Vec::new();
        self.visit_in_square(type_predicate, center, radius, status, &mut |poi| {
            results.push((poi.pos, poi.poi_type_id));
        });
        results
    }

    /// Walks every matching POI within `radius` blocks of `center`.
    ///
    /// Vanilla parity: `PoiManager.getInRange` -- the square query narrowed to a
    /// sphere by squared distance.
    fn visit_in_range(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
        visit: &mut impl FnMut(&PointOfInterest),
    ) {
        let radius_sq = i64::from(radius) * i64::from(radius);
        self.visit_in_square(type_predicate, center, radius, status, &mut |poi| {
            if distance_sq(poi.pos, center) <= radius_sq {
                visit(poi);
            }
        });
    }

    /// Returns all matching POIs within `radius` blocks of `center`.
    ///
    /// Vanilla parity: `PoiManager.getInRange`.
    #[must_use]
    pub fn get_in_range(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> Vec<(BlockPos, usize)> {
        let mut results = Vec::new();
        self.visit_in_range(type_predicate, center, radius, status, &mut |poi| {
            results.push((poi.pos, poi.poi_type_id));
        });
        results
    }

    /// Counts matching POIs within `radius` blocks of `center`.
    ///
    /// Vanilla parity: `PoiManager.getCountInRange`.
    #[must_use]
    pub fn count(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> usize {
        let mut count = 0;
        self.visit_in_range(type_predicate, center, radius, status, &mut |_| count += 1);
        count
    }

    /// Returns every matching POI position that also passes `pos_filter`.
    ///
    /// Vanilla parity: `PoiManager.findAll`.
    #[must_use]
    pub fn find_all(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        pos_filter: &impl Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> Vec<BlockPos> {
        let mut results = Vec::new();
        self.visit_in_range(type_predicate, center, radius, status, &mut |poi| {
            if pos_filter(poi.pos) {
                results.push(poi.pos);
            }
        });
        results
    }

    /// Returns every matching POI with its type, closest to `center` first.
    ///
    /// Vanilla parity: `PoiManager.findAllClosestFirstWithType`; the untyped
    /// `findAllWithType` is this without the sort, and every caller of it in
    /// vanilla either sorts or does not care about order.
    #[must_use]
    pub fn find_all_closest_first_with_type(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        pos_filter: &impl Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> Vec<(BlockPos, usize)> {
        let mut results = Vec::new();
        self.visit_in_range(type_predicate, center, radius, status, &mut |poi| {
            if pos_filter(poi.pos) {
                results.push((poi.pos, poi.poi_type_id));
            }
        });
        results.sort_by_key(|&(pos, _)| distance_sq(pos, center));
        results
    }

    /// Returns the first matching POI position that passes `pos_filter`.
    ///
    /// Vanilla parity: `PoiManager.find`.
    #[must_use]
    pub fn find(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        pos_filter: &impl Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> Option<BlockPos> {
        let mut found = None;
        self.visit_in_range(type_predicate, center, radius, status, &mut |poi| {
            if found.is_none() && pos_filter(poi.pos) {
                found = Some(poi.pos);
            }
        });
        found
    }

    /// Returns the matching POI position closest to `center` that passes `pos_filter`.
    ///
    /// Vanilla parity: `PoiManager.findClosest`, both the two-argument form
    /// (pass a filter that accepts everything) and the filtered overload.
    #[must_use]
    pub fn find_closest(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        pos_filter: &impl Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> Option<BlockPos> {
        self.find_closest_with_type(type_predicate, pos_filter, center, radius, status)
            .map(|(pos, _)| pos)
    }

    /// Returns the matching POI closest to `center`, with its type.
    ///
    /// Vanilla parity: `PoiManager.findClosestWithType`.
    #[must_use]
    pub fn find_closest_with_type(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        pos_filter: &impl Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        status: OccupationStatus,
    ) -> Option<(BlockPos, usize)> {
        let mut best: Option<(BlockPos, usize)> = None;
        let mut best_distance = i64::MAX;
        self.visit_in_range(type_predicate, center, radius, status, &mut |poi| {
            if !pos_filter(poi.pos) {
                return;
            }
            let distance = distance_sq(poi.pos, center);
            // Strictly less keeps the first of equally close POIs, the way
            // vanilla's `Stream.min` does.
            if distance < best_distance {
                best_distance = distance;
                best = Some((poi.pos, poi.poi_type_id));
            }
        });
        best
    }

    /// Claims a ticket on the first free matching POI and returns its position.
    ///
    /// Vanilla parity: `PoiManager.take`.
    pub fn take(
        &mut self,
        type_predicate: &impl Fn(usize) -> bool,
        filter: &impl Fn(usize, BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
    ) -> Option<BlockPos> {
        let mut found = None;
        self.visit_in_range(
            type_predicate,
            center,
            radius,
            OccupationStatus::Free,
            &mut |poi| {
                if found.is_none() && filter(poi.poi_type_id, poi.pos) {
                    found = Some(poi.pos);
                }
            },
        );

        let pos = found?;
        // The POI was matched under `Free`, so the ticket is there to take.
        let _acquired = self.reserve_ticket(pos);
        Some(pos)
    }

    /// Returns a random matching POI position that passes `pos_filter`.
    ///
    /// Vanilla parity: `PoiManager.getRandom`, which shuffles the whole
    /// in-range list and then takes the first position the filter accepts --
    /// not the same distribution as filtering first, so the shuffle stays.
    #[must_use]
    pub fn get_random(
        &self,
        type_predicate: &impl Fn(usize) -> bool,
        pos_filter: &impl Fn(BlockPos) -> bool,
        status: OccupationStatus,
        center: BlockPos,
        radius: i32,
        rng: &mut impl rand::Rng,
    ) -> Option<BlockPos> {
        let mut candidates = self.get_in_range(type_predicate, center, radius, status);
        vanilla_shuffle(&mut candidates, rng);
        candidates
            .into_iter()
            .map(|(pos, _)| pos)
            .find(|&pos| pos_filter(pos))
    }

    /// Returns the section distance from `section` to the nearest village center,
    /// or [`NO_VILLAGE_DISTANCE`] when none is within [`MAX_VILLAGE_DISTANCE`].
    ///
    /// Vanilla parity: `PoiManager.sectionsToVillage`. A village center is a
    /// section holding an occupied POI tagged `#minecraft:village`
    /// (`PoiManager.isVillageCenter`), and vanilla's `DistanceTracker` is a
    /// `SectionTracker` whose neighbor step covers all 26 adjacent sections --
    /// so the level it settles on is exactly the Chebyshev section distance to
    /// the nearest center, capped at 7. Foton measures that distance directly
    /// instead of maintaining the incremental graph: the answer is the same, and
    /// the query walks the sparse loaded columns rather than every section
    /// coordinate.
    #[must_use]
    pub fn sections_to_village(&self, section: SectionPos) -> i32 {
        let mut best = NO_VILLAGE_DISTANCE;

        for cx in section.x() - MAX_VILLAGE_DISTANCE..=section.x() + MAX_VILLAGE_DISTANCE {
            for cz in section.z() - MAX_VILLAGE_DISTANCE..=section.z() + MAX_VILLAGE_DISTANCE {
                let horizontal = (cx - section.x()).abs().max((cz - section.z()).abs());
                if horizontal >= best {
                    continue;
                }
                let Some(column) = self.columns.get(&ChunkPos::new(cx, cz)) else {
                    continue;
                };

                for (&section_y, set) in column {
                    let distance = horizontal.max((section_y - section.y()).abs());
                    if distance >= best {
                        continue;
                    }
                    if Self::is_village_center(set) {
                        best = distance;
                    }
                }
            }
        }

        best
    }

    /// Returns `true` if this section holds a claimed POI tagged `#minecraft:village`.
    ///
    /// Vanilla parity: the private `PoiManager.isVillageCenter`. Occupancy is the
    /// point of it: an unclaimed bed or bell is not yet anybody's village.
    fn is_village_center(set: &PointOfInterestSet) -> bool {
        !set.get_matching(
            &is_village_type,
            OccupationStatus::Occupied,
            &max_tickets_for,
        )
        .is_empty()
    }

    /// Scans a chunk section for POI block states and populates the storage.
    ///
    /// # Panics
    /// Panics if the POI type registry contains an inconsistent state-to-type mapping.
    pub fn scan_and_populate(&mut self, section: &ChunkSection, section_pos: SectionPos) {
        let registry = &REGISTRY.poi_types;
        let chunk_pos = ChunkPos::new(section_pos.x(), section_pos.z());
        let set = self.get_or_create_set(chunk_pos, section_pos.y());

        for y in 0..16u8 {
            for z in 0..16u8 {
                for x in 0..16u8 {
                    let state_id = section.states.get(x as usize, y as usize, z as usize);

                    let Some(poi_type_id) = registry.type_id_for_state(state_id) else {
                        continue;
                    };
                    let poi_type = registry
                        .by_id(poi_type_id)
                        .expect("POI type ID from state lookup must be valid");
                    let block_pos = BlockPos::new(
                        (section_pos.x() << 4) + i32::from(x),
                        (section_pos.y() << 4) + i32::from(y),
                        (section_pos.z() << 4) + i32::from(z),
                    );
                    let packed = PackedSectionBlockPos::from_block_pos(block_pos);
                    set.add(
                        packed,
                        PointOfInterest::new(block_pos, poi_type_id, poi_type.ticket_count),
                    );
                }
            }
        }
    }

    /// Updates POI storage when a block state changes.
    ///
    /// # Panics
    /// Panics if the POI type registry contains an inconsistent state-to-type mapping.
    pub fn on_block_state_change(
        &mut self,
        pos: BlockPos,
        old_state: BlockStateId,
        new_state: BlockStateId,
    ) {
        let registry = &REGISTRY.poi_types;
        let old_poi = registry.type_id_for_state(old_state);
        let new_poi = registry.type_id_for_state(new_state);

        if old_poi == new_poi {
            return;
        }

        if old_poi.is_some() {
            self.remove(pos);
        }

        if let Some(type_id) = new_poi {
            let poi_type = registry
                .by_id(type_id)
                .expect("POI type ID from state lookup must be valid");
            self.add(pos, type_id, poi_type.ticket_count);
        }
    }

    /// Collects all POI data in a chunk column for persistence.
    ///
    /// Returns `(BlockPos, free_tickets)` for each POI.
    #[must_use]
    pub fn collect_for_chunk(&self, chunk_pos: ChunkPos) -> Vec<(BlockPos, u32)> {
        let Some(column) = self.columns.get(&chunk_pos) else {
            return Vec::new();
        };
        let mut results = Vec::new();
        for set in column.values() {
            for (_, poi) in set.iter() {
                results.push((poi.pos, poi.free_tickets));
            }
        }
        results
    }

    /// Restores ticket state for POIs after loading from disk.
    ///
    /// Called after `scan_and_populate` has created fresh POIs from block states.
    /// Applies saved `free_tickets` values to matching positions.
    pub fn restore_tickets(&mut self, chunk_pos: ChunkPos, tickets: &[(BlockPos, u32)]) {
        let Some(column) = self.columns.get_mut(&chunk_pos) else {
            return;
        };
        for &(pos, free_tickets) in tickets {
            let section_y = SectionPos::block_to_section_coord(pos.0.y);
            let packed = PackedSectionBlockPos::from_block_pos(pos);
            if let Some(set) = column.get_mut(&section_y)
                && let Some(poi) = set.get_mut(packed)
            {
                poi.free_tickets = free_tickets;
            }
        }
    }

    /// Removes all POI data for a chunk column. Called during chunk unload.
    pub fn remove_chunk(&mut self, chunk_pos: ChunkPos) {
        self.columns.remove(&chunk_pos);
    }
}

#[cfg(test)]
mod tests;
