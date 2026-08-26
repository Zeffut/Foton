//! Vanilla `SocializeAtBell`.

use steel_registry::vanilla_entities;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Vanilla parity: `SocializeAtBell.SPEED_MODIFIER`.
const SPEED_MODIFIER: f64 = 0.3;
/// Vanilla parity: the `nextInt(100) == 0` that spaces conversations out.
const SOCIALIZE_CHANCE_IN: i32 = 100;
/// Vanilla parity: the `closerToCenterThan(body.position(), 4.0)` that keeps
/// this to villagers actually standing at the bell.
const AT_THE_BELL_DISTANCE: f64 = 4.0;
/// Vanilla parity: the `distanceToSqr(body) <= 32.0` of the partner search.
const PARTNER_DISTANCE_SQR: f64 = 32.0;
/// Vanilla parity: the `new WalkTarget(..., 0.3F, 1)`.
const CLOSE_ENOUGH_DIST: i32 = 1;

/// Picks a villager to stand and talk to at the bell.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SocializeAtBell`.
/// Half of what a gathering at the meeting point looks like: the other half is
/// the stroll around the bell it shares its gate with.
pub struct SocializeAtBell;

impl Trigger for SocializeAtBell {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::MEETING_POINT.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            memory_module_types::INTERACTION_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::INTERACTION_TARGET.id()) {
            return false;
        }
        let Some(meeting_point) = brain.get_memory(memory_module_types::MEETING_POINT) else {
            return false;
        };
        let Some(visible) = brain.get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        else {
            return false;
        };

        let mob = ctx.mob();
        if rand::random_range(0..SOCIALIZE_CHANCE_IN) != 0
            || meeting_point.dimension != ctx.world().key
            || !utils::block_closer_to_center_than(
                meeting_point.pos,
                mob.position(),
                AT_THE_BELL_DISTANCE,
            )
        {
            return false;
        }

        let body_position = mob.position();
        let partner = visible.find_closest(|candidate| {
            utils::is_of_type(candidate, &vanilla_entities::VILLAGER)
                && candidate.position().distance_squared(body_position) <= PARTNER_DISTANCE_SQR
        });
        // Vanilla returns `true` once the roll and the distance pass, whether or
        // not there was anybody to talk to.
        if let Some(partner) = partner {
            brain.set_memory(
                memory_module_types::INTERACTION_TARGET,
                utils::remember(&partner),
            );
            brain.set_memory(
                memory_module_types::LOOK_TARGET,
                PositionTracker::of_entity(&partner, true),
            );
            brain.set_memory(
                memory_module_types::WALK_TARGET,
                WalkTarget::new(
                    PositionTracker::of_entity(&partner, false),
                    SPEED_MODIFIER,
                    CLOSE_ENOUGH_DIST,
                ),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "SocializeAtBell"
    }
}
