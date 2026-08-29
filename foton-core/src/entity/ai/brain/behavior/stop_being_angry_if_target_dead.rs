//! Vanilla `StopBeingAngryIfTargetDead`.

use foton_registry::vanilla_entities;
use foton_registry::vanilla_game_rules::FORGIVE_DEAD_PLAYERS;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};

/// Forgets a grudge once the entity it was aimed at is dying.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.StopBeingAngryIfTargetDead`.
pub struct StopBeingAngryIfTargetDead;

impl Trigger for StopBeingAngryIfTargetDead {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::ANGRY_AT.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if !brain.has_memory_value(memory_module_types::ANGRY_AT.id()) {
            return false;
        }

        let Some(target) = utils::living_entity_from_uuid_memory(
            ctx.world(),
            brain,
            memory_module_types::ANGRY_AT,
        ) else {
            // Vanilla's `Optional.ofNullable(level.getEntity(uuid))` simply
            // finds nothing and the behavior still reports success.
            return true;
        };
        let Some(living) = target.as_living_entity() else {
            return true;
        };
        let forgiven = !utils::is_of_type(target.as_ref(), &vanilla_entities::PLAYER)
            || ctx.world().get_game_rule(&FORGIVE_DEAD_PLAYERS);
        if living.is_dead_or_dying() && forgiven {
            brain.erase_memory(memory_module_types::ANGRY_AT.id());
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "StopBeingAngryIfTargetDead"
    }
}
