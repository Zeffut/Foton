//! Vanilla `StartCelebratingIfTargetDead`.

use steel_registry::vanilla_entities;
use steel_registry::vanilla_game_rules::FORGIVE_DEAD_PLAYERS;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::{LivingEntity, SharedEntity};

/// Whether the kill is worth dancing over.
type DancePredicate = Box<dyn Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send>;

/// Marks the spot a dead attack target fell on so the mob can gloat there.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.StartCelebratingIfTargetDead`.
pub struct StartCelebratingIfTargetDead {
    celebrate_duration: i64,
    dance_predicate: DancePredicate,
}

impl StartCelebratingIfTargetDead {
    /// Vanilla parity: `StartCelebratingIfTargetDead.create`.
    #[must_use]
    pub fn new(
        celebrate_duration: i64,
        dance_predicate: impl Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send + 'static,
    ) -> Self {
        Self {
            celebrate_duration,
            dance_predicate: Box::new(dance_predicate),
        }
    }
}

impl Trigger for StartCelebratingIfTargetDead {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::ANGRY_AT.id(),
            memory_module_types::CELEBRATE_LOCATION.id(),
            memory_module_types::DANCING.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::CELEBRATE_LOCATION.id()) {
            return false;
        }
        let Some(target) = brain
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return false;
        };
        if !target
            .as_living_entity()
            .is_some_and(LivingEntity::is_dead_or_dying)
        {
            return false;
        }

        if (self.dance_predicate)(ctx, &target) {
            brain.set_memory_with_expiry(
                memory_module_types::DANCING,
                true,
                self.celebrate_duration,
            );
        }
        brain.set_memory_with_expiry(
            memory_module_types::CELEBRATE_LOCATION,
            target.block_position(),
            self.celebrate_duration,
        );

        let forgiven = !utils::is_of_type(target.as_ref(), &vanilla_entities::PLAYER)
            || ctx.world().get_game_rule(&FORGIVE_DEAD_PLAYERS);
        if forgiven {
            brain.erase_memory(memory_module_types::ATTACK_TARGET.id());
            brain.erase_memory(memory_module_types::ANGRY_AT.id());
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "StartCelebratingIfTargetDead"
    }
}
