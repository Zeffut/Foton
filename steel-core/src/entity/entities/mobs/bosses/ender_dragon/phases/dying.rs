//! The death spiral.

use std::sync::Arc;

use glam::DVec3;
use steel_utils::locks::SyncMutex;

use super::charging_player::{ARRIVED_DISTANCE_SQR, LOST_DISTANCE_SQR};
use super::{DragonPhaseInstance, EnderDragon, EnderDragonPhase};
use crate::chunk::heightmap::HeightmapType;
use crate::entity::{Entity as _, LivingEntity as _};
use crate::world::World;

/// The dragon flies at the podium and dies over it.
///
/// Vanilla parity: `DragonDeathPhase`. The health it sets is what drives the
/// whole animation: it holds the dragon at one point of health until it is over
/// the exit portal, then drops it to zero, which is what starts `tickDeath`.
pub struct DragonDeathPhase {
    target_location: SyncMutex<Option<DVec3>>,
}

impl Default for DragonDeathPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonDeathPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            target_location: SyncMutex::new(None),
        }
    }
}

impl DragonPhaseInstance for DragonDeathPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::Dying
    }

    fn do_server_tick(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let target = {
            let mut target = self.target_location.lock();
            if target.is_none() {
                // Vanilla parity: MOTION_BLOCKING here, not
                // MOTION_BLOCKING_NO_LEAVES -- the death phase is the one
                // podium lookup that uses it.
                let egg = world.heightmap_pos(
                    HeightmapType::MotionBlocking,
                    super::super::end_podium_location(dragon.fight_origin()),
                );
                *target = Some(bottom_center_of(egg));
            }
            *target
        };

        let Some(target) = target else {
            return;
        };

        let dist_to_target = target.distance_squared(dragon.position());
        let arrived = dist_to_target < ARRIVED_DISTANCE_SQR
            || dist_to_target > LOST_DISTANCE_SQR
            || dragon.horizontal_collision()
            || dragon.vertical_collision();
        dragon.set_health(if arrived { 0.0 } else { 1.0 });
    }

    fn begin(&self, _dragon: &EnderDragon) {
        *self.target_location.lock() = None;
    }

    fn fly_speed(&self) -> f32 {
        3.0
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        *self.target_location.lock()
    }
}

/// Vanilla `Vec3.atBottomCenterOf`.
pub(super) fn bottom_center_of(pos: steel_utils::BlockPos) -> DVec3 {
    DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()),
        f64::from(pos.z()) + 0.5,
    )
}
