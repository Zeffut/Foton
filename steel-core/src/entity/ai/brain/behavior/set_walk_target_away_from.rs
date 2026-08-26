//! Vanilla `SetWalkTargetAwayFrom`.

use glam::DVec3;

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleId, MemoryModuleType, WalkTarget, memory_module_types,
};
use crate::entity::ai::goal::land_random_pos_away;

/// Vanilla parity: the `RandomPos.generateRandomPos` retry budget of `create`.
const FLEE_ATTEMPTS: usize = 10;
/// Vanilla parity: the `16` horizontal range of the `LandRandomPos.getPosAway` call.
const FLEE_HORIZONTAL_RANGE: i32 = 16;
/// Vanilla parity: the `7` vertical range of the same call.
const FLEE_VERTICAL_RANGE: i32 = 7;

/// What the body is fleeing from.
enum AvoidSource {
    /// Vanilla parity: `SetWalkTargetAwayFrom.pos`.
    Position(MemoryModuleType<steel_utils::BlockPos>),
    /// Vanilla parity: `SetWalkTargetAwayFrom.entity`.
    Entity(MemoryModuleType<EntityMemory>),
}

/// Walks away from whatever a memory points at.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetWalkTargetAwayFrom`.
pub struct SetWalkTargetAwayFrom {
    source: AvoidSource,
    speed_modifier: f64,
    desired_distance: f64,
    interrupt_current_walk: bool,
}

impl SetWalkTargetAwayFrom {
    /// Flees a remembered block position.
    ///
    /// Vanilla parity: `SetWalkTargetAwayFrom.pos`.
    #[must_use]
    pub const fn pos(
        memory: MemoryModuleType<steel_utils::BlockPos>,
        speed_modifier: f64,
        desired_distance: i32,
        interrupt_current_walk: bool,
    ) -> Self {
        Self {
            source: AvoidSource::Position(memory),
            speed_modifier,
            desired_distance: desired_distance as f64,
            interrupt_current_walk,
        }
    }

    /// Flees a remembered entity.
    ///
    /// Vanilla parity: `SetWalkTargetAwayFrom.entity`.
    #[must_use]
    pub const fn entity(
        memory: MemoryModuleType<EntityMemory>,
        speed_modifier: f64,
        desired_distance: i32,
        interrupt_current_walk: bool,
    ) -> Self {
        Self {
            source: AvoidSource::Entity(memory),
            speed_modifier,
            desired_distance: desired_distance as f64,
            interrupt_current_walk,
        }
    }

    const fn memory(&self) -> MemoryModuleId {
        match &self.source {
            AvoidSource::Position(memory) => memory.id(),
            AvoidSource::Entity(memory) => memory.id(),
        }
    }

    /// Resolves the memory into the point to run from.
    fn avoid_position(&self, ctx: &BrainContext<'_>) -> Option<DVec3> {
        match &self.source {
            // Vanilla parity: the `Vec3::atBottomCenterOf` of the block form.
            AvoidSource::Position(memory) => ctx.brain().get_memory(*memory).map(|pos| {
                DVec3::new(
                    f64::from(pos.x()) + 0.5,
                    f64::from(pos.y()),
                    f64::from(pos.z()) + 0.5,
                )
            }),
            AvoidSource::Entity(memory) => ctx
                .brain()
                .get_memory(*memory)
                .and_then(|remembered| remembered.get())
                .map(|entity| entity.position()),
        }
    }
}

impl Trigger for SetWalkTargetAwayFrom {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::WALK_TARGET.id(), self.memory()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let current = brain.get_memory(memory_module_types::WALK_TARGET);
        if current.is_some() && !self.interrupt_current_walk {
            return false;
        }
        let Some(avoid_position) = self.avoid_position(ctx) else {
            return false;
        };

        let body_position = ctx.mob().position();
        if body_position.distance_squared(avoid_position)
            > self.desired_distance * self.desired_distance
        {
            return false;
        }

        // Vanilla parity: a walk already heading away at the same speed is left
        // alone, so a fleeing mob does not re-roll its escape every tick.
        if let Some(current) = current
            && (current.speed_modifier() - self.speed_modifier).abs() < f64::EPSILON
            && let Some(current_position) = current.target().current_position()
        {
            let current_direction = current_position - body_position;
            let avoid_direction = avoid_position - body_position;
            if current_direction.dot(avoid_direction) < 0.0 {
                return false;
            }
        }

        for _ in 0..FLEE_ATTEMPTS {
            if let Some(flee_to) = land_random_pos_away(
                ctx.mob(),
                FLEE_HORIZONTAL_RANGE,
                FLEE_VERTICAL_RANGE,
                avoid_position,
            ) {
                brain.set_memory(
                    memory_module_types::WALK_TARGET,
                    WalkTarget::of_position(flee_to, self.speed_modifier, 0),
                );
                break;
            }
        }

        true
    }

    fn debug_name(&self) -> &'static str {
        "SetWalkTargetAwayFrom"
    }
}
