//! The player half of the statistics counter.
//!
//! Vanilla parity: `ServerPlayer.awardStat` and `ServerStatsCounter.sendStats`.
//! The counter itself is in [`crate::stat::StatsCounter`]; this is what a live
//! player does with it.

use steel_protocol::packets::game::CAwardStats;
use steel_registry::stat::{CustomStatRef, Stat, StatTypeRef};
use steel_registry::{RegistryEntry, vanilla_custom_stats};

use super::Player;
use crate::entity::{Entity, LivingEntity as _};

impl Player {
    /// Adds one to a statistic.
    ///
    /// Vanilla parity: `Player.awardStat(Stat)`.
    ///
    /// TODO: also drive the scoreboard objectives keyed on this statistic, the
    /// way `ServerPlayer.awardStat` does. Steel has no statistic-keyed
    /// objectives yet, so there is nothing to drive.
    pub fn award_stat(&self, stat: Stat) {
        self.award_stat_amount(stat, 1);
    }

    /// Adds `amount` to a statistic.
    ///
    /// Vanilla parity: `ServerPlayer.awardStat(Stat, int)`.
    pub fn award_stat_amount(&self, stat: Stat, amount: i32) {
        self.stats.lock().increment(stat, amount);
    }

    /// Adds one to a `minecraft:custom` statistic.
    ///
    /// Vanilla parity: `Player.awardStat(Identifier)`.
    pub fn award_custom_stat(&self, stat: CustomStatRef) {
        self.award_stat(Stat::custom(stat));
    }

    /// Adds `amount` to a `minecraft:custom` statistic.
    pub fn award_custom_stat_amount(&self, stat: CustomStatRef, amount: i32) {
        self.award_stat_amount(Stat::custom(stat), amount);
    }

    /// Adds one to a statistic keyed by a registry entry.
    ///
    /// Vanilla parity: the `Stats.X.get(value)` half of an award, for the eight
    /// stat types whose values are blocks, items or entity types.
    pub fn award_stat_for(&self, stat_type: StatTypeRef, value: &impl RegistryEntry) {
        self.award_stat(Stat::new(stat_type, value));
    }

    /// Adds `amount` to a statistic keyed by a registry entry.
    pub fn award_stat_amount_for(
        &self,
        stat_type: StatTypeRef,
        value: &impl RegistryEntry,
        amount: i32,
    ) {
        self.award_stat_amount(Stat::new(stat_type, value), amount);
    }

    /// Sets a statistic back to zero.
    ///
    /// Vanilla parity: `ServerPlayer.resetStat`.
    pub fn reset_stat(&self, stat: Stat) {
        self.stats.lock().set(stat, 0);
    }

    /// What a statistic stands at.
    #[must_use]
    pub fn stat_value(&self, stat: Stat) -> i32 {
        self.stats.lock().value(stat)
    }

    /// Sends the statistics whose value the client has not been told.
    ///
    /// Vanilla parity: `ServerStatsCounter.sendStats`, which the client asks
    /// for by opening the statistics screen.
    pub fn send_dirty_statistics(&self) {
        let stats = self.stats.lock().take_dirty();
        if stats.is_empty() {
            return;
        }
        self.send_packet(CAwardStats { stats });
    }

    /// Marks every statistic as needing to be sent.
    ///
    /// Vanilla parity: the `player.getStats().markAllDirty()` of
    /// `PlayerList.placeNewPlayer`, which is what makes the screen complete
    /// rather than only carrying what moved since login.
    pub fn mark_all_statistics_dirty(&self) {
        self.stats.lock().mark_all_dirty();
    }

    /// Every statistic and its value, for saving.
    #[must_use]
    pub fn saved_statistics(&self) -> Vec<(Stat, i32)> {
        self.stats.lock().entries().collect()
    }

    /// Restores saved statistics, replacing whatever was counted so far.
    pub fn load_statistics(&self, entries: impl IntoIterator<Item = (Stat, i32)>) {
        self.stats.lock().load(entries);
    }

    /// The five time counters vanilla raises every tick.
    ///
    /// Vanilla parity: the block of `ServerPlayer.doTick` that awards
    /// `PLAY_TIME`, `TOTAL_WORLD_TIME`, `TIME_SINCE_DEATH`, `CROUCH_TIME` and
    /// `TIME_SINCE_REST`. `TIME_SINCE_REST` is not decoration: vanilla's
    /// phantom spawner reads it.
    pub(super) fn tick_time_statistics(&self) {
        self.award_custom_stat(&vanilla_custom_stats::PLAY_TIME);
        self.award_custom_stat(&vanilla_custom_stats::TOTAL_WORLD_TIME);
        if Entity::is_alive(self) {
            self.award_custom_stat(&vanilla_custom_stats::TIME_SINCE_DEATH);
        }
        if self.is_discrete() {
            self.award_custom_stat(&vanilla_custom_stats::SNEAK_TIME);
        }
        if !self.is_sleeping() {
            self.award_custom_stat(&vanilla_custom_stats::TIME_SINCE_REST);
        }
    }
}
