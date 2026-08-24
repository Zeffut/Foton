//! Vanilla `PlayerSensor`.

use std::sync::Arc;

use super::{Sensor, follow_range, is_entity_attackable, is_entity_targetable};
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::{Entity as _, SharedEntity};
use crate::player::Player;

/// Remembers the nearby players, and which of them are visible or attackable.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.PlayerSensor`.
pub struct PlayerSensor;

impl Sensor for PlayerSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_PLAYERS.id(),
            memory_module_types::NEAREST_VISIBLE_PLAYER.id(),
            memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER.id(),
            memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYERS.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let body_position = body.position();
        let range = follow_range(body);
        let range_sqr = range * range;

        let mut players: Vec<Arc<Player>> = Vec::new();
        ctx.world().players.iter_players(|_, player| {
            if !player.is_spectator()
                && player.position().distance_squared(body_position) <= range_sqr
            {
                players.push(Arc::clone(player));
            }
            true
        });
        players.sort_by(|left, right| {
            let left = left.position().distance_squared(body_position);
            let right = right.position().distance_squared(body_position);
            left.total_cmp(&right)
        });

        let brain = ctx.brain();
        brain.set_memory(
            memory_module_types::NEAREST_PLAYERS,
            players
                .iter()
                .map(|player| EntityMemory::new(&(Arc::clone(player) as SharedEntity)))
                .collect::<Vec<_>>(),
        );

        let visible: Vec<&Arc<Player>> = players
            .iter()
            .filter(|player| is_entity_targetable(ctx.world(), body, player.as_ref()))
            .collect();
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_PLAYER,
            visible
                .first()
                .map(|player| EntityMemory::new(&(Arc::clone(player) as SharedEntity))),
        );

        let attackable: Vec<&Arc<Player>> = visible
            .into_iter()
            .filter(|player| is_entity_attackable(ctx.world(), body, player.as_ref()))
            .collect();
        brain.set_memory(
            memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYERS,
            attackable
                .iter()
                .map(|player| EntityMemory::new(&(Arc::clone(player) as SharedEntity)))
                .collect::<Vec<_>>(),
        );
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_ATTACKABLE_PLAYER,
            attackable
                .first()
                .map(|player| EntityMemory::new(&(Arc::clone(player) as SharedEntity))),
        );
    }
}
