//! Ticks per second over the last one, five and fifteen minutes.
//!
//! Not vanilla: vanilla reports tick *time*, which says nothing once the
//! server falls behind and every tick starts late. This is what Paper's
//! `Server.getTPS()` measures -- wall-clock time between tick starts, sampled
//! once a second -- because plugins read it as a load signal and compare it to
//! the numbers a Paper server would give.

use std::time::Instant;

/// Ticks between samples: one second at the nominal rate.
const SAMPLE_INTERVAL: u64 = 20;

const NANOS_PER_SECOND: f64 = 1_000_000_000.0;

/// A time-weighted average of the last `N` one-second samples.
///
/// Weighted by each sample's real duration, so a ten-second stall counts ten
/// times as much as a normal second rather than once.
struct RollingAverage<const N: usize> {
    samples: [f64; N],
    durations: [f64; N],
    index: usize,
    /// Sum of `durations`, kept alongside so reading is O(1).
    total_duration: f64,
    /// Sum of `samples[i] * durations[i]`.
    weighted_total: f64,
}

impl<const N: usize> RollingAverage<N> {
    /// Starts full of perfect seconds, as Paper does, so the first minute does
    /// not read as a server that was idle before it booted.
    const fn new(nominal: f64) -> Self {
        Self {
            samples: [nominal; N],
            durations: [NANOS_PER_SECOND; N],
            index: 0,
            total_duration: NANOS_PER_SECOND * N as f64,
            weighted_total: nominal * NANOS_PER_SECOND * N as f64,
        }
    }

    fn add(&mut self, tps: f64, duration: f64) {
        self.total_duration -= self.durations[self.index];
        self.weighted_total -= self.samples[self.index] * self.durations[self.index];
        self.samples[self.index] = tps;
        self.durations[self.index] = duration;
        self.total_duration += duration;
        self.weighted_total += tps * duration;
        self.index = (self.index + 1) % N;
    }

    fn average(&self) -> f64 {
        if self.total_duration <= 0.0 {
            return 0.0;
        }
        self.weighted_total / self.total_duration
    }
}

/// The three averages `getTPS()` returns, fed once per tick.
pub struct TpsAverages {
    one_minute: RollingAverage<60>,
    five_minutes: RollingAverage<300>,
    fifteen_minutes: RollingAverage<900>,
    /// The tick the current sample started at, and when, or `None` before
    /// the first tick.
    section_start: Option<(u64, Instant)>,
}

impl TpsAverages {
    /// Averages that read as a steady `nominal` until real samples arrive.
    #[must_use]
    pub const fn new(nominal: f64) -> Self {
        Self {
            one_minute: RollingAverage::new(nominal),
            five_minutes: RollingAverage::new(nominal),
            fifteen_minutes: RollingAverage::new(nominal),
            section_start: None,
        }
    }

    /// Records that tick number `tick` started at `now`.
    pub fn tick_started(&mut self, tick: u64, now: Instant) {
        let Some((start_tick, start)) = self.section_start else {
            self.section_start = Some((tick, now));
            return;
        };
        if !tick.is_multiple_of(SAMPLE_INTERVAL) || tick <= start_tick {
            return;
        }
        let elapsed = now.duration_since(start).as_nanos() as f64;
        self.section_start = Some((tick, now));
        if elapsed <= 0.0 {
            return;
        }
        let tps = NANOS_PER_SECOND * (tick - start_tick) as f64 / elapsed;
        self.one_minute.add(tps, elapsed);
        self.five_minutes.add(tps, elapsed);
        self.fifteen_minutes.add(tps, elapsed);
    }

    /// Ticks per second over the last 1, 5 and 15 minutes, in that order.
    #[must_use]
    pub fn averages(&self) -> [f64; 3] {
        [
            self.one_minute.average(),
            self.five_minutes.average(),
            self.fifteen_minutes.average(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_stall_lowers_the_short_average_more_than_the_long_one() {
        let mut tps = TpsAverages::new(20.0);
        let mut now = Instant::now();
        tps.tick_started(0, now);
        // Forty ticks at half speed: two samples of 10 TPS, two seconds each.
        for tick in 1..=40 {
            now += Duration::from_millis(100);
            tps.tick_started(tick, now);
        }
        let [one, five, fifteen] = tps.averages();
        assert!(one < five && five < fifteen, "{one} {five} {fifteen}");
        // 58 seconds at 20 and 4 seconds at 10, weighted by duration.
        let expected = (58.0 * 20.0 + 4.0 * 10.0) / 62.0;
        assert!((one - expected).abs() < 1e-9, "{one} != {expected}");
    }
}
