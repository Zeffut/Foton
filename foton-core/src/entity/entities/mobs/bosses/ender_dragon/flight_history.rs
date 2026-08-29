//! The dragon's rolling record of where its body has been.
//!
//! Vanilla parity: `DragonFlightHistory`. The dragon's neck, head and tail do
//! not follow its current position -- they follow where it *was*, five, twelve,
//! fourteen and sixteen ticks ago, which is what makes the body flex when it
//! turns. This is the ring buffer those lookups read.
//!
//! **Gap**: vanilla's `get(int, float)` interpolates two samples for a partial
//! tick, and `copyFrom` clones the buffer. Both exist only for the client
//! renderer and its entity render state, so neither is kept here.

/// Samples the ring buffer holds.
///
/// Vanilla parity: `DragonFlightHistory.LENGTH`.
pub const LENGTH: usize = 64;

/// Index mask for the ring buffer.
///
/// Vanilla parity: `DragonFlightHistory.MASK`.
const MASK: i32 = 63;

/// One tick of recorded flight.
///
/// Vanilla parity: `DragonFlightHistory.Sample`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Recorded vertical position.
    pub y: f64,
    /// Recorded body yaw.
    pub y_rot: f32,
}

/// The dragon's last [`LENGTH`] ticks of height and yaw.
#[derive(Debug, Clone)]
pub struct DragonFlightHistory {
    samples: [Sample; LENGTH],
    head: i32,
}

impl Default for DragonFlightHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonFlightHistory {
    /// Creates an empty history.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: [Sample { y: 0.0, y_rot: 0.0 }; LENGTH],
            head: -1,
        }
    }

    /// Records this tick's height and yaw.
    ///
    /// Vanilla parity: `DragonFlightHistory.record`. The first record fills the
    /// whole buffer, so a freshly spawned dragon does not spend its first
    /// sixty-four ticks dragging its tail up from y=0.
    pub const fn record(&mut self, y: f64, y_rot: f32) {
        let sample = Sample { y, y_rot };
        if self.head < 0 {
            self.samples = [sample; LENGTH];
        }

        self.head += 1;
        if self.head == LENGTH as i32 {
            self.head = 0;
        }

        self.samples[self.head as usize] = sample;
    }

    /// Returns the sample recorded `delay` ticks ago.
    ///
    /// Vanilla parity: `DragonFlightHistory.get(int)`.
    #[must_use]
    pub const fn get(&self, delay: i32) -> Sample {
        self.samples[((self.head - delay) & MASK) as usize]
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "the samples are stored and read back verbatim, never computed"
)]
mod tests {
    use super::*;

    #[test]
    fn the_first_record_backfills_every_sample_so_the_tail_does_not_start_at_the_void() {
        let mut history = DragonFlightHistory::new();
        history.record(80.0, 45.0);

        for delay in 0..LENGTH as i32 {
            assert_eq!(history.get(delay).y, 80.0);
        }
    }

    #[test]
    fn a_sample_is_still_readable_after_the_ring_buffer_wraps_past_it() {
        let mut history = DragonFlightHistory::new();
        history.record(0.0, 0.0);
        for tick in 1..=LENGTH as i32 + 5 {
            history.record(f64::from(tick), tick as f32);
        }

        let latest = LENGTH as f64 + 5.0;
        assert_eq!(history.get(0).y, latest);
        assert_eq!(history.get(5).y, latest - 5.0);
        assert_eq!(history.get(16).y, latest - 16.0);
    }
}
