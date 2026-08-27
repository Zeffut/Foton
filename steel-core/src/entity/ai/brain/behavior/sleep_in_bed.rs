//! Vanilla `SleepInBed` and `WakeUp`.

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, Trigger, utils};
use crate::behavior::BlockStateBehaviorExt as _;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::memory::memory_module_types;

/// Vanilla parity: `SleepInBed.COOLDOWN_AFTER_BEING_WOKEN`.
const COOLDOWN_AFTER_BEING_WOKEN: i64 = 100;
/// Vanilla parity: the `timestamp + 40L` of `SleepInBed.stop`.
const COOLDOWN_AFTER_GETTING_UP: i64 = 40;
/// How near the bed a villager has to stand before it climbs in.
///
/// Vanilla parity: the `closerToCenterThan(body.position(), 2.0)` of
/// `SleepInBed.checkExtraStartConditions`.
const CLOSE_ENOUGH_TO_CLIMB_IN: f64 = 2.0;
/// Vanilla parity: the `closerToCenterThan(body.position(), 1.14)` of
/// `SleepInBed.canStillUse`.
const CLOSE_ENOUGH_TO_STAY_IN: f64 = 1.14;
/// Vanilla parity: the `body.getY() > bedPos.getY() + 0.4` of `canStillUse`,
/// which is how a villager notices the bed under it has been mined.
const MIN_HEIGHT_ABOVE_BED: f64 = 0.4;

/// Puts the body in the bed it remembers as home.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SleepInBed`. It runs
/// in the REST package, so a villager only ever tries this once the
/// `villager_schedule` timeline has put it in [`Activity::Rest`], and
/// [`WakeUp`] in the core package gets it out again the moment the schedule
/// moves on.
///
/// MISSING FOUNDATION: vanilla's `start` also closes the doors it opened on the
/// way home, through `InteractWithDoor.closeDoorsThatIHaveOpenedOrPassedThrough`.
/// Getting into bed is also when a villager shuts the doors it left open on
/// the way home -- the same `DOORS_TO_CLOSE` pass [`InteractWithDoor`] runs as
/// it walks, which is why that one is public.
///
/// [`InteractWithDoor`]: super::InteractWithDoor
pub struct SleepInBed {
    next_ok_start_time: i64,
}

impl SleepInBed {
    /// Vanilla parity: `new SleepInBed()`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_ok_start_time: 0,
        }
    }
}

impl Default for SleepInBed {
    fn default() -> Self {
        Self::new()
    }
}

/// Vanilla parity: the `ImmutableMap` handed to `SleepInBed`'s `super(...)`.
const SLEEP_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (memory_module_types::HOME.id(), MemoryStatus::ValuePresent),
    (
        memory_module_types::LAST_WOKEN.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::LAST_SLEPT.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id(),
        MemoryStatus::Registered,
    ),
];

impl TimedBehavior for SleepInBed {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        SLEEP_ENTRY_CONDITION
    }

    /// Vanilla parity: `SleepInBed.timedOut`, which returns `false` -- a
    /// villager stays asleep until the schedule or the bed says otherwise.
    fn times_out(&self) -> bool {
        false
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let mob = ctx.mob();
        if mob.is_passenger() {
            return false;
        }
        let brain = ctx.brain();
        let Some(home) = brain.get_memory(memory_module_types::HOME) else {
            return false;
        };
        let world = ctx.world();
        if home.dimension != world.key {
            return false;
        }

        // Vanilla parity: the hundred-tick pause after being shaken awake, so a
        // woken villager does not climb straight back in.
        if let Some(last_woken) = brain.get_memory(memory_module_types::LAST_WOKEN) {
            let since = world.game_time() - last_woken;
            if since > 0 && since < COOLDOWN_AFTER_BEING_WOKEN {
                return false;
            }
        }

        if !utils::block_closer_to_center_than(home.pos, mob.position(), CLOSE_ENOUGH_TO_CLIMB_IN) {
            return false;
        }
        let state = world.get_block_state(home.pos);
        state.is_bed() && !state.get_value(&BlockStateProperties::OCCUPIED)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(home) = brain.get_memory(memory_module_types::HOME) else {
            return false;
        };
        let mob = ctx.mob();
        brain.is_active(Activity::Rest)
            && mob.position().y > f64::from(home.pos.y()) + MIN_HEIGHT_ABOVE_BED
            && utils::block_closer_to_center_than(home.pos, mob.position(), CLOSE_ENOUGH_TO_STAY_IN)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if ctx.game_time() <= self.next_ok_start_time {
            return;
        }
        let brain = ctx.brain();
        let Some(home) = brain.get_memory(memory_module_types::HOME) else {
            return;
        };
        // Vanilla parity: the `closeDoorsThatIHaveOpenedOrPassedThrough(level,
        // body, null, null, ..)` of `SleepInBed.start` -- with no path nodes to
        // excuse, so every door still tracked is a candidate.
        super::close_doors_behind(ctx.world(), ctx.mob(), brain, None, None);
        if let Err(error) = ctx.mob().start_sleeping(home.pos) {
            log::debug!(
                "villager {} could not lie down in its bed: {error}",
                ctx.mob().id()
            );
            return;
        }
        brain.set_memory(memory_module_types::LAST_SLEPT, ctx.game_time());
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if !ctx.mob().is_sleeping() {
            return;
        }
        ctx.mob().stop_sleeping();
        self.next_ok_start_time = ctx.game_time() + COOLDOWN_AFTER_GETTING_UP;
    }

    fn debug_name(&self) -> &'static str {
        "SleepInBed"
    }
}

/// Gets the body out of bed as soon as its schedule stops saying REST.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.WakeUp`. It sits in
/// the core package, so it runs whatever activity the villager is in, and it is
/// what writes `LAST_WOKEN` -- through `stopSleeping` on the villager side --
/// that keeps `SleepInBed` from putting it straight back.
pub struct WakeUp;

impl Trigger for WakeUp {
    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let mob = ctx.mob();
        if ctx.brain().is_active(Activity::Rest) || !mob.is_sleeping() {
            return false;
        }
        mob.stop_sleeping();
        true
    }

    fn debug_name(&self) -> &'static str {
        "WakeUp"
    }
}
