//! Lining up a fireball.

use std::sync::Arc;

use glam::DVec3;
use steel_math::{fast_floor, trig};
use steel_registry::{level_events, vanilla_entities};
use steel_utils::locks::SyncMutex;

use super::charging_player::{ARRIVED_DISTANCE_SQR, LOST_DISTANCE_SQR};
use super::{
    DragonPhaseInstance, EnderDragon, EnderDragonPhase, navigate_to_next_path_node,
    wrap_ring_target,
};
use crate::entity::ai::node::Node;
use crate::entity::ai::path::Path;
use crate::entity::entities::DragonFireballEntity;
use crate::entity::projectile::HurtingProjectile as _;
use crate::entity::{Entity as _, LivingEntity as _, SharedEntity, next_entity_id};
use crate::player::Player;
use crate::world::World;

/// Ticks of clean line-of-sight before the fireball is let go.
///
/// Vanilla parity: `DragonStrafePlayerPhase.FIREBALL_CHARGE_AMOUNT`.
const FIREBALL_CHARGE_AMOUNT: i32 = 5;

/// Squared distance inside which the dragon bothers charging a fireball.
///
/// Vanilla parity: the `distanceToSqr(this.dragon) < 4096.0` of `doServerTick`.
const FIREBALL_RANGE_SQR: f64 = 4096.0;

/// How far off aim the dragon may be and still fire, in degrees.
///
/// Vanilla parity: the `angleDegs >= 0.0F && angleDegs < 10.0F` of the same.
const FIREBALL_AIM_TOLERANCE_DEGREES: f32 = 10.0;

/// How far in front of the head the fireball starts.
///
/// Vanilla parity: the `1.0` the view vector is scaled by.
const FIREBALL_SPAWN_OFFSET: f64 = 1.0;

/// The dragon flies at one player and spits at them.
///
/// Vanilla parity: `DragonStrafePlayerPhase`.
pub struct DragonStrafePlayerPhase {
    state: SyncMutex<StrafeState>,
}

struct StrafeState {
    fireball_charge: i32,
    current_path: Option<Path>,
    target_location: Option<DVec3>,
    attack_target: Option<Arc<Player>>,
    holding_pattern_clockwise: bool,
}

impl Default for DragonStrafePlayerPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonStrafePlayerPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SyncMutex::new(StrafeState {
                fireball_charge: 0,
                current_path: None,
                target_location: None,
                attack_target: None,
                holding_pattern_clockwise: false,
            }),
        }
    }

    /// Aims the strafe at one player and paths to them.
    ///
    /// Vanilla parity: `DragonStrafePlayerPhase.setTarget`.
    pub fn set_target(&self, dragon: &EnderDragon, target: &Arc<Player>) {
        let Some(world) = dragon.level() else {
            return;
        };

        let target_position = target.position();
        let current_node = dragon.find_closest_node_to_self(&world);
        let target_node = dragon.find_closest_node(
            &world,
            target_position.x,
            target_position.y,
            target_position.z,
        );

        let final_x = fast_floor(target_position.x);
        let final_z = fast_floor(target_position.z);
        let dragon_position = dragon.position();
        let horizontal =
            (f64::from(final_x) - dragon_position.x).hypot(f64::from(final_z) - dragon_position.z);
        let height_offset = strafe_height_offset(horizontal);
        let final_y = fast_floor(target_position.y + height_offset);
        let final_node = Node::new(final_x, final_y, final_z);

        let mut state = self.state.lock();
        state.attack_target = Some(target.clone());
        state.current_path = dragon.find_path(&world, current_node, target_node, Some(final_node));
        if let Some(path) = state.current_path.as_mut() {
            path.advance();
            if let Some(location) = navigate_to_next_path_node(path) {
                state.target_location = Some(location);
            }
        }
    }

    /// Vanilla parity: `DragonStrafePlayerPhase.findNewTarget`.
    fn find_new_target(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let mut state = self.state.lock();
        if state.current_path.as_ref().is_none_or(Path::is_done) {
            let current_node = dragon.find_closest_node_to_self(world);
            let mut target_node = current_node as i32;
            if rand::random_range(0..8) == 0 {
                state.holding_pattern_clockwise = !state.holding_pattern_clockwise;
                target_node += 6;
            }

            if state.holding_pattern_clockwise {
                target_node += 1;
            } else {
                target_node -= 1;
            }

            let target_node = wrap_ring_target(
                target_node,
                dragon.has_fight() && dragon.alive_crystals() > 0,
            );
            state.current_path = dragon.find_path(world, current_node, target_node, None);
            if let Some(path) = state.current_path.as_mut() {
                path.advance();
            }
        }

        if let Some(path) = state.current_path.as_mut()
            && let Some(location) = navigate_to_next_path_node(path)
        {
            state.target_location = Some(location);
        }
    }

    /// Spits a fireball at the target and returns to the holding pattern.
    ///
    /// Vanilla parity: the innermost branch of `doServerTick`.
    fn shoot_fireball(&self, dragon: &EnderDragon, world: &Arc<World>, target: &Arc<Player>) {
        let view = dragon.look_angle();
        let head = dragon.head();
        let start = DVec3::new(
            head.position().x - view.x * FIREBALL_SPAWN_OFFSET,
            head.y_at(0.5) + 0.5,
            head.position().z - view.z * FIREBALL_SPAWN_OFFSET,
        );
        let direction = DVec3::new(
            target.position().x - start.x,
            super::y_at(target.as_ref(), 0.5) - start.y,
            target.position().z - start.z,
        );

        if !dragon.is_silent() {
            world.level_event(
                level_events::SOUND_DRAGON_FIREBALL,
                dragon.block_position(),
                0,
                None,
            );
        }

        let fireball = Arc::new(DragonFireballEntity::new(
            &vanilla_entities::DRAGON_FIREBALL,
            next_entity_id(),
            start,
            Arc::downgrade(world),
        ));
        // Vanilla parity: `new DragonFireball(level, this.dragon, direction)`
        // then `snapTo(startingX, startingY, startingZ, 0, 0)`. The constructor
        // is what sets the owner and the heading; Steel spawns at the muzzle and
        // shoots from there, the same way the wither spits a skull.
        if let Some(owner) = world.get_entity_by_id(dragon.id()) {
            fireball.shoot_from_owner(&owner, direction);
        } else {
            fireball.set_rotation((0.0, 0.0));
            fireball.assign_directional_movement(direction);
        }

        let entity: SharedEntity = fireball;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn the dragon's fireball: {error}");
        }

        let mut state = self.state.lock();
        state.fireball_charge = 0;
        if let Some(path) = state.current_path.as_mut() {
            while !path.is_done() {
                path.advance();
            }
        }
        drop(state);

        dragon
            .phase_manager()
            .set_phase(dragon, EnderDragonPhase::HoldingPattern);
    }
}

impl DragonPhaseInstance for DragonStrafePlayerPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::StrafePlayer
    }

    fn do_server_tick(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let Some(target) = self.state.lock().attack_target.clone() else {
            log::warn!("Skipping player strafe phase because no player was found");
            dragon
                .phase_manager()
                .set_phase(dragon, EnderDragonPhase::HoldingPattern);
            return;
        };

        let dragon_position = dragon.position();
        let target_position = target.position();
        {
            let mut state = self.state.lock();
            if state.current_path.as_ref().is_some_and(Path::is_done) {
                let horizontal = (target_position.x - dragon_position.x)
                    .hypot(target_position.z - dragon_position.z);
                state.target_location = Some(DVec3::new(
                    target_position.x,
                    target_position.y + strafe_height_offset(horizontal),
                    target_position.z,
                ));
            }
        }

        let dist_to_target = self
            .state
            .lock()
            .target_location
            .map_or(0.0, |location| location.distance_squared(dragon_position));
        if dist_to_target < ARRIVED_DISTANCE_SQR || dist_to_target > LOST_DISTANCE_SQR {
            self.find_new_target(dragon, world);
        }

        let in_range = target_position.distance_squared(dragon_position) < FIREBALL_RANGE_SQR;
        if !in_range || !dragon.has_line_of_sight(target.as_ref()) {
            let mut state = self.state.lock();
            if state.fireball_charge > 0 {
                state.fireball_charge -= 1;
            }
            return;
        }

        let charge = {
            let mut state = self.state.lock();
            state.fireball_charge += 1;
            state.fireball_charge
        };

        let y_rot = f64::from(dragon.rotation().0).to_radians();
        let aim = DVec3::new(
            target_position.x - dragon_position.x,
            0.0,
            target_position.z - dragon_position.z,
        )
        .normalize_or_zero();
        let dir = DVec3::new(
            f64::from(trig::sin(y_rot)),
            0.0,
            -f64::from(trig::cos(y_rot)),
        )
        .normalize_or_zero();
        let angle_degrees = (dir.dot(aim) as f32).acos().to_degrees() + 0.5;

        if charge >= FIREBALL_CHARGE_AMOUNT
            && (0.0..FIREBALL_AIM_TOLERANCE_DEGREES).contains(&angle_degrees)
        {
            self.shoot_fireball(dragon, world, &target);
        }
    }

    fn begin(&self, _dragon: &EnderDragon) {
        let mut state = self.state.lock();
        state.fireball_charge = 0;
        state.target_location = None;
        state.current_path = None;
        state.attack_target = None;
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        self.state.lock().target_location
    }

    fn as_strafe_player(&self) -> Option<&Self> {
        Some(self)
    }
}

/// How far above the target the dragon aims while strafing.
///
/// Vanilla parity: the `Math.min(0.4F + dist / 80.0 - 1.0, 10.0)` that both
/// `setTarget` and `doServerTick` compute.
fn strafe_height_offset(horizontal_distance: f64) -> f64 {
    (f64::from(0.4_f32) + horizontal_distance / 80.0 - 1.0).min(10.0)
}
