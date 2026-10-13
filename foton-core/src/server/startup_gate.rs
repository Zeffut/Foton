//! Keeps players out until a freshly started server keeps up with its ticks.
//!
//! The first seconds after boot are the heaviest a server has: worlds finish loading, chunks
//! around the spawn generate, plugins warm up. A player let in then joins a server that is
//! seconds behind and sees it lag. The listener opens only once the recent ticks fit their
//! budget -- or after [`MAX_WAIT`], so a machine that is simply slow still takes players.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use foton_utils::locks::SyncMutex;
use tokio::sync::Notify;

/// Ticks the stability check looks back over: one second at 20 TPS.
const WINDOW_TICKS: usize = 20;

/// No tick of the window may take longer than this many tick budgets.
const MAX_SPIKE_BUDGETS: u64 = 4;

/// The longest the gate waits for stability before opening anyway.
pub(crate) const MAX_WAIT: Duration = Duration::from_secs(20);

/// Whether the server has settled enough after boot to take connections.
pub struct StartupGate {
    open: AtomicBool,
    opened: Notify,
    progress: SyncMutex<Progress>,
}

struct Progress {
    first_tick: Option<Instant>,
    recent: [u64; WINDOW_TICKS],
    recorded: usize,
}

/// Why the gate opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOpening {
    /// The last second of ticks fit their budget.
    Stable,
    /// [`MAX_WAIT`] ran out first.
    TimedOut,
}

impl StartupGate {
    /// A closed gate.
    #[must_use]
    pub fn new() -> Self {
        Self {
            open: AtomicBool::new(false),
            opened: Notify::new(),
            progress: SyncMutex::new(Progress {
                first_tick: None,
                recent: [0; WINDOW_TICKS],
                recorded: 0,
            }),
        }
    }

    /// Whether players may connect.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// Waits until players may connect.
    pub async fn wait_open(&self) {
        loop {
            let notified = self.opened.notified();
            if self.is_open() {
                return;
            }
            notified.await;
        }
    }

    /// Records how long a tick took against its budget, and opens the gate once the server has
    /// settled. Returns how it opened, the one time it does.
    pub fn record_tick(
        &self,
        tick_nanos: u64,
        budget_nanos: u64,
        now: Instant,
    ) -> Option<GateOpening> {
        if self.is_open() {
            return None;
        }
        let opening = {
            let mut progress = self.progress.lock();
            let first_tick = *progress.first_tick.get_or_insert(now);
            let slot = progress.recorded % WINDOW_TICKS;
            progress.recent[slot] = tick_nanos;
            progress.recorded += 1;
            if progress.is_stable(budget_nanos) {
                Some(GateOpening::Stable)
            } else if now.duration_since(first_tick) >= MAX_WAIT {
                Some(GateOpening::TimedOut)
            } else {
                None
            }
        }?;
        self.open.store(true, Ordering::Release);
        self.opened.notify_waiters();
        Some(opening)
    }
}

impl Default for StartupGate {
    fn default() -> Self {
        Self::new()
    }
}

impl Progress {
    /// A full window whose ticks average within budget, with no spike past a few budgets.
    fn is_stable(&self, budget_nanos: u64) -> bool {
        if self.recorded < WINDOW_TICKS {
            return false;
        }
        let total: u64 = self.recent.iter().sum();
        let worst = self.recent.iter().copied().max().unwrap_or(0);
        total <= budget_nanos * WINDOW_TICKS as u64 && worst <= budget_nanos * MAX_SPIKE_BUDGETS
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::time::timeout;

    use super::*;

    const BUDGET: u64 = 50_000_000;

    fn ticks(gate: &StartupGate, start: Instant, from: u64, durations: &[u64]) -> Vec<GateOpening> {
        durations
            .iter()
            .enumerate()
            .filter_map(|(index, &nanos)| {
                let at = start + Duration::from_millis((from + index as u64) * 50);
                gate.record_tick(nanos, BUDGET, at)
            })
            .collect()
    }

    #[test]
    fn a_second_of_ticks_in_budget_opens_the_gate_once() {
        let gate = StartupGate::new();
        let start = Instant::now();
        assert!(ticks(&gate, start, 0, &[BUDGET / 2; WINDOW_TICKS - 1]).is_empty());
        assert!(!gate.is_open());
        assert_eq!(
            ticks(&gate, start, 19, &[BUDGET / 2; 5]),
            [GateOpening::Stable]
        );
        assert!(gate.is_open());
    }

    #[test]
    fn a_spike_in_the_window_keeps_it_closed_until_it_scrolls_out() {
        let gate = StartupGate::new();
        let start = Instant::now();
        let mut durations = vec![BUDGET / 10; WINDOW_TICKS];
        durations[3] = BUDGET * 5;
        assert!(ticks(&gate, start, 0, &durations).is_empty());
        // Four more short ticks still leave the spike in the window.
        assert!(ticks(&gate, start, 20, &[BUDGET / 10; 3]).is_empty());
        assert_eq!(
            ticks(&gate, start, 23, &[BUDGET / 10]),
            [GateOpening::Stable]
        );
    }

    #[test]
    fn a_server_that_never_settles_opens_after_the_maximum_wait() {
        let gate = StartupGate::new();
        let start = Instant::now();
        let slow = [BUDGET * 2; 500];
        let openings = ticks(&gate, start, 0, &slow);
        assert_eq!(openings, [GateOpening::TimedOut]);
        assert!(gate.is_open());
    }

    #[tokio::test]
    async fn waiters_wake_when_the_gate_opens() {
        let gate = Arc::new(StartupGate::new());
        let waiter = tokio::spawn({
            let gate = gate.clone();
            async move { gate.wait_open().await }
        });
        ticks(&gate, Instant::now(), 0, &[1; WINDOW_TICKS]);
        timeout(Duration::from_secs(5), waiter)
            .await
            .expect("waiter woke")
            .expect("waiter ran");
    }
}
