//! The job site and bed a villager holds a ticket on.
//!
//! Vanilla parity: the `JOB_SITE` and `HOME` entries of `Villager.POI_MEMORIES`,
//! acquired by `AcquirePoi`, promoted by `AssignProfessionFromJobSite`, and
//! released by `Villager.releasePoi`.
//!
//! Vanilla holds these as brain memories and acquires them from the villager's
//! WORK and REST activities. Steel's villager has no schedule yet (see the
//! module docs on [`super`]), so the acquisition runs directly from the mob's
//! server tick on the same twenty-tick-plus-jitter cadence `AcquirePoi` uses.
//! The observable behavior a player sees -- a villager walks to an unclaimed
//! workstation, takes its profession from it, and gives it up when it dies --
//! is the same; what is missing is that it does not wait until its working
//! hours to do it.

use rand::RngExt as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::poi::PoiTypeRef;
use steel_registry::vanilla_poi_type_tags::PoiTag;
use steel_registry::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _, vanilla_poi_types};
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;

use crate::poi::poi_storage::OccupationStatus;
use crate::world::World;

/// Vanilla parity: `AcquirePoi.SCAN_RANGE`.
const SCAN_RANGE: i32 = 48;
/// Vanilla parity: the `rate` of `AcquirePoi`, twenty ticks plus up to twenty
/// more of jitter, so a village's villagers do not all scan on one tick.
const SCAN_RATE_TICKS: i64 = 20;

/// One villager's claims on the world.
#[derive(Debug, Default)]
pub struct VillagerPoiLinks {
    job_site: SyncMutex<Option<BlockPos>>,
    home: SyncMutex<Option<BlockPos>>,
    next_job_scan: SyncMutex<Option<i64>>,
    next_home_scan: SyncMutex<Option<i64>>,
}

/// What a scan found, so the caller can react to a newly taken job site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoiAcquisition {
    /// Nothing was claimed this tick.
    None,
    /// A job site was claimed at this position, with this POI type.
    JobSite(BlockPos, PoiTypeRef),
    /// A bed was claimed at this position.
    Home(BlockPos),
}

impl VillagerPoiLinks {
    /// A villager that holds no claims yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The workstation this villager has claimed, if any.
    #[must_use]
    pub fn job_site(&self) -> Option<BlockPos> {
        *self.job_site.lock()
    }

    /// The bed this villager has claimed, if any.
    #[must_use]
    pub fn home(&self) -> Option<BlockPos> {
        *self.home.lock()
    }

    /// Records a job site without claiming a ticket, for a load or a conversion.
    pub fn set_job_site(&self, pos: Option<BlockPos>) {
        *self.job_site.lock() = pos;
    }

    /// Records a home without claiming a ticket, for a load or a conversion.
    pub fn set_home(&self, pos: Option<BlockPos>) {
        *self.home.lock() = pos;
    }

    /// Whether enough ticks have passed to scan again, and books the next scan.
    ///
    /// Vanilla parity: the `nextScheduledStart` bookkeeping at the top of
    /// `AcquirePoi`, including its first-call behavior of booking a jittered
    /// start and doing nothing that tick.
    fn due(slot: &SyncMutex<Option<i64>>, game_time: i64) -> bool {
        let mut next = slot.lock();
        // Vanilla uses a zero `MutableLong` as its "not booked yet" marker,
        // which cannot tell an unbooked scan from one booked for tick zero.
        // `None` says the same thing without the collision.
        let Some(booked) = *next else {
            *next = Some(game_time + rand::rng().random_range(0..SCAN_RATE_TICKS));
            return false;
        };
        if game_time < booked {
            return false;
        }
        *next = Some(game_time + SCAN_RATE_TICKS + rand::rng().random_range(0..SCAN_RATE_TICKS));
        true
    }

    /// Looks for an unclaimed workstation and takes a ticket on it.
    ///
    /// `held_profession` is the villager's current profession key, or `None`
    /// when it is unemployed -- an unemployed villager will take any acquirable
    /// job site, an employed one only its own kind, which is vanilla's
    /// `heldJobSite` / `ALL_ACQUIRABLE_JOBS` split.
    ///
    /// `reachable` is asked before the ticket is taken, mirroring the way
    /// `AcquirePoi` paths to its candidates first, so a villager does not claim
    /// a workstation through a wall.
    pub fn try_acquire_job_site(
        &self,
        world: &World,
        origin: BlockPos,
        game_time: i64,
        held_profession: Option<&steel_utils::Identifier>,
        reachable: impl Fn(BlockPos, u32) -> bool,
    ) -> PoiAcquisition {
        if self.job_site.lock().is_some() || !Self::due(&self.next_job_scan, game_time) {
            return PoiAcquisition::None;
        }

        let accepts = |poi_type_id: usize| {
            let Some(poi_type) = REGISTRY.poi_types.by_id(poi_type_id) else {
                return false;
            };
            match held_profession {
                // Vanilla parity: `VillagerProfession.ALL_ACQUIRABLE_JOBS`.
                None => REGISTRY
                    .poi_types
                    .is_in_tag(poi_type, &PoiTag::ACQUIRABLE_JOB_SITE),
                // Vanilla parity: `heldJobSite`, which each profession builds as
                // `poiType.is(PoiTypes.<PROFESSION>)` -- the POI and the
                // profession share a key.
                Some(profession) => poi_type.key == *profession,
            }
        };

        let found = {
            let storage = world.poi_storage.lock();
            storage.find_closest_with_type(
                &accepts,
                &|_| true,
                origin,
                SCAN_RANGE,
                OccupationStatus::Free,
            )
        };
        let Some((pos, poi_type_id)) = found else {
            return PoiAcquisition::None;
        };
        let Some(poi_type) = REGISTRY.poi_types.by_id(poi_type_id) else {
            return PoiAcquisition::None;
        };
        if !reachable(pos, poi_type.search_distance) {
            return PoiAcquisition::None;
        }

        let taken = world.poi_storage.lock().take(
            &accepts,
            &|_, candidate| candidate == pos,
            origin,
            SCAN_RANGE,
        );
        if taken != Some(pos) {
            return PoiAcquisition::None;
        }

        *self.job_site.lock() = Some(pos);
        PoiAcquisition::JobSite(pos, poi_type)
    }

    /// Looks for an unclaimed bed and takes a ticket on it.
    pub fn try_acquire_home(
        &self,
        world: &World,
        origin: BlockPos,
        game_time: i64,
        reachable: impl Fn(BlockPos, u32) -> bool,
    ) -> PoiAcquisition {
        if self.home.lock().is_some() || !Self::due(&self.next_home_scan, game_time) {
            return PoiAcquisition::None;
        }

        let accepts = |poi_type_id: usize| {
            REGISTRY
                .poi_types
                .by_id(poi_type_id)
                .is_some_and(|poi_type| poi_type.key == vanilla_poi_types::HOME.key)
        };

        let found = {
            let storage = world.poi_storage.lock();
            storage.find_closest_with_type(
                &accepts,
                &|_| true,
                origin,
                SCAN_RANGE,
                OccupationStatus::Free,
            )
        };
        let Some((pos, _)) = found else {
            return PoiAcquisition::None;
        };
        if !reachable(pos, vanilla_poi_types::HOME.search_distance) {
            return PoiAcquisition::None;
        }

        let taken = world.poi_storage.lock().take(
            &accepts,
            &|_, candidate| candidate == pos,
            origin,
            SCAN_RANGE,
        );
        if taken != Some(pos) {
            return PoiAcquisition::None;
        }

        *self.home.lock() = Some(pos);
        PoiAcquisition::Home(pos)
    }

    /// Gives up both tickets.
    ///
    /// Vanilla parity: `Villager.releaseAllPois`, called from `die` and from
    /// every conversion -- a villager killed at its workstation must not leave
    /// the workstation claimed forever.
    pub fn release_all(&self, world: &World) {
        let mut storage = world.poi_storage.lock();
        for slot in [&self.job_site, &self.home] {
            if let Some(pos) = slot.lock().take() {
                let _released = storage.release_ticket(pos);
            }
        }
    }

    /// Writes the two claims the way vanilla saves the matching brain memories.
    ///
    /// Vanilla parity: the `Brain.memories` compound of a saved villager, whose
    /// `minecraft:job_site` and `minecraft:home` entries each hold a `GlobalPos`.
    pub fn save(&self, nbt: &mut NbtCompound) {
        let mut memories = NbtCompound::new();
        for (key, pos) in [
            ("minecraft:job_site", self.job_site()),
            ("minecraft:home", self.home()),
        ] {
            let Some(pos) = pos else { continue };
            let mut global_pos = NbtCompound::new();
            // MISSING FOUNDATION: a villager cannot hold a claim in another
            // dimension, and Steel has no per-world dimension key reachable
            // from here, so the saved dimension is always the overworld. A
            // villager taken through a portal would have released its claims on
            // the way out, so nothing observable depends on it yet.
            global_pos.insert("dimension", "minecraft:overworld");
            global_pos.insert("pos", NbtTag::IntArray(vec![pos.x(), pos.y(), pos.z()]));
            let mut memory = NbtCompound::new();
            memory.insert("value", global_pos);
            memories.insert(key, memory);
        }
        if memories.is_empty() {
            return;
        }
        let mut brain = NbtCompound::new();
        brain.insert("memories", memories);
        nbt.insert("Brain", brain);
    }

    /// Reads the two claims back.
    ///
    /// The ticket itself is not re-taken here: the POI storage restores its own
    /// ticket counts from the region file, so taking one again would double-book
    /// the workstation.
    pub fn load(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let Some(memories) = nbt
            .compound("Brain")
            .and_then(|brain| brain.compound("memories"))
        else {
            return;
        };
        for (key, slot) in [
            ("minecraft:job_site", &self.job_site),
            ("minecraft:home", &self.home),
        ] {
            let pos = memories
                .compound(key)
                .and_then(|memory| memory.compound("value"))
                .and_then(|value| value.int_array("pos"))
                .and_then(|array| {
                    let [x, y, z] = array[..] else { return None };
                    Some(BlockPos::new(x, y, z))
                });
            *slot.lock() = pos;
        }
    }
}
