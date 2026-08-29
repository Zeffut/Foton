//! What a sensor or behavior is handed on every call.

use std::sync::Arc;

use super::Brain;
use crate::entity::PathfinderMob;
use crate::world::World;

/// One brain tick's arguments, bundled.
///
/// Vanilla passes `(ServerLevel level, E body, long timestamp)` to each of
/// `Sensor.tick`, `BehaviorControl.tryStart`, `tickOrStop` and `doStop`. Foton
/// bundles them and adds the brain, because a Foton entity reaches its world
/// through a `Weak` upgrade and its brain through a trait method, and doing
/// either once per behavior per tick instead of once per brain tick is waste.
pub struct BrainContext<'a> {
    world: &'a Arc<World>,
    mob: &'a dyn PathfinderMob,
    brain: &'a Brain,
    game_time: i64,
}

impl<'a> BrainContext<'a> {
    /// Bundles one brain tick's arguments.
    #[must_use]
    pub const fn new(
        world: &'a Arc<World>,
        mob: &'a dyn PathfinderMob,
        brain: &'a Brain,
        game_time: i64,
    ) -> Self {
        Self {
            world,
            mob,
            brain,
            game_time,
        }
    }

    /// The world the body is in. Vanilla's `ServerLevel level`.
    #[must_use]
    pub const fn world(&self) -> &'a Arc<World> {
        self.world
    }

    /// The mob this brain drives. Vanilla's `E body`.
    #[must_use]
    pub const fn mob(&self) -> &'a dyn PathfinderMob {
        self.mob
    }

    /// The body's brain. Vanilla's `body.getBrain()`.
    #[must_use]
    pub const fn brain(&self) -> &'a Brain {
        self.brain
    }

    /// The game time this brain tick started at. Vanilla's `long timestamp`.
    #[must_use]
    pub const fn game_time(&self) -> i64 {
        self.game_time
    }
}
