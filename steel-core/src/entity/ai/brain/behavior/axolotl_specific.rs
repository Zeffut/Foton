//! The two behaviors that make an axolotl play dead.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.axolotl.PlayDead` and
//! `ValidatePlayDead`. They are a pair: the first one holds the axolotl still
//! and heals it, and the second one, running in the core activity, counts the
//! clock down and hands control back when it runs out.

use steel_registry::vanilla_mob_effects;

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, Trigger};
use crate::entity::MobEffectInstance;
use crate::entity::ai::brain::memory::memory_module_types;

/// Vanilla parity: `Axolotl.TOTAL_PLAYDEAD_TIME`, which is both the memory's
/// starting value and the behavior's duration.
pub const TOTAL_PLAYDEAD_TIME: i32 = 200;

/// Holds an axolotl still and heals it while it plays dead.
///
/// Vanilla parity: `net.minecraft.world.entity.animal.axolotl.PlayDead`.
pub struct PlayDead;

impl TimedBehavior for PlayDead {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        const CONDITIONS: &[(MemoryModuleId, MemoryStatus)] = &[
            (
                memory_module_types::PLAY_DEAD_TICKS.id(),
                MemoryStatus::ValuePresent,
            ),
            (
                memory_module_types::HURT_BY_ENTITY.id(),
                MemoryStatus::ValuePresent,
            ),
        ];
        CONDITIONS
    }

    fn duration(&self) -> (i32, i32) {
        (TOTAL_PLAYDEAD_TIME, TOTAL_PLAYDEAD_TIME)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob().is_in_water()
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob().is_in_water()
            && ctx
                .brain()
                .has_memory_value(memory_module_types::PLAY_DEAD_TICKS.id())
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
        ctx.mob().add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::REGENERATION,
            TOTAL_PLAYDEAD_TIME,
            0,
        ));
    }

    fn debug_name(&self) -> &'static str {
        "PlayDead"
    }
}

/// Counts the play-dead clock down and ends the act when it hits zero.
///
/// Vanilla parity: `net.minecraft.world.entity.animal.axolotl.ValidatePlayDead`.
/// It lives in the core activity, so it keeps running while `PLAY_DEAD` holds
/// every other activity out.
pub struct ValidatePlayDead;

impl Trigger for ValidatePlayDead {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::PLAY_DEAD_TICKS.id(),
            memory_module_types::HURT_BY_ENTITY.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(ticks) = brain.get_memory(memory_module_types::PLAY_DEAD_TICKS) else {
            return false;
        };

        if ticks <= 0 {
            brain.erase_memory(memory_module_types::PLAY_DEAD_TICKS.id());
            brain.erase_memory(memory_module_types::HURT_BY_ENTITY.id());
            brain.use_default_activity();
        } else {
            brain.set_memory(memory_module_types::PLAY_DEAD_TICKS, ticks - 1);
        }

        true
    }

    fn debug_name(&self) -> &'static str {
        "ValidatePlayDead"
    }
}
