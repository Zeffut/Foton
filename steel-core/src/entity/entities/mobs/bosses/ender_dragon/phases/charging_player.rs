//! Diving at a point.

use std::sync::Arc;

use glam::DVec3;
use steel_utils::locks::SyncMutex;

use super::{DragonPhaseInstance, EnderDragon, EnderDragonPhase};
use crate::entity::Entity as _;
use crate::world::World;

/// Ticks the dragon keeps flying after a charge has run out of road.
///
/// Vanilla parity: `DragonChargePlayerPhase.CHARGE_RECOVERY_TIME`.
const CHARGE_RECOVERY_TIME: i32 = 10;

/// Squared distance below which the charge counts as arrived.
///
/// Vanilla parity: the `distToTarget < 100.0` shared by every flying phase.
pub(super) const ARRIVED_DISTANCE_SQR: f64 = 100.0;

/// Squared distance past which the target counts as lost.
///
/// Vanilla parity: the `distToTarget > 22500.0` shared by every flying phase --
/// 150 blocks.
pub(super) const LOST_DISTANCE_SQR: f64 = 22_500.0;

/// The dragon charges a fixed point at speed.
///
/// Vanilla parity: `DragonChargePlayerPhase`. The target is a point, not a
/// player: the scan that starts a charge samples where the player was standing
/// and the dragon commits to it.
pub struct DragonChargePlayerPhase {
    state: SyncMutex<ChargeState>,
}

struct ChargeState {
    target_location: Option<DVec3>,
    time_since_charge: i32,
}

impl Default for DragonChargePlayerPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonChargePlayerPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SyncMutex::new(ChargeState {
                target_location: None,
                time_since_charge: 0,
            }),
        }
    }

    /// Aims the charge.
    ///
    /// Vanilla parity: `DragonChargePlayerPhase.setTarget`.
    pub fn set_target(&self, target: DVec3) {
        self.state.lock().target_location = Some(target);
    }
}

impl DragonPhaseInstance for DragonChargePlayerPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::ChargingPlayer
    }

    fn do_server_tick(&self, dragon: &EnderDragon, _world: &Arc<World>) {
        let give_up = {
            let mut state = self.state.lock();
            match state.target_location {
                None => {
                    log::warn!("Aborting charge player as no target was set.");
                    true
                }
                Some(_) if state.time_since_charge > 0 => {
                    state.time_since_charge += 1;
                    state.time_since_charge >= CHARGE_RECOVERY_TIME
                }
                Some(target) => {
                    let dist_to_target = target.distance_squared(dragon.position());
                    if dist_to_target < ARRIVED_DISTANCE_SQR
                        || dist_to_target > LOST_DISTANCE_SQR
                        || dragon.horizontal_collision()
                        || dragon.vertical_collision()
                    {
                        state.time_since_charge += 1;
                    }
                    false
                }
            }
        };

        if give_up {
            dragon
                .phase_manager()
                .set_phase(dragon, EnderDragonPhase::HoldingPattern);
        }
    }

    fn begin(&self, _dragon: &EnderDragon) {
        let mut state = self.state.lock();
        state.target_location = None;
        state.time_since_charge = 0;
    }

    fn fly_speed(&self) -> f32 {
        3.0
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        self.state.lock().target_location
    }
}
