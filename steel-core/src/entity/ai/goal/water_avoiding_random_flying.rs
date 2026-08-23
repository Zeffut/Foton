use std::f64::consts::FRAC_PI_2;

use glam::DVec3;

use super::random_pos::{air_and_water_random_pos, hover_random_pos};
use super::random_stroll::RandomStrollGoal;
use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;

/// How far the goal looks horizontally.
///
/// Vanilla parity: the `8` of `WaterAvoidingRandomFlyingGoal.getPosition`.
const HORIZONTAL_DIST: i32 = 8;

/// How far up a hover destination may be.
///
/// Vanilla parity: the `7` of the `HoverRandomPos.getPos` call.
const HOVER_VERTICAL_DIST: i32 = 7;

/// Highest and lowest a hover destination floats above what it found.
///
/// Vanilla parity: the `3` and `1` of the same call.
const HOVER_MAX_HEIGHT: i32 = 3;
const HOVER_MIN_HEIGHT: i32 = 1;

/// How far up or down the fallback air destination may be.
///
/// Vanilla parity: the `4` and `-2` of the `AirAndWaterRandomPos.getPos` call.
const AIR_VERTICAL_DIST: i32 = 4;
const AIR_FLYING_HEIGHT: i32 = -2;

/// A subclass override of where the flier wanders to.
///
/// Vanilla parity: the `getPosition` override of `Parrot.ParrotWanderGoal`.
type FlyingStrollPosition = Box<dyn Fn(&dyn PathfinderMob) -> Option<DVec3> + Send>;

/// The idle wander of a flying mob.
///
/// Vanilla parity: `WaterAvoidingRandomFlyingGoal`, which is the water-avoiding
/// stroll with a destination picked in three dimensions rather than on the
/// ground.
pub struct WaterAvoidingRandomFlyingGoal {
    stroll: RandomStrollGoal,
    position: Option<FlyingStrollPosition>,
}

impl WaterAvoidingRandomFlyingGoal {
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64) -> Self {
        Self {
            stroll: RandomStrollGoal::new(speed_modifier),
            position: None,
        }
    }

    /// Overrides where the flier wanders to.
    ///
    /// Vanilla parity: a `getPosition` override, as `Parrot.ParrotWanderGoal`
    /// uses to prefer a branch to sit on.
    #[must_use]
    pub(crate) fn with_position(
        mut self,
        position: impl Fn(&dyn PathfinderMob) -> Option<DVec3> + Send + 'static,
    ) -> Self {
        self.position = Some(Box::new(position));
        self
    }
}

impl Goal for WaterAvoidingRandomFlyingGoal {
    fn controls(&self) -> GoalControls {
        self.stroll.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        match &self.position {
            Some(position) => self.stroll.can_use_with_position(mob, position),
            None => self
                .stroll
                .can_use_with_position(mob, flying_stroll_position),
        }
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.stroll.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.stroll.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.stroll.stop(mob);
    }
}

/// Vanilla parity: `WaterAvoidingRandomFlyingGoal.getPosition`.
pub(crate) fn flying_stroll_position(mob: &dyn PathfinderMob) -> Option<DVec3> {
    let wander_direction = mob.calculate_view_vector(0.0, mob.rotation().0);
    hover_random_pos(
        mob,
        HORIZONTAL_DIST,
        HOVER_VERTICAL_DIST,
        wander_direction.x,
        wander_direction.z,
        FRAC_PI_2,
        HOVER_MAX_HEIGHT,
        HOVER_MIN_HEIGHT,
    )
    .or_else(|| {
        air_and_water_random_pos(
            mob,
            HORIZONTAL_DIST,
            AIR_VERTICAL_DIST,
            AIR_FLYING_HEIGHT,
            wander_direction.x,
            wander_direction.z,
            FRAC_PI_2,
        )
    })
}
