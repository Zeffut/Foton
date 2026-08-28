//! Triggers that fire from where a player is and what the world did to them.

use super::fire;
use crate::player::Player;

/// Fired once per player tick.
///
/// Vanilla parity: `CriteriaTriggers.TICK`, invoked from `ServerPlayer.tick`.
/// `PlayerTrigger` carries nothing but its `player` predicate, so that
/// predicate is the whole test -- which is how `nether/all_effects` and the
/// other "be in this state right now" advancements are written.
pub fn tick(player: &Player) {
    fire(player, "minecraft:tick", |_| true);
}

/// Fired once every twenty player ticks.
///
/// Vanilla parity: `CriteriaTriggers.LOCATION`, invoked from
/// `ServerPlayer.doTick` under `this.tickCount % 20 == 0`. It shares
/// `PlayerTrigger` with [`tick`]; the twenty-tick spacing is the only
/// difference, and it is what keeps the biome and structure checks off the
/// per-tick path.
pub fn location(player: &Player) {
    fire(player, "minecraft:location", |_| true);
}
