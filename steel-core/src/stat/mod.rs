//! What one player has counted.
//!
//! Vanilla parity: `StatsCounter` and the dirty-set half `ServerStatsCounter`
//! adds. The registry of statistics lives in `steel_registry::stat`; this is
//! the part that belongs to a player.

use rustc_hash::{FxHashMap, FxHashSet};
use steel_registry::stat::Stat;

/// Every statistic this player has a value for.
pub struct StatsCounter {
    values: FxHashMap<Stat, i32>,
    /// The statistics whose value the client has not been told yet.
    ///
    /// Vanilla parity: `ServerStatsCounter.dirty`. Only these travel, which is
    /// what keeps the statistics screen from re-sending hundreds of unchanged
    /// counters every time one of them moves.
    dirty: FxHashSet<Stat>,
}

impl StatsCounter {
    /// A player who has counted nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: FxHashMap::default(),
            dirty: FxHashSet::default(),
        }
    }

    /// What this statistic stands at.
    ///
    /// Vanilla parity: `StatsCounter.getValue`, whose map has a default return
    /// value of zero -- an unset statistic is zero, not absent.
    #[must_use]
    pub fn value(&self, stat: Stat) -> i32 {
        self.values.get(&stat).copied().unwrap_or_default()
    }

    /// Sets a statistic outright.
    ///
    /// Vanilla parity: `ServerStatsCounter.setValue`, which is also what
    /// `Player.resetStat` goes through.
    pub fn set(&mut self, stat: Stat, value: i32) {
        self.values.insert(stat, value);
        self.dirty.insert(stat);
    }

    /// Adds to a statistic.
    ///
    /// Vanilla parity: `StatsCounter.increment`, which saturates at `i32::MAX`
    /// rather than wrapping: it adds in a `long` and clamps.
    pub fn increment(&mut self, stat: Stat, amount: i32) {
        let raised = i64::from(self.value(stat)).saturating_add(i64::from(amount));
        let clamped = i32::try_from(raised).unwrap_or(i32::MAX);
        self.set(stat, clamped);
    }

    /// Takes the statistics the client still has to be told about.
    ///
    /// Vanilla parity: `ServerStatsCounter.getDirty`, which clears as it reads.
    /// The result is sorted so the same set of changes always encodes to the
    /// same packet.
    pub fn take_dirty(&mut self) -> Vec<(Stat, i32)> {
        let mut taken: Vec<(Stat, i32)> = self
            .dirty
            .drain()
            .map(|stat| (stat, self.values.get(&stat).copied().unwrap_or_default()))
            .collect();
        taken.sort_unstable();
        taken
    }

    /// Marks every statistic as needing to be sent.
    ///
    /// Vanilla parity: `ServerStatsCounter.markAllDirty`, called when a player
    /// joins so the screen starts out complete.
    pub fn mark_all_dirty(&mut self) {
        self.dirty.extend(self.values.keys().copied());
    }

    /// Every statistic and its value, for saving.
    pub fn entries(&self) -> impl Iterator<Item = (Stat, i32)> + '_ {
        self.values.iter().map(|(&stat, &value)| (stat, value))
    }

    /// Restores saved statistics, replacing whatever was counted so far.
    ///
    /// Vanilla parity: `ServerStatsCounter.parse`, which fills a counter that
    /// was just constructed.
    pub fn load(&mut self, entries: impl IntoIterator<Item = (Stat, i32)>) {
        self.values.clear();
        self.dirty.clear();
        self.values.extend(entries);
    }
}

impl Default for StatsCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
