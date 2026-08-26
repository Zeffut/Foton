//! Coming down onto the podium, and leaving it again.

use std::sync::Arc;

use glam::DVec3;
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;

use super::charging_player::{ARRIVED_DISTANCE_SQR, LOST_DISTANCE_SQR};
use super::dying::bottom_center_of;
use super::{
    DragonPhaseInstance, EnderDragon, EnderDragonPhase, horizontal_distance,
    navigate_to_next_path_node,
};
use crate::chunk::heightmap::HeightmapType;
use crate::entity::Entity as _;
use crate::entity::ai::node::Node;
use crate::entity::ai::path::Path;
use crate::entity::ai::targeting::TargetingConditions;
use crate::world::World;

/// Squared distance the landing phase counts as "on the podium".
///
/// Vanilla parity: the `distanceToSqr(...) < 1.0` of `DragonLandingPhase`.
const LANDED_DISTANCE_SQR: f64 = 1.0;

/// How close the takeoff has to stay to the podium to keep climbing.
///
/// Vanilla parity: the `closerToCenterThan(this.dragon.position(), 10.0)` of
/// `DragonTakeoffPhase`.
const TAKEOFF_PODIUM_RADIUS: f64 = 10.0;

/// Height the landing approach aims for when it has a player to swing around.
///
/// Vanilla parity: the `105.0` of `DragonLandingApproachPhase.findNewTarget`.
const APPROACH_SWING_HEIGHT: f64 = 105.0;

/// How far out the approach swings before turning in.
///
/// Vanilla parity: the `-aim.x * 40.0` of the same method.
const APPROACH_SWING_RADIUS: f64 = 40.0;

/// Flying in towards the podium.
///
/// Vanilla parity: `DragonLandingApproachPhase`.
pub struct DragonLandingApproachPhase {
    state: SyncMutex<ApproachState>,
}

#[derive(Default)]
struct ApproachState {
    current_path: Option<Path>,
    target_location: Option<DVec3>,
}

impl Default for DragonLandingApproachPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonLandingApproachPhase {
    /// Creates the phase.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SyncMutex::new(ApproachState::default()),
        }
    }

    /// Vanilla parity: `DragonLandingApproachPhase.findNewTarget`.
    fn find_new_target(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let mut state = self.state.lock();
        if state.current_path.as_ref().is_none_or(Path::is_done) {
            let current_node = dragon.find_closest_node_to_self(world);
            let egg = world.heightmap_pos(
                HeightmapType::MotionBlockingNoLeaves,
                super::super::end_podium_location(dragon.fight_origin()),
            );
            // Vanilla parity: `TargetingConditions.forCombat().ignoreLineOfSight()`.
            let conditions = TargetingConditions::for_combat().ignore_line_of_sight();
            let nearest =
                super::nearest_player_to(world, dragon, &conditions, super::corner_of(egg));

            let target_node = match nearest {
                Some(player) => {
                    let position = player.position();
                    let aim = DVec3::new(position.x, 0.0, position.z).normalize_or_zero();
                    dragon.find_closest_node(
                        world,
                        -aim.x * APPROACH_SWING_RADIUS,
                        APPROACH_SWING_HEIGHT,
                        -aim.z * APPROACH_SWING_RADIUS,
                    )
                }
                None => {
                    dragon.find_closest_node(world, APPROACH_SWING_RADIUS, f64::from(egg.y()), 0.0)
                }
            };

            let final_node = Node::new(egg.x(), egg.y(), egg.z());
            state.current_path =
                dragon.find_path(world, current_node, target_node, Some(final_node));
            if let Some(path) = state.current_path.as_mut() {
                path.advance();
            }
        }

        if let Some(path) = state.current_path.as_mut()
            && let Some(target) = navigate_to_next_path_node(path)
        {
            state.target_location = Some(target);
        }

        let landed = state.current_path.as_ref().is_some_and(Path::is_done);
        drop(state);
        if landed {
            dragon
                .phase_manager()
                .set_phase(dragon, EnderDragonPhase::Landing);
        }
    }
}

impl DragonPhaseInstance for DragonLandingApproachPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::LandingApproach
    }

    fn begin(&self, _dragon: &EnderDragon) {
        let mut state = self.state.lock();
        state.current_path = None;
        state.target_location = None;
    }

    fn do_server_tick(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let dist_to_target = self
            .state
            .lock()
            .target_location
            .map_or(0.0, |target| target.distance_squared(dragon.position()));
        if dist_to_target < ARRIVED_DISTANCE_SQR
            || dist_to_target > LOST_DISTANCE_SQR
            || dragon.horizontal_collision()
            || dragon.vertical_collision()
        {
            self.find_new_target(dragon, world);
        }
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        self.state.lock().target_location
    }
}

/// Dropping onto the podium.
///
/// Vanilla parity: `DragonLandingPhase`.
pub struct DragonLandingPhase {
    target_location: SyncMutex<Option<DVec3>>,
}

impl Default for DragonLandingPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonLandingPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            target_location: SyncMutex::new(None),
        }
    }
}

impl DragonPhaseInstance for DragonLandingPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::Landing
    }

    fn begin(&self, _dragon: &EnderDragon) {
        *self.target_location.lock() = None;
    }

    fn do_server_tick(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let target = {
            let mut target = self.target_location.lock();
            if target.is_none() {
                let egg = world.heightmap_pos(
                    HeightmapType::MotionBlockingNoLeaves,
                    super::super::end_podium_location(dragon.fight_origin()),
                );
                *target = Some(bottom_center_of(egg));
            }
            *target
        };

        let Some(target) = target else {
            return;
        };
        if target.distance_squared(dragon.position()) >= LANDED_DISTANCE_SQR {
            return;
        }

        let manager = dragon.phase_manager();
        // Vanilla parity: the flame count is reset before the handover, so a
        // dragon that lands again gets a fresh four breaths.
        if let Some(flaming) = manager
            .instance(EnderDragonPhase::SittingFlaming)
            .as_sitting_flaming()
        {
            flaming.reset_flame_count();
        }
        manager.set_phase(dragon, EnderDragonPhase::SittingScanning);
    }

    fn fly_speed(&self) -> f32 {
        1.5
    }

    /// Vanilla parity: `DragonLandingPhase.getTurnSpeed`, which is the shared
    /// body without the `0.7 /` -- the landing turn is much tighter.
    fn turn_speed(&self, dragon: &EnderDragon) -> f32 {
        let rot_speed = horizontal_distance(dragon.velocity()) as f32 + 1.0;
        let dist = rot_speed.min(40.0);
        dist / rot_speed
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        *self.target_location.lock()
    }
}

/// Leaving the podium.
///
/// Vanilla parity: `DragonTakeoffPhase`.
pub struct DragonTakeoffPhase {
    state: SyncMutex<TakeoffState>,
}

struct TakeoffState {
    first_tick: bool,
    current_path: Option<Path>,
    target_location: Option<DVec3>,
}

impl Default for DragonTakeoffPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonTakeoffPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SyncMutex::new(TakeoffState {
                first_tick: false,
                current_path: None,
                target_location: None,
            }),
        }
    }

    /// Vanilla parity: `DragonTakeoffPhase.findNewTarget`.
    fn find_new_target(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let current_node = dragon.find_closest_node_to_self(world);
        let look = dragon.head_look_vector(1.0);
        let target_node = dragon.find_closest_node(
            world,
            -look.x * APPROACH_SWING_RADIUS,
            APPROACH_SWING_HEIGHT,
            -look.z * APPROACH_SWING_RADIUS,
        );
        let target_node = super::wrap_ring_target(target_node as i32, dragon.alive_crystals() > 0);

        let mut state = self.state.lock();
        state.current_path = dragon.find_path(world, current_node, target_node, None);

        // Vanilla parity: takeoff's `navigateToNextPathNode` advances once
        // before reading the node, unlike the other three copies of it.
        if let Some(path) = state.current_path.as_mut() {
            path.advance();
            if let Some(target) = navigate_to_next_path_node(path) {
                state.target_location = Some(target);
            }
        }
    }
}

impl DragonPhaseInstance for DragonTakeoffPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::Takeoff
    }

    fn begin(&self, _dragon: &EnderDragon) {
        let mut state = self.state.lock();
        state.first_tick = true;
        state.current_path = None;
        state.target_location = None;
    }

    fn do_server_tick(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let climbing = {
            let mut state = self.state.lock();
            if state.first_tick || state.current_path.is_none() {
                state.first_tick = false;
                false
            } else {
                true
            }
        };

        if !climbing {
            self.find_new_target(dragon, world);
            return;
        }

        let egg = world.heightmap_pos(
            HeightmapType::MotionBlockingNoLeaves,
            super::super::end_podium_location(dragon.fight_origin()),
        );
        if !closer_to_center_than(egg, dragon.position(), TAKEOFF_PODIUM_RADIUS) {
            dragon
                .phase_manager()
                .set_phase(dragon, EnderDragonPhase::HoldingPattern);
        }
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        self.state.lock().target_location
    }
}

/// Vanilla `Vec3i.closerToCenterThan`.
fn closer_to_center_than(pos: BlockPos, point: DVec3, distance: f64) -> bool {
    let center = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    );
    center.distance_squared(point) < distance * distance
}
