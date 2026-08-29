//! Walk to the raid, sweeping up any raider passed on the way.
//!
//! Vanilla parity: `PathfindToRaidGoal`. This is what stops a wave that spawned
//! ninety blocks out from standing in a field: it aims the mob at the village
//! center in short hops, re-aiming each time the path runs out. The second half
//! is the recruitment sweep -- once a second, every unattached raider within
//! sixteen blocks is pulled into the raid, which is how a passing patrol ends
//! up fighting somebody else's fight.

use core::f64::consts::FRAC_PI_2;

use glam::DVec3;

use super::default_random_pos_towards;
use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;

/// Ticks between two recruitment sweeps.
///
/// Vanilla parity: `PathfindToRaidGoal.RECRUITMENT_SEARCH_TICK_DELAY`.
const RECRUITMENT_SEARCH_TICK_DELAY: i32 = 20;

/// Speed a raider walks towards its raid at.
///
/// Vanilla parity: `PathfindToRaidGoal.SPEED_MODIFIER`.
const SPEED_MODIFIER: f64 = 1.0;

/// How far each hop towards the village center reaches.
///
/// Vanilla parity: the `getPosTowards(mob, 15, 4, ..)` of `tick`.
const HOP_HORIZONTAL_DISTANCE: i32 = 15;
const HOP_VERTICAL_DISTANCE: i32 = 4;

/// How far a raider will look for others to recruit.
///
/// Vanilla parity: the `inflate(16.0)` of `recruitNearby`.
const RECRUITMENT_RANGE: f64 = 16.0;

/// Walks a raider to its raid.
///
/// Vanilla parity: `PathfindToRaidGoal`.
pub(crate) struct PathfindToRaidGoal {
    /// Tick count past which the next recruitment sweep runs.
    recruitment_tick: i32,
}

impl PathfindToRaidGoal {
    /// Creates the goal.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            recruitment_tick: 0,
        }
    }

    /// Returns whether the mob is in a raid it still has to reach.
    ///
    /// Vanilla parity: the shared body of `canUse` and `canContinueToUse`.
    fn is_heading_to_raid(mob: &dyn PathfinderMob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        let Some(status) = raider.current_raid_status() else {
            return false;
        };
        if !status.active || status.over {
            return false;
        }
        mob.level()
            .is_some_and(|world| !world.is_village(mob.block_position()))
    }
}

impl Default for PathfindToRaidGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for PathfindToRaidGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_none()
            && mob.controlling_passenger_mob().is_none()
            && Self::is_heading_to_raid(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        Self::is_heading_to_raid(mob)
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(raider) = mob.as_raider() else {
            return;
        };
        let Some(raid) = raider.current_raid() else {
            return;
        };
        if !raid.is_active() {
            return;
        }
        let Some(world) = mob.level() else {
            return;
        };

        if mob.tick_count() > self.recruitment_tick {
            self.recruitment_tick = mob.tick_count() + RECRUITMENT_SEARCH_TICK_DELAY;
            let search = mob.bounding_box().inflate(RECRUITMENT_RANGE);
            let recruits = world.get_entities_in_aabb_matching(&search, |entity| {
                entity
                    .as_raider()
                    .is_some_and(|other| !other.has_active_raid() && other.is_recruitable())
            });
            for recruit in recruits {
                let Some(recruit_raider) = recruit.as_raider() else {
                    continue;
                };
                raid.join_raid(&world, raid.groups_spawned(), recruit_raider, None, true);
            }
        }

        if mob.is_path_finding() {
            return;
        }
        let (center_x, center_y, center_z) = raid.center().get_bottom_center();
        let Some(next) = default_random_pos_towards(
            mob,
            HOP_HORIZONTAL_DISTANCE,
            HOP_VERTICAL_DISTANCE,
            DVec3::new(center_x, center_y, center_z),
            FRAC_PI_2,
        ) else {
            return;
        };
        mob.move_to_pos(next, SPEED_MODIFIER);
    }
}
