//! Vanilla `InteractWith` and `SetLookAndInteract`.

use steel_registry::entity_type::EntityTypeRef;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleId, MemoryModuleType, WalkTarget, memory_module_types,
};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Walks up to a nearby mob of one type and remembers it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.InteractWith`.
pub struct InteractWith {
    entity_type: EntityTypeRef,
    interaction_range_sqr: f64,
    interaction_target: MemoryModuleType<EntityMemory>,
    speed_modifier: f64,
    stop_distance: i32,
}

impl InteractWith {
    /// Vanilla parity: `InteractWith.of(EntityType, int, MemoryModuleType, float, int)`.
    #[must_use]
    pub fn of(
        entity_type: EntityTypeRef,
        interaction_range: i32,
        interaction_target: MemoryModuleType<EntityMemory>,
        speed_modifier: f64,
        stop_distance: i32,
    ) -> Self {
        Self {
            entity_type,
            interaction_range_sqr: f64::from(interaction_range) * f64::from(interaction_range),
            interaction_target,
            speed_modifier,
            stop_distance,
        }
    }
}

impl Trigger for InteractWith {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            self.interaction_target.id(),
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }
        let Some(visible) = brain.get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        else {
            return false;
        };

        let body_position = ctx.mob().position();
        let entity_type = self.entity_type;
        // Vanilla reports success as soon as any candidate of the right type is
        // visible, whether or not one is close enough to walk to.
        let Some(any_of_type) = visible.find_closest(|candidate| {
            utils::is_of_type(candidate.as_entity_event_source(), entity_type)
        }) else {
            return false;
        };
        drop(any_of_type);

        let interaction_range_sqr = self.interaction_range_sqr;
        let Some(closest) = visible.find_closest(|candidate| {
            candidate.position().distance_squared(body_position) <= interaction_range_sqr
                && utils::is_of_type(candidate.as_entity_event_source(), entity_type)
        }) else {
            return true;
        };

        brain.set_memory(self.interaction_target, EntityMemory::new(&closest));
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&closest, true),
        );
        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_entity(&closest, self.speed_modifier, self.stop_distance),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "InteractWith"
    }
}

/// Turns to face the nearest mob of one type and remembers it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetLookAndInteract`.
pub struct SetLookAndInteract {
    entity_type: EntityTypeRef,
    interaction_range_sqr: f64,
}

impl SetLookAndInteract {
    /// Vanilla parity: `SetLookAndInteract.create`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, interaction_range: i32) -> Self {
        Self {
            entity_type,
            interaction_range_sqr: f64::from(interaction_range) * f64::from(interaction_range),
        }
    }
}

impl Trigger for SetLookAndInteract {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::INTERACTION_TARGET.id(),
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::INTERACTION_TARGET.id()) {
            return false;
        }
        let Some(visible) = brain.get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        else {
            return false;
        };

        let body_position = ctx.mob().position();
        let entity_type = self.entity_type;
        let interaction_range_sqr = self.interaction_range_sqr;
        let Some(closest) = visible.find_closest(|candidate| {
            candidate.position().distance_squared(body_position) <= interaction_range_sqr
                && utils::is_of_type(candidate.as_entity_event_source(), entity_type)
        }) else {
            return false;
        };

        brain.set_memory(
            memory_module_types::INTERACTION_TARGET,
            EntityMemory::new(&closest),
        );
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&closest, true),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetLookAndInteract"
    }
}
