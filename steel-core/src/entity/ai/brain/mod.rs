//! Vanilla's brain: memories, sensors, activities and a behavior schedule.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.Brain` and the
//! `ai/behavior`, `ai/sensing` and `ai/memory` packages around it. This is the
//! other half of vanilla's mob AI; the goal half lives in
//! [`crate::entity::ai::goal`].
//!
//! # Lock ordering
//!
//! A brain holds three locks, and they may only ever be taken in this order:
//!
//! 1. `runtime` -- the sensors and the behavior schedule. Taken by
//!    [`Brain::tick`] and [`Brain::stop_all`] and by nothing else. A sensor or
//!    behavior running underneath it must **never** call back into a method
//!    that takes it.
//! 2. `activities` -- which activities are active and what they require.
//! 3. `memories` -- the leaf lock, and the only one a behavior normally needs.
//!
//! Every public method here takes at most the locks its level allows, so a
//! behavior is free to read and write memories, and free to switch activity,
//! from inside its own tick -- which is what vanilla's `UpdateActivityFromSchedule`
//! does.
//!
//! The mob's own locks sit outside all three. In particular a behavior must
//! not call [`crate::entity::PathfinderMob::is_panicking`] (which locks the goal
//! selector) from anywhere the goal selector is already held; the brain is
//! ticked from `custom_server_ai_step`, after
//! `tick_pathfinder_goal_selectors` has released both selectors, so that is
//! safe by construction.

pub mod behavior;
pub mod context;
pub mod memory;
pub mod position_tracker;
pub mod sensor;

mod activity;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use simdnbt::borrow::NbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use steel_utils::locks::SyncMutex;

pub use activity::Activity;
pub use context::BrainContext;

use behavior::{BehaviorControl, BehaviorStatus};
use memory::{Memories, MemoryModuleId, MemoryModuleType, MemoryStatus, MemoryValueType};
use sensor::{Sensor, SensorType};

use crate::entity::PathfinderMob;
use crate::world::World;

/// The NBT key vanilla stores a packed brain under.
///
/// Vanilla parity: `LivingEntity.TAG_BRAIN`.
pub const BRAIN_NBT_KEY: &str = "Brain";

/// One activity's behaviors and the memories that gate it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.ActivityData`. 26.2 replaced
/// the older `Brain.addActivity` / `addActivityAndRemoveMemoryWhenStopped` /
/// `addActivityWithConditions` trio with this record plus its `create`
/// overloads, so those three method names no longer exist upstream; the
/// constructors below are the same four shapes.
pub struct ActivityData {
    activity: Activity,
    behaviors: Vec<(i32, Box<dyn BehaviorControl>)>,
    conditions: Vec<(MemoryModuleId, MemoryStatus)>,
    memories_to_erase_when_stopped: Vec<MemoryModuleId>,
}

impl ActivityData {
    /// Numbers `behaviors` upward from `priority_of_first_behavior`.
    ///
    /// Vanilla parity: `ActivityData.create(activity, priorityOfFirstBehavior, behaviorList)`.
    #[must_use]
    pub fn create(
        activity: Activity,
        priority_of_first_behavior: i32,
        behaviors: Vec<Box<dyn BehaviorControl>>,
    ) -> Self {
        Self::with_priorities(
            activity,
            behaviors
                .into_iter()
                .enumerate()
                .map(|(offset, behavior)| {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_possible_wrap,
                        reason = "an activity never holds more behaviors than an i32 can count"
                    )]
                    let priority = priority_of_first_behavior + offset as i32;
                    (priority, behavior)
                })
                .collect(),
        )
    }

    /// Takes an explicit priority per behavior.
    ///
    /// Vanilla parity: `ActivityData.create(activity, behaviorPriorityPairs)`.
    #[must_use]
    pub const fn with_priorities(
        activity: Activity,
        behaviors: Vec<(i32, Box<dyn BehaviorControl>)>,
    ) -> Self {
        Self {
            activity,
            behaviors,
            conditions: Vec::new(),
            memories_to_erase_when_stopped: Vec::new(),
        }
    }

    /// Only lets this activity become active while `conditions` hold.
    ///
    /// Vanilla parity: the `Set<Pair<MemoryModuleType<?>, MemoryStatus>> conditions`
    /// argument of `ActivityData.create`.
    #[must_use]
    pub fn with_conditions(mut self, conditions: Vec<(MemoryModuleId, MemoryStatus)>) -> Self {
        self.conditions = conditions;
        self
    }

    /// Erases `memories` when this activity stops.
    ///
    /// Vanilla parity: the `memoriesToEraseWhenStopped` argument.
    #[must_use]
    pub fn erasing_when_stopped(mut self, memories: Vec<MemoryModuleId>) -> Self {
        self.memories_to_erase_when_stopped = memories;
        self
    }

    /// Requires `memory` to hold a value, and erases it when the activity stops.
    ///
    /// Vanilla parity: `ActivityData.create(activity, priority, list, memoryThatMustHaveValueAndWillBeErasedAfter)`.
    #[must_use]
    pub fn gated_by(mut self, memory: MemoryModuleId) -> Self {
        self.conditions = vec![(memory, MemoryStatus::ValuePresent)];
        self.memories_to_erase_when_stopped = vec![memory];
        self
    }
}

/// Which activities are running and what they need.
#[derive(Debug)]
struct ActivityState {
    core: Vec<Activity>,
    active: FxHashSet<Activity>,
    default_activity: Activity,
    requirements: FxHashMap<Activity, Vec<(MemoryModuleId, MemoryStatus)>>,
    erase_when_stopped: FxHashMap<Activity, Vec<MemoryModuleId>>,
}

impl ActivityState {
    fn new() -> Self {
        Self {
            core: vec![Activity::Core],
            active: FxHashSet::default(),
            default_activity: Activity::Idle,
            requirements: FxHashMap::default(),
            erase_when_stopped: FxHashMap::default(),
        }
    }
}

/// One behavior in the schedule, with the activity and priority it runs at.
struct ScheduledBehavior {
    priority: i32,
    activity: Activity,
    behavior: Box<dyn BehaviorControl>,
}

/// One sensor and its rescan countdown.
///
/// Vanilla parity: the `scanRate`/`timeToTick` pair inside `Sensor`, plus
/// `randomlyDelayStart`, which staggers a herd's sensors so they do not all
/// rescan on the same tick.
struct ScheduledSensor {
    sensor: Box<dyn Sensor>,
    scan_rate: i32,
    time_to_tick: i32,
}

impl ScheduledSensor {
    fn new(sensor: Box<dyn Sensor>) -> Self {
        let scan_rate = sensor.scan_rate();
        Self {
            sensor,
            scan_rate,
            time_to_tick: rand::random_range(0..scan_rate.max(1)),
        }
    }

    /// Vanilla parity: the `final Sensor.tick`.
    fn tick(&mut self, ctx: &BrainContext<'_>) {
        self.time_to_tick -= 1;
        if self.time_to_tick > 0 {
            return;
        }
        self.time_to_tick = self.scan_rate;
        self.sensor.do_tick(ctx);
    }
}

/// The sensors and the behavior schedule.
///
/// Vanilla parity: the `sensors` and `availableBehaviorsByPriority` fields of
/// `Brain`. Vanilla keys the schedule on a `TreeMap<Integer, Map<Activity,
/// LinkedHashSet<BehaviorControl>>>`; a flat list kept sorted by priority walks
/// in exactly that order and is far easier to borrow from.
struct BrainRuntime {
    sensors: Vec<ScheduledSensor>,
    behaviors: Vec<ScheduledBehavior>,
}

/// A mob's brain.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.Brain<E>`. Vanilla is generic
/// over the body type so its behaviors can be typed; Steel takes
/// `&dyn PathfinderMob` for the same reason its goals do -- the mob owns the
/// brain, so the brain cannot name the mob's type -- and a behavior that needs
/// a concrete mob downcasts with `steel_utils::Downcast`.
pub struct Brain {
    /// Leaf lock. See the module's lock-ordering note.
    memories: SyncMutex<Memories>,
    /// Taken before `memories`, after `runtime`.
    activities: SyncMutex<ActivityState>,
    /// Outermost. Held only by [`Brain::tick`] and [`Brain::stop_all`].
    runtime: SyncMutex<BrainRuntime>,
}

impl Brain {
    /// Builds a brain with no sensors and no behaviors.
    ///
    /// Vanilla parity: the no-argument `Brain()` constructor that
    /// `LivingEntity.makeBrain` returns for a mob with no AI of its own.
    #[must_use]
    pub fn empty() -> Self {
        let brain = Self {
            memories: SyncMutex::new(Memories::default()),
            activities: SyncMutex::new(ActivityState::new()),
            runtime: SyncMutex::new(BrainRuntime {
                sensors: Vec::new(),
                behaviors: Vec::new(),
            }),
        };
        brain.use_default_activity();
        brain
    }

    /// Builds a brain from its sensors and activities.
    ///
    /// Vanilla parity: the `Brain(memoryTypes, sensorTypes, activities, memories, randomSource)`
    /// constructor.
    #[must_use]
    pub fn new(sensor_types: &[SensorType], activities: Vec<ActivityData>) -> Self {
        let brain = Self::empty();

        {
            let mut runtime = brain.runtime.lock();
            let mut memories = brain.memories.lock();
            for &sensor_type in sensor_types {
                let sensor = sensor_type.create();
                for required in sensor.required_memories() {
                    memories.register(required, memory::is_saved(required));
                }
                runtime.sensors.push(ScheduledSensor::new(sensor));
            }
        }

        for activity in activities {
            brain.add_activity(activity);
        }

        brain.use_default_activity();
        brain
    }

    /// Registers one activity's behaviors, conditions and cleanup.
    ///
    /// Vanilla parity: `Brain.addActivity`.
    fn add_activity(&self, data: ActivityData) {
        {
            let mut activities = self.activities.lock();
            activities
                .requirements
                .insert(data.activity, data.conditions);
            if !data.memories_to_erase_when_stopped.is_empty() {
                activities
                    .erase_when_stopped
                    .insert(data.activity, data.memories_to_erase_when_stopped);
            }
        }

        let mut runtime = self.runtime.lock();
        let mut memories = self.memories.lock();
        for (priority, behavior) in data.behaviors {
            for required in behavior.required_memories() {
                memories.register(required, memory::is_saved(required));
            }
            runtime.behaviors.push(ScheduledBehavior {
                priority,
                activity: data.activity,
                behavior,
            });
        }
        // A stable sort keeps insertion order inside one priority, which is
        // what vanilla's `LinkedHashSet` per activity gives.
        runtime
            .behaviors
            .sort_by_key(|scheduled| scheduled.priority);
    }

    /// Runs one brain tick.
    ///
    /// Vanilla parity: `Brain.tick`.
    pub fn tick(&self, world: &Arc<World>, mob: &dyn PathfinderMob) {
        self.memories.lock().tick();

        let ctx = BrainContext::new(world, mob, self, world.game_time());
        let mut runtime = self.runtime.lock();

        for sensor in &mut runtime.sensors {
            sensor.tick(&ctx);
        }

        // Vanilla parity: `startEachNonRunningBehavior`.
        for index in 0..runtime.behaviors.len() {
            let scheduled = &runtime.behaviors[index];
            if scheduled.behavior.status() != BehaviorStatus::Stopped
                || !self.is_active(scheduled.activity)
            {
                continue;
            }
            runtime.behaviors[index].behavior.try_start(&ctx);
        }

        // Vanilla parity: `tickEachRunningBehavior`.
        for scheduled in &mut runtime.behaviors {
            if scheduled.behavior.status() == BehaviorStatus::Running {
                scheduled.behavior.tick_or_stop(&ctx);
            }
        }
    }

    /// Stops every running behavior.
    ///
    /// Vanilla parity: `Brain.stopAll`.
    pub fn stop_all(&self, world: &Arc<World>, mob: &dyn PathfinderMob) {
        let ctx = BrainContext::new(world, mob, self, world.game_time());
        let mut runtime = self.runtime.lock();
        for scheduled in &mut runtime.behaviors {
            if scheduled.behavior.status() == BehaviorStatus::Running {
                scheduled.behavior.do_stop(&ctx);
            }
        }
    }

    /// Returns the names of the behaviors currently running, for debugging.
    ///
    /// Vanilla parity: `Brain.getRunningBehaviors`.
    #[must_use]
    pub fn running_behaviors(&self) -> Vec<&'static str> {
        self.runtime
            .lock()
            .behaviors
            .iter()
            .filter(|scheduled| scheduled.behavior.status() == BehaviorStatus::Running)
            .map(|scheduled| scheduled.behavior.debug_name())
            .collect()
    }

    /// Vanilla parity: `Brain.isBrainDead`.
    #[must_use]
    pub fn is_brain_dead(&self) -> bool {
        let runtime = self.runtime.lock();
        self.memories.lock().is_empty()
            && runtime.sensors.is_empty()
            && runtime.behaviors.is_empty()
    }

    // Activities.

    /// Vanilla parity: `Brain.setCoreActivities`.
    pub fn set_core_activities(&self, activities: Vec<Activity>) {
        self.activities.lock().core = activities;
    }

    /// Vanilla parity: `Brain.setDefaultActivity`.
    pub fn set_default_activity(&self, activity: Activity) {
        self.activities.lock().default_activity = activity;
    }

    /// Vanilla parity: `Brain.useDefaultActivity`.
    pub fn use_default_activity(&self) {
        let default_activity = self.activities.lock().default_activity;
        self.set_active_activity(default_activity);
    }

    /// Vanilla parity: `Brain.isActive`.
    #[must_use]
    pub fn is_active(&self, activity: Activity) -> bool {
        self.activities.lock().active.contains(&activity)
    }

    /// Vanilla parity: `Brain.getActiveNonCoreActivity`.
    #[must_use]
    pub fn active_non_core_activity(&self) -> Option<Activity> {
        let activities = self.activities.lock();
        activities
            .active
            .iter()
            .find(|activity| !activities.core.contains(activity))
            .copied()
    }

    /// Switches to `activity`, or falls back to the default when its memories
    /// are not in the required state.
    ///
    /// Vanilla parity: `Brain.setActiveActivityIfPossible`.
    pub fn set_active_activity_if_possible(&self, activity: Activity) {
        if self.activity_requirements_are_met(activity) {
            self.set_active_activity(activity);
        } else {
            self.use_default_activity();
        }
    }

    /// Switches to the first activity whose memories are in the required state.
    ///
    /// Vanilla parity: `Brain.setActiveActivityToFirstValid`.
    pub fn set_active_activity_to_first_valid(&self, activities: &[Activity]) {
        for &activity in activities {
            if self.activity_requirements_are_met(activity) {
                self.set_active_activity(activity);
                break;
            }
        }
    }

    /// Vanilla parity: the private `Brain.setActiveActivity`.
    fn set_active_activity(&self, activity: Activity) {
        let to_erase = {
            let mut activities = self.activities.lock();
            if activities.active.contains(&activity) {
                return;
            }

            // Vanilla parity: the memory cleanup `setActiveActivity` runs for every
            // activity it is leaving behind.
            let to_erase: Vec<MemoryModuleId> = activities
                .active
                .iter()
                .filter(|old| **old != activity)
                .filter_map(|old| activities.erase_when_stopped.get(old))
                .flatten()
                .copied()
                .collect();

            activities.active.clear();
            let core = activities.core.clone();
            activities.active.extend(core);
            activities.active.insert(activity);
            to_erase
        };

        let mut memories = self.memories.lock();
        for memory in to_erase {
            memories.erase(memory);
        }
    }

    /// Vanilla parity: the private `Brain.activityRequirementsAreMet`.
    fn activity_requirements_are_met(&self, activity: Activity) -> bool {
        let Some(requirements) = self.activities.lock().requirements.get(&activity).cloned() else {
            return false;
        };
        let memories = self.memories.lock();
        requirements
            .iter()
            .all(|&(memory, status)| memories.check(memory, status))
    }

    // Memories.

    /// Vanilla parity: `Brain.checkMemory`.
    #[must_use]
    pub fn check_memory(&self, memory: MemoryModuleId, status: MemoryStatus) -> bool {
        self.memories.lock().check(memory, status)
    }

    /// Vanilla parity: `Brain.hasMemoryValue`.
    #[must_use]
    pub fn has_memory_value(&self, memory: MemoryModuleId) -> bool {
        self.check_memory(memory, MemoryStatus::ValuePresent)
    }

    /// Vanilla parity: `Brain.getMemory`.
    #[must_use]
    pub fn get_memory<T: MemoryValueType>(&self, memory: MemoryModuleType<T>) -> Option<T> {
        T::from_memory_value(self.memories.lock().get(memory.id())?)
    }

    /// Vanilla parity: `Brain.isMemoryValue`.
    #[must_use]
    pub fn is_memory_value<T: MemoryValueType + PartialEq>(
        &self,
        memory: MemoryModuleType<T>,
        value: &T,
    ) -> bool {
        self.get_memory(memory)
            .is_some_and(|stored| stored == *value)
    }

    /// Vanilla parity: `Brain.getTimeUntilExpiry`.
    #[must_use]
    pub fn time_until_expiry<T: MemoryValueType>(&self, memory: MemoryModuleType<T>) -> i64 {
        self.memories.lock().time_to_live(memory.id())
    }

    /// Vanilla parity: `Brain.setMemory`.
    pub fn set_memory<T: MemoryValueType>(&self, memory: MemoryModuleType<T>, value: T) {
        self.memories
            .lock()
            .set(memory.id(), Some(value.into_memory_value()), i64::MAX);
    }

    /// Sets `memory` when `value` is present, erases it otherwise.
    ///
    /// Vanilla parity: `Brain.setMemory(MemoryModuleType<U>, Optional<? extends U>)`.
    pub fn set_memory_or_erase<T: MemoryValueType>(
        &self,
        memory: MemoryModuleType<T>,
        value: Option<T>,
    ) {
        self.memories.lock().set(
            memory.id(),
            value.map(MemoryValueType::into_memory_value),
            i64::MAX,
        );
    }

    /// Vanilla parity: `Brain.setMemoryWithExpiry`.
    pub fn set_memory_with_expiry<T: MemoryValueType>(
        &self,
        memory: MemoryModuleType<T>,
        value: T,
        time_to_live: i64,
    ) {
        self.memories
            .lock()
            .set(memory.id(), Some(value.into_memory_value()), time_to_live);
    }

    /// Vanilla parity: `Brain.eraseMemory`.
    pub fn erase_memory(&self, memory: MemoryModuleId) {
        self.memories.lock().erase(memory);
    }

    /// Vanilla parity: `Brain.clearMemories`.
    pub fn clear_memories(&self) {
        self.memories.lock().clear();
    }

    // Save and load.

    /// Writes the serializable memories under the `Brain` key.
    ///
    /// Vanilla parity: `output.store("Brain", Brain.Packed.CODEC, this.brain.pack())`
    /// in `LivingEntity.addAdditionalSaveData`.
    pub fn save(&self, nbt: &mut NbtCompound) {
        let mut packed = NbtCompound::new();
        packed.insert("memories", self.memories.lock().pack());
        nbt.insert(BRAIN_NBT_KEY, packed);
    }

    /// Reads back what [`Self::save`] wrote.
    ///
    /// Vanilla parity: `input.read("Brain", Brain.Packed.CODEC)` feeding
    /// `makeBrain`. Vanilla rebuilds the whole brain around the packed
    /// memories; Steel restores into the brain the mob already built, which
    /// keeps the same memories -- the constructor registers them either way --
    /// without making every mob able to replace its own brain field.
    pub fn load(&self, nbt: BorrowedNbtCompound<'_, '_>) {
        let Some(memories) = nbt
            .compound(BRAIN_NBT_KEY)
            .and_then(|brain| brain.compound("memories"))
        else {
            return;
        };
        self.memories.lock().restore(&memories);
    }
}

impl Default for Brain {
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for Brain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Both snapshots are taken before the builder runs, so this keeps the
        // module's runtime-then-activities lock order even while formatting.
        let running = self.running_behaviors();
        let active = self.activities.lock().active.clone();
        f.debug_struct("Brain")
            .field("active", &active)
            .field("running", &running)
            .finish_non_exhaustive()
    }
}
