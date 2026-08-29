//! Break-door goal.
//!
//! Vanilla parity: `BreakDoorGoal`. A mob that cannot path through a wooden
//! door hits it instead, and after twelve seconds the door is gone. The
//! crack overlay, the knocking and the difficulty gate all come from here: on
//! easy, nothing breaks anything.

use foton_registry::level_events;
use foton_registry::vanilla_game_rules::MOB_GRIEFING;
use foton_utils::types::{Difficulty, InteractionHand};
use glam::DVec3;

use super::door_interact::DoorInteractGoal;
use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;

/// Shortest a door can ever take to break, in ticks.
///
/// Vanilla parity: `BreakDoorGoal.DEFAULT_DOOR_BREAK_TIME`, which
/// `getDoorBreakTime` takes the maximum of. The vindicator's six is below it,
/// so a vindicator breaks a door in the same two hundred and forty ticks as a
/// zombie -- the constructor argument is dead weight in vanilla and is kept
/// only so the value written here is the one vanilla passes.
const DEFAULT_DOOR_BREAK_TIME: i32 = 240;

/// How far the crack overlay is divided.
///
/// Vanilla parity: the `* 10.0F` of the progress calculation.
const BREAK_PROGRESS_STAGES: f32 = 10.0;

/// Chance each tick that the mob knocks and swings.
///
/// Vanilla parity: the `nextInt(20) == 0` of `tick`.
const KNOCK_CHANCE_DENOMINATOR: i32 = 20;

/// How close the mob has to stay to keep working on the door.
///
/// Vanilla parity: the `closerToCenterThan(position, 2.0)` of
/// `canContinueToUse`.
const DOOR_REACH: f64 = 2.0;

/// Which difficulty an override of the goal accepts.
///
/// Vanilla parity: the `Predicate<Difficulty>` every `BreakDoorGoal` takes,
/// which is `NORMAL or HARD` for the two mobs that use it.
pub(crate) type DifficultyPredicate = fn(Difficulty) -> bool;

/// Beats a wooden door down.
///
/// Vanilla parity: `BreakDoorGoal`.
pub(crate) struct BreakDoorGoal {
    /// The shared door-finding half of the goal.
    door_interact: DoorInteractGoal,
    /// Which difficulties allow breaking at all.
    valid_difficulties: DifficultyPredicate,
    /// Ticks spent on the current door.
    break_time: i32,
    /// The last crack stage sent, so the overlay is only resent when it moves.
    last_break_progress: i32,
    /// The break time this goal was built with, before vanilla's floor.
    door_break_time: i32,
}

impl BreakDoorGoal {
    /// Creates the goal with vanilla's `(mob, seconds, difficulties)` shape.
    #[must_use]
    pub(crate) const fn new(seconds: i32, valid_difficulties: DifficultyPredicate) -> Self {
        Self {
            door_interact: DoorInteractGoal::new(),
            valid_difficulties,
            break_time: 0,
            last_break_progress: -1,
            door_break_time: seconds,
        }
    }

    /// Returns how long this door takes, in ticks.
    ///
    /// Vanilla parity: `getDoorBreakTime`.
    const fn door_break_time(&self) -> i32 {
        if self.door_break_time > DEFAULT_DOOR_BREAK_TIME {
            self.door_break_time
        } else {
            DEFAULT_DOOR_BREAK_TIME
        }
    }

    /// Returns whether this difficulty allows door breaking.
    fn is_valid_difficulty(&self, mob: &dyn PathfinderMob) -> bool {
        mob.level()
            .is_some_and(|world| (self.valid_difficulties)(world.difficulty()))
    }
}

impl Goal for BreakDoorGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !self.door_interact.can_use(mob) {
            return false;
        }
        let griefing = mob
            .level()
            .is_some_and(|world| world.get_game_rule(&MOB_GRIEFING));
        griefing && self.is_valid_difficulty(mob) && !self.door_interact.is_open(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let door_pos = self.door_interact.door_pos();
        let (x, y, z) = door_pos.get_bottom_center();
        let center = DVec3::new(x, y + 0.5, z);
        self.break_time <= self.door_break_time()
            && !self.door_interact.is_open(mob)
            && center.distance_squared(mob.position()) < DOOR_REACH * DOOR_REACH
            && self.is_valid_difficulty(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.door_interact.start(mob);
        self.break_time = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(world) = mob.level() {
            world.broadcast_block_destruction(mob.id(), self.door_interact.door_pos(), -1);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.door_interact.tick(mob);
        let Some(world) = mob.level() else {
            return;
        };
        let door_pos = self.door_interact.door_pos();

        if rand::random_range(0..KNOCK_CHANCE_DENOMINATOR) == 0 {
            world.level_event(level_events::SOUND_ZOMBIE_WOODEN_DOOR, door_pos, 0, None);
            if !mob.living_base().swing_state().swinging() {
                mob.swing(InteractionHand::MainHand, false);
            }
        }

        self.break_time += 1;
        let break_time = self.door_break_time();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla truncates the ten-stage progress to an int"
        )]
        let progress = (self.break_time as f32 / break_time as f32 * BREAK_PROGRESS_STAGES) as i32;
        if progress != self.last_break_progress {
            world.broadcast_block_destruction(mob.id(), door_pos, progress);
            self.last_break_progress = progress;
        }

        if self.break_time != break_time || !self.is_valid_difficulty(mob) {
            return;
        }
        world.remove_block(door_pos, false);
        world.level_event(level_events::SOUND_ZOMBIE_DOOR_CRASH, door_pos, 0, None);
        // Vanilla reads the state back after removing the door, so the
        // particles are the air that replaced it. Kept, because the client
        // reads the id and a mismatch here is visible.
        let broken_state = world.get_block_state(door_pos);
        world.destroy_block_effect(door_pos, u32::from(broken_state.0), None);
    }
}
