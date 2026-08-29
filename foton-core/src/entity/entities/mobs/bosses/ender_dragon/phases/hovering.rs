//! Holding still.

use std::sync::Arc;

use foton_utils::locks::SyncMutex;
use glam::DVec3;

use super::{DragonPhaseInstance, EnderDragon, EnderDragonPhase};
use crate::entity::Entity as _;
use crate::world::World;

/// The dragon hangs where it is.
///
/// Vanilla parity: `DragonHoverPhase`. This is the phase a dragon summoned
/// outside a fight sits in forever, and the one `/summon ender_dragon` lands a
/// dragon in.
pub struct DragonHoverPhase {
    target_location: SyncMutex<Option<DVec3>>,
}

impl Default for DragonHoverPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonHoverPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            target_location: SyncMutex::new(None),
        }
    }
}

impl DragonPhaseInstance for DragonHoverPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::Hovering
    }

    /// Vanilla parity: `DragonHoverPhase.isSitting` returns true even though
    /// the dragon is in the air, which is what stops a hovering dragon beating
    /// its wings at anyone standing under it.
    fn is_sitting(&self) -> bool {
        true
    }

    fn do_server_tick(&self, dragon: &EnderDragon, _world: &Arc<World>) {
        let mut target = self.target_location.lock();
        if target.is_none() {
            *target = Some(dragon.position());
        }
    }

    fn begin(&self, _dragon: &EnderDragon) {
        *self.target_location.lock() = None;
    }

    fn fly_speed(&self) -> f32 {
        1.0
    }

    fn fly_target_location(&self) -> Option<DVec3> {
        *self.target_location.lock()
    }
}
