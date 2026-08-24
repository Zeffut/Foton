//! Vanilla `DoNothing`.

use super::{BehaviorControl, BehaviorStatus, BrainContext};
use crate::entity::ai::brain::memory::MemoryModuleId;

/// Occupies its priority slot for a while so nothing below it runs.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.DoNothing`. It
/// implements `BehaviorControl` directly rather than extending `Behavior`
/// because it has no entry condition and no work to do, and that is exactly
/// what makes it useful inside a `RunOne`.
pub struct DoNothing {
    min_duration: i32,
    max_duration: i32,
    status: BehaviorStatus,
    end_timestamp: i64,
}

impl DoNothing {
    /// Idles for somewhere between `min_duration` and `max_duration` ticks.
    #[must_use]
    pub const fn new(min_duration: i32, max_duration: i32) -> Self {
        Self {
            min_duration,
            max_duration,
            status: BehaviorStatus::Stopped,
            end_timestamp: 0,
        }
    }
}

impl BehaviorControl for DoNothing {
    fn status(&self) -> BehaviorStatus {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleId> {
        Vec::new()
    }

    fn try_start(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.status = BehaviorStatus::Running;
        let duration =
            self.min_duration + rand::random_range(0..=(self.max_duration - self.min_duration));
        self.end_timestamp = ctx.game_time() + i64::from(duration);
        true
    }

    fn tick_or_stop(&mut self, ctx: &BrainContext<'_>) {
        if ctx.game_time() > self.end_timestamp {
            self.do_stop(ctx);
        }
    }

    fn do_stop(&mut self, _ctx: &BrainContext<'_>) {
        self.status = BehaviorStatus::Stopped;
    }

    fn debug_name(&self) -> &'static str {
        "DoNothing"
    }
}
