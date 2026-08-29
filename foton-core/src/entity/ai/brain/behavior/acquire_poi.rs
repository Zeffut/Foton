//! Vanilla `AcquirePoi`.

use foton_registry::poi::PoiTypeRef;
use foton_registry::{REGISTRY, RegistryExt as _};
use foton_utils::entity_events::EntityStatus;
use foton_utils::{BlockPos, GlobalPos};
use rustc_hash::FxHashMap;

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryModuleType};
use crate::poi::poi_storage::OccupationStatus;
use crate::world::World;

/// Vanilla parity: `AcquirePoi.SCAN_RANGE`.
pub const SCAN_RANGE: i32 = 48;
/// Vanilla parity: the `int batchSize = 5` of `AcquirePoi.create`.
const BATCH_SIZE: usize = 5;
/// Vanilla parity: the `int rate = 20` of `AcquirePoi.create`.
const RATE: i64 = 20;
/// Vanilla parity: `JitteredLinearRetry.MIN_INTERVAL_INCREASE`.
const MIN_INTERVAL_INCREASE: i32 = 40;
/// Vanilla parity: `JitteredLinearRetry.MAX_INTERVAL_INCREASE`.
const MAX_INTERVAL_INCREASE: i32 = 80;
/// Vanilla parity: `JitteredLinearRetry.MAX_RETRY_PATHFINDING_INTERVAL`.
const MAX_RETRY_PATHFINDING_INTERVAL: i32 = 400;
/// The reach range used when nothing in the batch declares a larger one.
///
/// Vanilla parity: the `int maxRange = 1` seed of `AcquirePoi.findPathToPois`.
const MIN_REACH_RANGE: i32 = 1;

/// Which POI types this behavior will claim.
///
/// Vanilla's is a bare `Predicate<Holder<PoiType>>`, bound to the body's
/// profession when the brain is built and re-bound by `Villager.refreshBrain`
/// whenever that profession changes. Foton hands the tick's context to the
/// predicate instead, so a villager that takes up a trade starts looking for
/// the matching workstation on its next scan without its brain being replaced.
type PoiTypeFilter = Box<dyn Fn(&BrainContext<'_>, PoiTypeRef) -> bool + Send>;
/// An extra test on the block itself, beyond its POI type.
type PoiPositionFilter = Box<dyn Fn(&World, BlockPos) -> bool + Send>;

/// How long a position that could not be pathed to is left alone.
///
/// Vanilla parity: `AcquirePoi.JitteredLinearRetry`. Every failed attempt
/// pushes the next one further out, up to four hundred ticks, so a village
/// whose only free bed is walled off does not repath to it every second.
struct JitteredLinearRetry {
    previous_attempt_timestamp: i64,
    next_scheduled_attempt_timestamp: i64,
    current_delay: i32,
}

impl JitteredLinearRetry {
    fn new(first_attempt_timestamp: i64) -> Self {
        let mut retry = Self {
            previous_attempt_timestamp: 0,
            next_scheduled_attempt_timestamp: 0,
            current_delay: 0,
        };
        retry.mark_attempt(first_attempt_timestamp);
        retry
    }

    fn mark_attempt(&mut self, timestamp: i64) {
        self.previous_attempt_timestamp = timestamp;
        // Vanilla's `currentDelay + random.nextInt(40) + 40` is this half-open
        // range added to the running delay.
        let suggested_delay =
            self.current_delay + rand::random_range(MIN_INTERVAL_INCREASE..MAX_INTERVAL_INCREASE);
        self.current_delay = suggested_delay.min(MAX_RETRY_PATHFINDING_INTERVAL);
        self.next_scheduled_attempt_timestamp = timestamp + i64::from(self.current_delay);
    }

    fn is_still_valid(&self, timestamp: i64) -> bool {
        timestamp - self.previous_attempt_timestamp < i64::from(MAX_RETRY_PATHFINDING_INTERVAL)
    }

    const fn should_retry(&self, timestamp: i64) -> bool {
        timestamp >= self.next_scheduled_attempt_timestamp
    }
}

/// Finds the nearest free point of interest it can walk to and claims a ticket.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.AcquirePoi`. This is
/// how a villager gets a workstation, a bed and a meeting point, and how a
/// piglin brute or a bee-like mob would get a home.
///
/// Vanilla's two-memory form (`memoryToValidate` != `memoryToAcquire`) is what
/// stops an employed villager from hunting a second workstation: the villager
/// acquires into `POTENTIAL_JOB_SITE` but only while `JOB_SITE` is also empty.
pub struct AcquirePoi {
    poi_type: PoiTypeFilter,
    memory_to_validate: MemoryModuleType<GlobalPos>,
    memory_to_acquire: MemoryModuleType<GlobalPos>,
    only_if_adult: bool,
    on_poi_acquisition_event: Option<EntityStatus>,
    valid_poi: PoiPositionFilter,
    /// Vanilla's `MutableLong nextScheduledStart`, whose zero doubles as "not
    /// booked yet"; `None` says that without colliding with game time zero.
    next_scheduled_start: Option<i64>,
    batch_cache: FxHashMap<BlockPos, JitteredLinearRetry>,
}

impl AcquirePoi {
    /// Vanilla parity: the five-argument `AcquirePoi.create`, whose
    /// `memoryToValidate` is the memory it acquires into.
    #[must_use]
    pub fn new(
        poi_type: impl Fn(&BrainContext<'_>, PoiTypeRef) -> bool + Send + 'static,
        memory_to_acquire: MemoryModuleType<GlobalPos>,
        only_if_adult: bool,
        on_poi_acquisition_event: Option<EntityStatus>,
    ) -> Self {
        Self::with_validated_memory(
            poi_type,
            memory_to_acquire,
            memory_to_acquire,
            only_if_adult,
            on_poi_acquisition_event,
        )
    }

    /// Vanilla parity: the six-argument `AcquirePoi.create`.
    #[must_use]
    pub fn with_validated_memory(
        poi_type: impl Fn(&BrainContext<'_>, PoiTypeRef) -> bool + Send + 'static,
        memory_to_validate: MemoryModuleType<GlobalPos>,
        memory_to_acquire: MemoryModuleType<GlobalPos>,
        only_if_adult: bool,
        on_poi_acquisition_event: Option<EntityStatus>,
    ) -> Self {
        Self {
            poi_type: Box::new(poi_type),
            memory_to_validate,
            memory_to_acquire,
            only_if_adult,
            on_poi_acquisition_event,
            valid_poi: Box::new(|_, _| true),
            next_scheduled_start: None,
            batch_cache: FxHashMap::default(),
        }
    }

    /// Narrows the claim to positions the block itself accepts.
    ///
    /// Vanilla parity: the `BiPredicate<ServerLevel, BlockPos> validPoi`
    /// argument, which is how a villager refuses a bed somebody is already in.
    #[must_use]
    pub fn with_valid_poi(
        mut self,
        valid_poi: impl Fn(&World, BlockPos) -> bool + Send + 'static,
    ) -> Self {
        self.valid_poi = Box::new(valid_poi);
        self
    }

    /// Whether the POI type registered under this id is one this claims.
    fn accepts_type(&self, ctx: &BrainContext<'_>, poi_type_id: usize) -> bool {
        REGISTRY
            .poi_types
            .by_id(poi_type_id)
            .is_some_and(|poi_type| (self.poi_type)(ctx, poi_type))
    }

    /// Whether this position may be pathed to again yet, marking the attempt.
    ///
    /// Vanilla parity: the `cacheTest` predicate of `AcquirePoi.create`.
    fn cache_allows(&mut self, pos: BlockPos, timestamp: i64) -> bool {
        let Some(retry) = self.batch_cache.get_mut(&pos) else {
            return true;
        };
        if !retry.should_retry(timestamp) {
            return false;
        }
        retry.mark_attempt(timestamp);
        true
    }
}

impl Trigger for AcquirePoi {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![self.memory_to_validate.id(), self.memory_to_acquire.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        // Vanilla parity: the `i.absent(...)` groups, the outer one only
        // present when the two memories differ.
        if brain.has_memory_value(self.memory_to_acquire.id())
            || brain.has_memory_value(self.memory_to_validate.id())
        {
            return false;
        }

        let mob = ctx.mob();
        if self.only_if_adult && mob.is_baby() {
            return false;
        }

        let timestamp = ctx.game_time();
        let Some(next_start) = self.next_scheduled_start else {
            self.next_scheduled_start = Some(timestamp + rand::random_range(0..RATE));
            return false;
        };
        if timestamp < next_start {
            return false;
        }
        self.next_scheduled_start = Some(timestamp + RATE + rand::random_range(0..RATE));

        let world = ctx.world();
        self.batch_cache
            .retain(|_, retry| retry.is_still_valid(timestamp));

        let in_range = {
            let storage = world.poi_storage.lock();
            storage.find_all_closest_first_with_type(
                &|poi_type_id| self.accepts_type(ctx, poi_type_id),
                &|_| true,
                mob.block_position(),
                SCAN_RANGE,
                // Vanilla parity: `PoiManager.Occupancy.HAS_SPACE`.
                OccupationStatus::Free,
            )
        };

        // Vanilla limits to five *before* running `validPoi`, so a batch can
        // come out shorter than five, and only the survivors are ever cached.
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        for (pos, poi_type_id) in in_range {
            if batch.len() == BATCH_SIZE {
                break;
            }
            if self.cache_allows(pos, timestamp) {
                batch.push((pos, poi_type_id));
            }
        }
        let candidates: Vec<(BlockPos, usize)> = batch
            .into_iter()
            .filter(|&(pos, _)| (self.valid_poi)(world, pos))
            .collect();

        let Some(target_pos) = path_to_candidates(ctx, &candidates) else {
            // Vanilla parity: the `computeIfAbsent` that puts every unreachable
            // candidate on the retry ladder.
            for &(pos, _) in &candidates {
                self.batch_cache
                    .entry(pos)
                    .or_insert_with(|| JitteredLinearRetry::new(timestamp));
            }
            return true;
        };

        let taken = {
            let mut storage = world.poi_storage.lock();
            // Vanilla checks `getType(targetPos)` is present before taking, so a
            // POI that vanished between the scan and the path is not claimed.
            if storage.get_type(target_pos).is_none() {
                None
            } else {
                storage.take(
                    &|poi_type_id| self.accepts_type(ctx, poi_type_id),
                    &|_, candidate| candidate == target_pos,
                    target_pos,
                    1,
                )
            }
        };
        if taken != Some(target_pos) {
            return true;
        }

        brain.set_memory(
            self.memory_to_acquire,
            GlobalPos::new(world.key.clone(), target_pos),
        );
        if let Some(event) = self.on_poi_acquisition_event {
            mob.broadcast_entity_event(event);
        }
        self.batch_cache.clear();
        true
    }

    fn debug_name(&self) -> &'static str {
        "AcquirePoi"
    }
}

/// Vanilla parity: `AcquirePoi.findPathToPois`, returning the reachable target
/// rather than the path, which is all the caller uses.
fn path_to_candidates(
    ctx: &BrainContext<'_>,
    candidates: &[(BlockPos, usize)],
) -> Option<BlockPos> {
    if candidates.is_empty() {
        return None;
    }
    let mut max_range = MIN_REACH_RANGE;
    let mut targets = Vec::with_capacity(candidates.len());
    for &(pos, poi_type_id) in candidates {
        if let Some(poi_type) = REGISTRY.poi_types.by_id(poi_type_id) {
            max_range = max_range.max(i32::try_from(poi_type.search_distance).unwrap_or(i32::MAX));
        }
        targets.push(pos);
    }
    let path = ctx
        .mob()
        .create_path_to_targets(ctx.world(), &targets, max_range)?;
    path.can_reach().then(|| path.target())
}
