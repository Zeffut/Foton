//! Circling.

use std::sync::Arc;

use foton_utils::BlockPos;
use foton_utils::locks::SyncMutex;
use glam::DVec3;

use super::charging_player::{ARRIVED_DISTANCE_SQR, LOST_DISTANCE_SQR};
use super::{
    DragonPhaseInstance, EnderDragon, EnderDragonPhase, navigate_to_next_path_node,
    wrap_ring_target,
};
use crate::chunk::heightmap::HeightmapType;
use crate::entity::ai::path::Path;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::entities::EndCrystalEntity;
use crate::entity::{Entity as _, LivingEntity as _};
use crate::player::Player;
use crate::world::World;

/// Divisor turning distance-from-podium into a strafe chance.
///
/// Vanilla parity: the `/ 512.0` of `findNewTarget`. The further the nearest
/// player is from the podium, the less often the dragon breaks off to strafe.
const STRAFE_DISTANCE_SCALE: f64 = 512.0;

/// The dragon's resting state: circling the ring.
///
/// Vanilla parity: `DragonHoldingPatternPhase`. Every trip around the ring it
/// rolls for whether to land, to strafe someone, or to keep going.
pub struct DragonHoldingPatternPhase {
    state: SyncMutex<HoldingState>,
}

struct HoldingState {
    current_path: Option<Path>,
    target_location: Option<DVec3>,
    clockwise: bool,
}

impl Default for DragonHoldingPatternPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonHoldingPatternPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SyncMutex::new(HoldingState {
                current_path: None,
                target_location: None,
                clockwise: false,
            }),
        }
    }

    /// Hands over to the strafe phase aimed at one player.
    ///
    /// Vanilla parity: `DragonHoldingPatternPhase.strafePlayer`.
    fn strafe_player(dragon: &EnderDragon, target: &Arc<Player>) {
        let manager = dragon.phase_manager();
        manager.set_phase(dragon, EnderDragonPhase::StrafePlayer);
        if let Some(strafe) = manager
            .instance(EnderDragonPhase::StrafePlayer)
            .as_strafe_player()
        {
            strafe.set_target(dragon, target);
        }
    }

    /// Vanilla parity: `DragonHoldingPatternPhase.findNewTarget`.
    fn find_new_target(&self, dragon: &EnderDragon, world: &Arc<World>) {
        if self.consider_leaving_the_ring(dragon, world) {
            return;
        }

        let mut state = self.state.lock();
        if state.current_path.as_ref().is_none_or(Path::is_done) {
            let current_node = dragon.find_closest_node_to_self(world);
            let mut target_node = current_node as i32;
            if rand::random_range(0..8) == 0 {
                state.clockwise = !state.clockwise;
                target_node += 6;
            }

            if state.clockwise {
                target_node += 1;
            } else {
                target_node -= 1;
            }

            // Vanilla parity: the holding pattern tests `aliveCrystals() >= 0`,
            // not `> 0` as the takeoff and strafe phases do. With a fight
            // attached that is always true, so a dragon in a fight stays on the
            // outer ring here even after the last crystal has gone; only a
            // dragon with no fight at all drops to the inner rings.
            let target_node = wrap_ring_target(
                target_node,
                dragon.has_fight() && dragon.alive_crystals() >= 0,
            );

            state.current_path = dragon.find_path(world, current_node, target_node, None);
            if let Some(path) = state.current_path.as_mut() {
                path.advance();
            }
        }

        if let Some(path) = state.current_path.as_mut()
            && let Some(target) = navigate_to_next_path_node(path)
        {
            state.target_location = Some(target);
        }
    }

    /// Rolls for landing or strafing at the end of a lap.
    ///
    /// Vanilla parity: the `if (this.currentPath != null && this.currentPath.isDone())`
    /// block that opens `findNewTarget`. Returns whether the dragon left the
    /// holding pattern.
    fn consider_leaving_the_ring(&self, dragon: &EnderDragon, world: &Arc<World>) -> bool {
        if !self
            .state
            .lock()
            .current_path
            .as_ref()
            .is_some_and(Path::is_done)
        {
            return false;
        }

        let egg = world.heightmap_pos(
            HeightmapType::MotionBlockingNoLeaves,
            super::super::end_podium_location(dragon.fight_origin()),
        );
        let crystals = dragon.alive_crystals();
        if rand::random_range(0..crystals + 3) == 0 {
            dragon
                .phase_manager()
                .set_phase(dragon, EnderDragonPhase::LandingApproach);
            return true;
        }

        // Vanilla computes a `distSqr = 64.0` fallback for the no-player case
        // and then never reads it: the strafe branch it feeds is guarded on the
        // player being non-null. Returning here is the same thing without it.
        // Vanilla parity: `TargetingConditions.forCombat().ignoreLineOfSight()`.
        let conditions = TargetingConditions::for_combat().ignore_line_of_sight();
        let Some(nearest) =
            super::nearest_player_to(world, dragon, &conditions, super::corner_of(egg))
        else {
            return false;
        };

        let dist_sqr = super::dist_to_center_sqr(egg, nearest.position()) / STRAFE_DISTANCE_SCALE;
        let weight = (dist_sqr + 2.0) as i32;
        if rand::random_range(0..weight.max(1)) == 0 || rand::random_range(0..crystals + 2) == 0 {
            Self::strafe_player(dragon, &nearest);
            return true;
        }

        false
    }
}

impl DragonPhaseInstance for DragonHoldingPatternPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::HoldingPattern
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

    fn begin(&self, _dragon: &EnderDragon) {
        let mut state = self.state.lock();
        state.current_path = None;
        state.target_location = None;
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        self.state.lock().target_location
    }

    /// Vanilla parity: `DragonHoldingPatternPhase.onCrystalDestroyed` -- the
    /// dragon turns on whoever broke a crystal.
    fn on_crystal_destroyed(
        &self,
        dragon: &EnderDragon,
        _world: &Arc<World>,
        _crystal: &EndCrystalEntity,
        _pos: BlockPos,
        _source: &DamageSource,
        player: Option<&Arc<Player>>,
    ) {
        let Some(player) = player else {
            return;
        };
        if dragon.can_attack(player.as_ref()) {
            Self::strafe_player(dragon, player);
        }
    }
}
