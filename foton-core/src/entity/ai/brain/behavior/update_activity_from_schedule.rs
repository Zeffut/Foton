//! Vanilla `UpdateActivityFromSchedule`.

use super::{BrainContext, Trigger};

/// Re-reads the brain's schedule attribute and switches activity to match.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.UpdateActivityFromSchedule`.
///
/// This is what actually turns the clock into behavior. Every villager activity
/// package but PANIC and HIDE ends with it at priority 99, so the lowest-priority
/// thing a working villager does is notice that it is no longer working hours.
/// The twenty-tick throttle lives in [`Brain::update_activity_from_schedule`],
/// exactly as it does in vanilla, so scheduling this every tick is cheap.
///
/// [`Brain::update_activity_from_schedule`]: crate::entity::ai::brain::Brain::update_activity_from_schedule
pub struct UpdateActivityFromSchedule;

impl Trigger for UpdateActivityFromSchedule {
    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .update_activity_from_schedule(ctx.world(), ctx.game_time());
        true
    }

    fn debug_name(&self) -> &'static str {
        "UpdateActivityFromSchedule"
    }
}
