//! The two door goals and the Johnny goal a vindicator adds.
//!
//! All three are thin gates over goals that already exist: two of them refuse
//! to run outside a raid, and the third refuses to run unless the vindicator
//! is named Johnny. Vanilla declares the open-door one on `AbstractIllager`
//! rather than on `Vindicator`, but the vindicator is its only user.

use steel_utils::Downcast as _;
use steel_utils::types::Difficulty;

use super::VindicatorEntity;
use crate::entity::ai::goal::{
    BreakDoorGoal, Goal, GoalControls, NearestAttackableTargetGoal, OpenDoorGoal,
    reduced_tick_delay,
};
use crate::entity::{PathfinderMob, Raider};

/// Seconds vanilla builds the vindicator's door goal with.
///
/// Vanilla parity: the `6` of `new VindicatorBreakDoorGoal(this)`. It is below
/// `BreakDoorGoal`'s two-hundred-and-forty-tick floor, so it changes nothing;
/// it is written here because it is what vanilla passes.
const DOOR_BREAK_SECONDS: i32 = 6;

/// How often the vindicator reconsiders starting on a door.
///
/// Vanilla parity: the `nextInt(reducedTickDelay(10)) == 0` of `canUse`.
const DOOR_ATTEMPT_INTERVAL: i32 = 10;

/// Returns whether this difficulty lets a vindicator break doors.
///
/// Vanilla parity: `Vindicator.DOOR_BREAKING_PREDICATE`.
const fn breaks_doors_on(difficulty: Difficulty) -> bool {
    matches!(difficulty, Difficulty::Normal | Difficulty::Hard)
}

/// Beats a door down, but only during a raid.
///
/// Vanilla parity: `Vindicator.VindicatorBreakDoorGoal`. Steel has no raid, so
/// this goal never starts -- which is also true of vanilla outside a raid.
pub(super) struct VindicatorBreakDoorGoal {
    break_door: BreakDoorGoal,
}

impl VindicatorBreakDoorGoal {
    /// Creates the goal.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            break_door: BreakDoorGoal::new(DOOR_BREAK_SECONDS, breaks_doors_on),
        }
    }
}

impl Goal for VindicatorBreakDoorGoal {
    /// Vanilla parity: the `setFlags(EnumSet.of(Flag.MOVE))` the subclass adds
    /// on top of the plain `BreakDoorGoal`, which claims nothing.
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let in_raid = mob.as_raider().is_some_and(Raider::has_active_raid);
        in_raid
            && rand::random_range(0..reduced_tick_delay(DOOR_ATTEMPT_INTERVAL)) == 0
            && self.break_door.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.as_raider().is_some_and(Raider::has_active_raid)
            && self.break_door.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.break_door.start(mob);
        mob.set_no_action_time(0);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.break_door.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.break_door.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.break_door.tick(mob);
    }
}

/// Opens a door, but only during a raid.
///
/// Vanilla parity: `AbstractIllager.RaiderOpenDoorGoal`.
pub(super) struct RaiderOpenDoorGoal {
    open_door: OpenDoorGoal,
}

impl RaiderOpenDoorGoal {
    /// Creates the goal, which never closes the door behind it.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            open_door: OpenDoorGoal::new(false),
        }
    }
}

impl Goal for RaiderOpenDoorGoal {
    fn controls(&self) -> GoalControls {
        self.open_door.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.open_door.can_use(mob) && mob.as_raider().is_some_and(Raider::has_active_raid)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.open_door.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.open_door.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.open_door.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.open_door.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.open_door.tick(mob);
    }
}

/// Attacks anything at all, once the vindicator has been named Johnny.
///
/// Vanilla parity: `Vindicator.VindicatorJohnnyAttackGoal`, a
/// `NearestAttackableTargetGoal<LivingEntity>` with a zero random interval and
/// `attackable()` as its only filter.
pub(super) struct VindicatorJohnnyAttackGoal {
    nearest_attackable: NearestAttackableTargetGoal,
}

impl VindicatorJohnnyAttackGoal {
    /// Creates the goal.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            nearest_attackable: NearestAttackableTargetGoal::new_with_interval(
                0,
                true,
                true,
                |_, target, _| target.attackable(),
            ),
        }
    }

    /// Returns whether this mob answers to the name.
    fn is_johnny(mob: &dyn PathfinderMob) -> bool {
        mob.downcast_ref::<VindicatorEntity>()
            .is_some_and(VindicatorEntity::is_johnny)
    }
}

impl Goal for VindicatorJohnnyAttackGoal {
    fn controls(&self) -> GoalControls {
        self.nearest_attackable.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        Self::is_johnny(mob) && self.nearest_attackable.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.nearest_attackable.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.nearest_attackable.start(mob);
        mob.set_no_action_time(0);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.nearest_attackable.stop(mob);
    }
}
