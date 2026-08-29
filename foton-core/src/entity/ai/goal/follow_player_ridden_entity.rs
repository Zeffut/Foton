use std::sync::Arc;

use foton_utils::Downcast as _;
use glam::DVec3;

use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::{Entity, PathfinderMob};
use crate::physics::MoverType;
use crate::player::Player;

/// How far around itself the mob looks for a vehicle with a driver.
///
/// Vanilla parity: the `inflate(5.0)` of `FollowPlayerRiddenEntityGoal`.
const SEARCH_RANGE: f64 = 5.0;
/// Ticks between two repaths.
///
/// Vanilla parity: `adjustedTickDelay(10)`. This goal does not require an
/// update every tick, so the adjustment halves it.
const RECALC_INTERVAL_TICKS: i32 = 10;
/// How close the mob has to get before it starts running ahead.
const SWITCH_TO_LEADING_DISTANCE: f64 = 4.0;
/// How far it may fall behind before it goes back to chasing.
const SWITCH_TO_CHASING_DISTANCE: f64 = 12.0;
/// How far ahead of the vehicle the mob aims while leading.
const LEAD_DISTANCE: i32 = 10;
/// Self-propulsion while chasing the vehicle.
const CHASE_SPEED: f32 = 0.015;
/// Self-propulsion while leading it.
const LEAD_SPEED: f32 = 0.01;

/// Picks out the vehicle kind this goal escorts.
///
/// Vanilla parity: the `Class<? extends Entity> entityTypeToFollow` constructor
/// argument, which Foton takes as a predicate because it has no class objects.
type VehicleFilter = Box<dyn Fn(&dyn Entity) -> bool + Send + Sync>;

/// Which half of the escort the mob is doing.
///
/// Vanilla parity: `FollowPlayerRiddenEntityGoal.FollowEntityGoal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    GoToEntity,
    GoInEntityDirection,
}

/// Escorts a vehicle a player is driving.
///
/// Vanilla parity: `FollowPlayerRiddenEntityGoal`, the goal that makes dolphins
/// race a rowing boat. It chases until it is alongside, then runs ahead until
/// it has fallen behind again.
pub(crate) struct FollowPlayerRiddenEntityGoal {
    is_vehicle_to_follow: VehicleFilter,
    following: Option<Arc<Player>>,
    time_to_recalc_path: i32,
    stage: Stage,
}

impl FollowPlayerRiddenEntityGoal {
    #[must_use]
    pub(crate) fn new(
        is_vehicle_to_follow: impl Fn(&dyn Entity) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            is_vehicle_to_follow: Box::new(is_vehicle_to_follow),
            following: None,
            time_to_recalc_path: 0,
            stage: Stage::GoToEntity,
        }
    }

    /// Vanilla parity: the `getEntitiesOfClass(entityTypeToFollow, ...)` search
    /// both `canUse` and `start` run.
    fn find_driver(
        &self,
        mob: &dyn PathfinderMob,
        mut accept: impl FnMut(&Player) -> bool,
    ) -> Option<Arc<Player>> {
        let world = mob.level()?;
        let search_box = mob.bounding_box().inflate(SEARCH_RANGE);
        for entity in world.get_entities_in_aabb_matching(&search_box, |entity| {
            (self.is_vehicle_to_follow)(entity)
        }) {
            let Some(passenger) = entity.controlling_passenger() else {
                continue;
            };
            if passenger.downcast_ref::<Player>().is_none() {
                continue;
            }

            let driver = world.nearest_player(passenger.position(), 1.0, |candidate| {
                candidate.uuid() == passenger.uuid()
            });
            // Vanilla walks every vehicle in the box rather than stopping at the
            // first, so a moored boat beside a rowed one does not hide it.
            if let Some(driver) = driver
                && accept(&driver)
            {
                return Some(driver);
            }
        }

        None
    }
}

impl Goal for FollowPlayerRiddenEntityGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn is_interruptable(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if self
            .following
            .as_ref()
            .is_some_and(|player| player.has_moved_horizontally_recently())
        {
            return true;
        }

        self.find_driver(mob, Player::has_moved_horizontally_recently)
            .is_some()
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        self.following
            .as_ref()
            .is_some_and(|player| player.is_passenger() && player.has_moved_horizontally_recently())
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.following = self.find_driver(mob, |_| true);
        self.time_to_recalc_path = 0;
        self.stage = Stage::GoToEntity;
    }

    fn stop(&mut self, _mob: &dyn PathfinderMob) {
        self.following = None;
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(following) = self.following.clone() else {
            return;
        };

        let speed = if self.stage == Stage::GoInEntityDirection {
            LEAD_SPEED
        } else {
            CHASE_SPEED
        };
        let input = mob.travel_input();
        mob.move_relative(
            speed,
            DVec3::new(
                f64::from(input.sideways()),
                f64::from(input.vertical()),
                f64::from(input.forward()),
            ),
        );
        mob.move_entity(MoverType::SelfMovement, mob.velocity());

        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path > 0 {
            return;
        }
        self.time_to_recalc_path = reduced_tick_delay(RECALC_INTERVAL_TICKS);

        let distance = mob.position().distance(following.position());
        match self.stage {
            Stage::GoToEntity => {
                let behind = following
                    .direction_yaw()
                    .opposite()
                    .relative(following.block_position())
                    .offset(0, -1, 0);
                mob.move_to_pos(
                    DVec3::new(
                        f64::from(behind.x()),
                        f64::from(behind.y()),
                        f64::from(behind.z()),
                    ),
                    1.0,
                );
                if distance < SWITCH_TO_LEADING_DISTANCE {
                    self.time_to_recalc_path = 0;
                    self.stage = Stage::GoInEntityDirection;
                }
            }
            Stage::GoInEntityDirection => {
                let direction = following.direction_yaw();
                let (step_x, step_z) = direction.offset_xz();
                let ahead = following.block_position().offset(
                    step_x * LEAD_DISTANCE,
                    0,
                    step_z * LEAD_DISTANCE,
                );
                mob.move_to_pos(
                    DVec3::new(
                        f64::from(ahead.x()),
                        f64::from(ahead.y() - 1),
                        f64::from(ahead.z()),
                    ),
                    1.0,
                );
                if distance > SWITCH_TO_CHASING_DISTANCE {
                    self.time_to_recalc_path = 0;
                    self.stage = Stage::GoToEntity;
                }
            }
        }
    }
}
