//! Applying one mob effect to everybody in an area.

use std::sync::Arc;

use foton_utils::types::GameType;
use glam::DVec3;

use super::World;
use crate::entity::{Entity, LivingEntity as _, MobEffectInstance};
use crate::player::Player;

impl World {
    /// Gives `effect` to every survival player within `radius`, and returns how many got it.
    ///
    /// Vanilla parity: `MobEffectUtil.addEffectToPlayersAround`. The warden's darkness is
    /// reapplied every six seconds, so the filter matters: a player who already has it at
    /// the same strength and more than `display_effect_limit` ticks left is skipped, which
    /// is what stops the effect being refreshed to full on every pulse.
    pub fn add_effect_to_players_around(
        &self,
        source: Option<&dyn Entity>,
        position: DVec3,
        radius: f64,
        effect: &MobEffectInstance,
        display_effect_limit: i32,
    ) -> usize {
        let mut affected = Vec::new();
        self.players.iter_players(|_, player| {
            if should_receive(
                player,
                source,
                position,
                radius,
                effect,
                display_effect_limit,
            ) {
                affected.push(Arc::clone(player));
            }
            true
        });

        let count = affected.len();
        for player in affected {
            player.add_mob_effect(effect.clone());
        }
        count
    }
}

fn should_receive(
    player: &Player,
    source: Option<&dyn Entity>,
    position: DVec3,
    radius: f64,
    effect: &MobEffectInstance,
    display_effect_limit: i32,
) -> bool {
    if player.game_mode() != GameType::Survival {
        return false;
    }
    if source.is_some_and(|source| source.is_allied_to(player)) {
        return false;
    }
    if position.distance(player.position()) >= radius {
        return false;
    }
    match player.mob_effect(effect.effect()) {
        None => true,
        Some(active) => {
            active.amplifier() < effect.amplifier() || active.duration() < display_effect_limit
        }
    }
}
