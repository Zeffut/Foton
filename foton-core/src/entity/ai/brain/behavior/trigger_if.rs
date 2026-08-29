//! Vanilla `BehaviorBuilder.triggerIf`.

use std::sync::Arc;

use super::{BrainContext, Trigger};

use crate::entity::PathfinderMob;
use crate::world::World;

/// The condition a `TriggerIf` reports.
///
/// Vanilla has one overload taking the body alone and one taking the level
/// beside it; both collapse into a test on the tick's context here.
type Condition = Box<dyn Fn(&BrainContext<'_>) -> bool + Send>;

/// Succeeds when a condition holds, and does nothing else.
///
/// Vanilla parity: `BehaviorBuilder.triggerIf(Predicate)`. It exists to fill a
/// slot in a weighted [`super::RunOne`]: a frog's idle list has one of these
/// against `Entity::onGround`, so a frog standing still spends some of its rolls
/// doing nothing at all rather than always croaking or strolling. Vanilla's
/// other use is as the first half of a [`super::Sequence`], which is how a
/// raiding village gates a whole behavior on what its raid is doing.
pub struct TriggerIf {
    condition: Condition,
    debug_name: &'static str,
}

impl TriggerIf {
    /// Succeeds while `condition` holds.
    #[must_use]
    pub fn new(
        debug_name: &'static str,
        condition: impl Fn(&dyn PathfinderMob) -> bool + Send + 'static,
    ) -> Self {
        Self {
            condition: Box::new(move |ctx| condition(ctx.mob())),
            debug_name,
        }
    }

    /// Succeeds while `condition` holds of the body and the world it is in.
    ///
    /// Vanilla parity: the `BiPredicate<ServerLevel, E>` overload of
    /// `BehaviorBuilder.triggerIf`.
    #[must_use]
    pub fn with_level(
        debug_name: &'static str,
        condition: impl Fn(&Arc<World>, &dyn PathfinderMob) -> bool + Send + 'static,
    ) -> Self {
        Self {
            condition: Box::new(move |ctx| condition(ctx.world(), ctx.mob())),
            debug_name,
        }
    }
}

impl Trigger for TriggerIf {
    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        (self.condition)(ctx)
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}
