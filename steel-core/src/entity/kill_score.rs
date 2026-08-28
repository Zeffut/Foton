//! Crediting a kill.
//!
//! Vanilla parity: `Entity.awardKillScore` and the `ServerPlayer` override,
//! which is one decision with three outcomes -- the two kill triggers and the
//! kill statistics -- and so is kept whole here rather than split across them.

use steel_registry::stat::Stat;
use steel_registry::{vanilla_custom_stats, vanilla_stat_types};

use crate::advancement::triggers;
use crate::entity::Entity;
use crate::entity::damage::DamageSource;

/// Credits `killer` with killing `victim`.
///
/// The `ServerPlayer` override wraps the base in `victim != this`, so a player
/// who kills themselves is credited with nothing -- which is why the guard is
/// here rather than in either half.
pub fn award_kill_score(killer: &dyn Entity, victim: &dyn Entity, killing_blow: &DamageSource) {
    if killer.as_player().is_some() && killer.id() == victim.id() {
        return;
    }

    // The base `Entity.awardKillScore`, which only speaks when a player died.
    if let Some(victim_player) = victim.as_player() {
        triggers::entity::entity_killed_player(victim_player, killer, killing_blow);
    }

    let Some(killer_player) = killer.as_player() else {
        return;
    };
    // TODO: drive the `killCount` scoreboard objectives here too, once Steel
    // has statistic-keyed objectives.
    if victim.as_player().is_some() {
        killer_player.award_custom_stat(&vanilla_custom_stats::PLAYER_KILLS);
    } else {
        killer_player.award_custom_stat(&vanilla_custom_stats::MOB_KILLS);
    }
    killer_player.award_stat(Stat::new(&vanilla_stat_types::KILLED, victim.entity_type()));
    triggers::entity::player_killed_entity(killer_player, victim, killing_blow);
}
