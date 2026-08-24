//! Behaviors: the units a brain's activities are built from.
//!
//! # Why there is no `BehaviorBuilder`
//!
//! Vanilla 26.2 writes most of its behaviors against the applicative-functor
//! DSL in `ai/behavior/declarative/`. That DSL exists to work around a Java
//! limitation: `i.group(i.present(A), i.absent(B)).apply(i, (a, b) -> ...)` is
//! how you get "run this only when memory A is set and memory B is not, and
//! hand me typed accessors to both" out of a language with no variadic
//! generics, and it is built on `DataFixerUpper`'s `K1`/`App` emulation of
//! higher-kinded types.
//!
//! Rust expresses the same thing natively:
//!
//! ```ignore
//! let Some(a) = ctx.brain().get_memory(A) else { return false };
//! if ctx.brain().has_memory_value(B.id()) { return false }
//! ```
//!
//! Porting the DSL would mean re-emulating higher-kinded types with GATs to
//! produce something strictly less readable than the two lines above, so Steel
//! ports [`OneShot`] and [`Trigger`] -- which are plain, useful shapes -- and
//! skips the builder. The one thing the DSL gave for free was
//! `getRequiredMemories`, derived from the accessors a behavior asked for; here
//! each behavior states that list itself, in [`Trigger::required_memories`] or
//! [`TimedBehavior::entry_condition`].

mod animal_panic;
mod back_up_if_too_close;
mod count_down_cooldown_ticks;
mod do_nothing;
mod gate_behavior;
mod look_at_target_sink;
mod melee_attack;
mod move_to_target_sink;
mod random_stroll;
mod set_entity_look_target;
mod set_walk_target_from_attack_target;
mod set_walk_target_from_look_target;
mod start_attacking;
mod stop_attacking_if_target_invalid;
mod swim;
pub mod transport_items_between_containers;
mod trigger_gate;

pub(crate) mod utils;

pub use animal_panic::AnimalPanic;
pub use count_down_cooldown_ticks::CountDownCooldownTicks;
pub use do_nothing::DoNothing;
pub use gate_behavior::RunOne;
pub use look_at_target_sink::LookAtTargetSink;
pub use move_to_target_sink::MoveToTargetSink;
pub use random_stroll::RandomStroll;
pub use set_entity_look_target::SetEntityLookTargetSometimes;
pub use transport_items_between_containers::TransportItemsBetweenContainers;

/// The general-purpose behaviors no Steel mob drives yet.
///
/// Vanilla builds every brain mob out of this set, so they are ported with the
/// framework rather than one at a time: a piglin, a warden or a hoglin needs
/// the attack four, and a villager needs the walk and gaze pair. The
/// expectation below goes away line by line as mobs pick them up -- an
/// unfulfilled expectation is the compiler saying one is now in use.
#[expect(
    unused_imports,
    reason = "framework re-exports waiting for the brain mobs they were ported for"
)]
pub use {
    back_up_if_too_close::BackUpIfTooClose,
    gate_behavior::{GateBehavior, OrderPolicy, RunningPolicy, ShufflingList},
    melee_attack::MeleeAttack,
    set_entity_look_target::SetEntityLookTarget,
    set_walk_target_from_attack_target::SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    set_walk_target_from_look_target::SetWalkTargetFromLookTarget,
    start_attacking::StartAttacking,
    stop_attacking_if_target_invalid::StopAttackingIfTargetInvalid,
    swim::Swim,
    trigger_gate::TriggerGate,
};

use super::context::BrainContext;
use super::memory::{MemoryModuleId, MemoryStatus};

/// How long a [`TimedBehavior`] runs when it does not say otherwise.
///
/// Vanilla parity: `Behavior.DEFAULT_DURATION`.
pub const DEFAULT_DURATION: i32 = 60;

/// Whether a behavior is currently running.
///
/// Vanilla parity: `Behavior.Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorStatus {
    Stopped,
    Running,
}

/// What a brain can schedule.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.BehaviorControl`.
pub trait BehaviorControl: Send {
    /// Vanilla parity: `BehaviorControl.getStatus`.
    fn status(&self) -> BehaviorStatus;

    /// The memories this behavior reads, so the brain can register them.
    ///
    /// Vanilla parity: `BehaviorControl.getRequiredMemories`.
    fn required_memories(&self) -> Vec<MemoryModuleId>;

    /// Vanilla parity: `BehaviorControl.tryStart`.
    fn try_start(&mut self, ctx: &BrainContext<'_>) -> bool;

    /// Vanilla parity: `BehaviorControl.tickOrStop`.
    fn tick_or_stop(&mut self, ctx: &BrainContext<'_>);

    /// Vanilla parity: `BehaviorControl.doStop`.
    fn do_stop(&mut self, ctx: &BrainContext<'_>);

    /// Vanilla parity: `BehaviorControl.debugString`.
    fn debug_name(&self) -> &'static str;
}

/// A behavior that runs for a while once started.
///
/// Vanilla parity: the abstract `net.minecraft.world.entity.ai.behavior.Behavior`.
/// Rust has no abstract base class, so the `final` half of vanilla's class --
/// the status field, the randomized duration and the start/tick/stop
/// sequencing -- lives in [`Behavior`], and the overridable half is this trait.
pub trait TimedBehavior: Send {
    /// The memories that must be in the given state before this may start.
    ///
    /// Vanilla parity: the `entryCondition` map passed to `super(...)`.
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)];

    /// The inclusive tick range this behavior runs for.
    ///
    /// Vanilla parity: the `minDuration`/`maxDuration` constructor arguments.
    fn duration(&self) -> (i32, i32) {
        (DEFAULT_DURATION, DEFAULT_DURATION)
    }

    /// Whether running out of duration stops this behavior.
    ///
    /// Vanilla parity: overriding `Behavior.timedOut` to return `false`.
    fn times_out(&self) -> bool {
        true
    }

    /// Vanilla parity: `Behavior.checkExtraStartConditions`.
    fn check_extra_start_conditions(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    /// Vanilla parity: `Behavior.start`.
    fn start(&mut self, _ctx: &BrainContext<'_>) {}

    /// Vanilla parity: `Behavior.tick`.
    fn tick(&mut self, _ctx: &BrainContext<'_>) {}

    /// Vanilla parity: `Behavior.stop`.
    fn stop(&mut self, _ctx: &BrainContext<'_>) {}

    /// Vanilla parity: `Behavior.canStillUse`, which defaults to `false` so a
    /// behavior that does not override it runs for exactly its duration.
    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        false
    }

    /// Vanilla parity: `Behavior.debugString`.
    fn debug_name(&self) -> &'static str;
}

/// Drives a [`TimedBehavior`] the way vanilla's `Behavior` drives its subclass.
pub struct Behavior<B: TimedBehavior> {
    inner: B,
    status: BehaviorStatus,
    end_timestamp: i64,
}

impl<B: TimedBehavior> Behavior<B> {
    /// Wraps `inner` so a brain can schedule it.
    #[must_use]
    pub const fn new(inner: B) -> Self {
        Self {
            inner,
            status: BehaviorStatus::Stopped,
            end_timestamp: 0,
        }
    }

    /// Boxes `inner` ready for an activity list.
    #[must_use]
    pub fn boxed(inner: B) -> Box<dyn BehaviorControl>
    where
        B: 'static,
    {
        Box::new(Self::new(inner))
    }

    /// Vanilla parity: `Behavior.hasRequiredMemories`.
    fn has_required_memories(&self, ctx: &BrainContext<'_>) -> bool {
        self.inner
            .entry_condition()
            .iter()
            .all(|&(memory, status)| ctx.brain().check_memory(memory, status))
    }

    /// Vanilla parity: `Behavior.timedOut`.
    fn timed_out(&self, timestamp: i64) -> bool {
        self.inner.times_out() && timestamp > self.end_timestamp
    }
}

impl<B: TimedBehavior> BehaviorControl for Behavior<B> {
    fn status(&self) -> BehaviorStatus {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        self.inner
            .entry_condition()
            .iter()
            .map(|&(memory, _)| memory)
            .collect()
    }

    fn try_start(&mut self, ctx: &BrainContext<'_>) -> bool {
        if !self.has_required_memories(ctx) || !self.inner.check_extra_start_conditions(ctx) {
            return false;
        }

        self.status = BehaviorStatus::Running;
        let (min_duration, max_duration) = self.inner.duration();
        let duration = min_duration + rand::random_range(0..=(max_duration - min_duration));
        self.end_timestamp = ctx.game_time() + i64::from(duration);
        self.inner.start(ctx);
        true
    }

    fn tick_or_stop(&mut self, ctx: &BrainContext<'_>) {
        if !self.timed_out(ctx.game_time()) && self.inner.can_still_use(ctx) {
            self.inner.tick(ctx);
        } else {
            self.do_stop(ctx);
        }
    }

    fn do_stop(&mut self, ctx: &BrainContext<'_>) {
        self.status = BehaviorStatus::Stopped;
        self.inner.stop(ctx);
    }

    fn debug_name(&self) -> &'static str {
        self.inner.debug_name()
    }
}

/// A behavior that does its whole job in the tick it starts.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.declarative.Trigger`.
pub trait Trigger: Send {
    /// The memories this trigger reads.
    ///
    /// Vanilla derives this from the accessors declared in the
    /// `BehaviorBuilder` group; without the builder each trigger lists them.
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        Vec::new()
    }

    /// Runs once. Returning `false` means it did nothing.
    ///
    /// Vanilla parity: `Trigger.trigger`.
    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool;

    /// Vanilla parity: `BehaviorControl.debugString`.
    fn debug_name(&self) -> &'static str;
}

/// Schedules a [`Trigger`] as a behavior that stops the tick after it starts.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.OneShot`.
pub struct OneShot<T: Trigger> {
    inner: T,
    status: BehaviorStatus,
}

impl<T: Trigger> OneShot<T> {
    /// Wraps `inner` so a brain can schedule it.
    #[must_use]
    pub const fn new(inner: T) -> Self {
        Self {
            inner,
            status: BehaviorStatus::Stopped,
        }
    }

    /// Boxes `inner` ready for an activity list.
    #[must_use]
    pub fn boxed(inner: T) -> Box<dyn BehaviorControl>
    where
        T: 'static,
    {
        Box::new(Self::new(inner))
    }
}

impl<T: Trigger> BehaviorControl for OneShot<T> {
    fn status(&self) -> BehaviorStatus {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        self.inner.required_memories()
    }

    fn try_start(&mut self, ctx: &BrainContext<'_>) -> bool {
        if !self.inner.trigger(ctx) {
            return false;
        }
        self.status = BehaviorStatus::Running;
        true
    }

    fn tick_or_stop(&mut self, ctx: &BrainContext<'_>) {
        self.do_stop(ctx);
    }

    fn do_stop(&mut self, _ctx: &BrainContext<'_>) {
        self.status = BehaviorStatus::Stopped;
    }

    fn debug_name(&self) -> &'static str {
        self.inner.debug_name()
    }
}
