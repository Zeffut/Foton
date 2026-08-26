//! Vanilla `FollowTemptation`.

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior};

use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Vanilla `FollowTemptation.TEMPTATION_COOLDOWN`.
const TEMPTATION_COOLDOWN: i32 = 100;
/// Vanilla `FollowTemptation.DEFAULT_CLOSE_ENOUGH_DIST`.
pub const DEFAULT_CLOSE_ENOUGH_DIST: f64 = 2.5;
/// How close the walk target asks the mob to get.
///
/// Vanilla parity: the `2` of the `WalkTarget` this behavior sets.
const WALK_TARGET_CLOSE_ENOUGH: i32 = 2;

/// How fast this mob follows the food, and how close it stops.
type MobDistance = Box<dyn Fn(&dyn PathfinderMob) -> f64 + Send>;

/// Walks toward the player holding what this mob wants.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.FollowTemptation`.
/// It is what makes an animal trail a player holding its food, and its stop is
/// what puts the hundred-tick cooldown on so the mob does not re-lock instantly.
pub struct FollowTemptation {
    speed_modifier: MobDistance,
    close_enough_distance: MobDistance,
    look_in_the_eyes: bool,
}

/// Vanilla parity: the `entryCondition` map built in the constructor.
const ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::TEMPTATION_COOLDOWN_TICKS.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::IS_TEMPTED.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::TEMPTING_PLAYER.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::BREED_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::IS_PANICKING.id(),
        MemoryStatus::ValueAbsent,
    ),
];

impl FollowTemptation {
    /// Follows at a fixed speed, stopping two and a half blocks short.
    ///
    /// Vanilla parity: `new FollowTemptation(Function<LivingEntity, Float>)`.
    #[must_use]
    pub fn new(speed_modifier: impl Fn(&dyn PathfinderMob) -> f64 + Send + 'static) -> Self {
        Self {
            speed_modifier: Box::new(speed_modifier),
            close_enough_distance: Box::new(|_| DEFAULT_CLOSE_ENOUGH_DIST),
            look_in_the_eyes: false,
        }
    }

    /// Stops at a distance this mob decides for itself.
    ///
    /// Vanilla parity: the three-argument constructor's `closeEnoughDistance`,
    /// which the sniffer uses to keep a baby closer than an adult.
    #[must_use]
    pub fn with_close_enough_distance(
        mut self,
        close_enough_distance: impl Fn(&dyn PathfinderMob) -> f64 + Send + 'static,
    ) -> Self {
        self.close_enough_distance = Box::new(close_enough_distance);
        self
    }

    /// Aims at the player's eyes rather than their feet.
    ///
    /// Vanilla parity: the `lookInTheEyes` flag.
    #[must_use]
    pub const fn looking_in_the_eyes(mut self) -> Self {
        self.look_in_the_eyes = true;
        self
    }
}

impl TimedBehavior for FollowTemptation {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        ENTRY_CONDITION
    }

    /// Vanilla parity: `FollowTemptation.timedOut`, which is `false` -- the
    /// behavior runs for as long as the player keeps holding the food out.
    fn times_out(&self) -> bool {
        false
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        brain
            .get_memory(memory_module_types::TEMPTING_PLAYER)
            .is_some()
            && !brain.has_memory_value(memory_module_types::BREED_TARGET.id())
            && !brain.has_memory_value(memory_module_types::IS_PANICKING.id())
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .set_memory(memory_module_types::IS_TEMPTED, true);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.set_memory(
            memory_module_types::TEMPTATION_COOLDOWN_TICKS,
            TEMPTATION_COOLDOWN,
        );
        brain.erase_memory(memory_module_types::IS_TEMPTED.id());
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(player) = ctx
            .brain()
            .get_memory(memory_module_types::TEMPTING_PLAYER)
            .and_then(|memory| memory.get())
        else {
            return;
        };

        let body = ctx.mob();
        let brain = ctx.brain();
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&player, true),
        );

        let close_enough = (self.close_enough_distance)(body);
        let distance_sqr = body.position().distance_squared(player.position());
        if distance_sqr < close_enough * close_enough {
            brain.erase_memory(memory_module_types::WALK_TARGET.id());
            return;
        }

        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::new(
                PositionTracker::of_entity(&player, self.look_in_the_eyes),
                (self.speed_modifier)(body),
                WALK_TARGET_CLOSE_ENOUGH,
            ),
        );
    }

    fn debug_name(&self) -> &'static str {
        "FollowTemptation"
    }
}
