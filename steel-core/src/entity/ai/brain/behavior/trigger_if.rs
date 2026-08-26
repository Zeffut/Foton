//! Vanilla `BehaviorBuilder.triggerIf`.

use super::{BrainContext, Trigger};

use crate::entity::PathfinderMob;

/// The condition a `TriggerIf` reports.
type Condition = Box<dyn Fn(&dyn PathfinderMob) -> bool + Send>;

/// Succeeds when a condition holds, and does nothing else.
///
/// Vanilla parity: `BehaviorBuilder.triggerIf(Predicate)`. It exists to fill a
/// slot in a weighted [`super::RunOne`]: a frog's idle list has one of these
/// against `Entity::onGround`, so a frog standing still spends some of its rolls
/// doing nothing at all rather than always croaking or strolling.
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
            condition: Box::new(condition),
            debug_name,
        }
    }
}

impl Trigger for TriggerIf {
    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        (self.condition)(ctx.mob())
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}
