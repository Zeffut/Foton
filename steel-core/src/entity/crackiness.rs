//! How badly cracked something is, as a fraction of how intact it is.

/// The thresholds one kind of thing cracks at.
///
/// Vanilla parity: `net.minecraft.world.entity.Crackiness`. The fractions are
/// how much of the thing is left, so they read downwards: above `low` it is
/// unmarked, and below `high` it is about to give.
pub struct Crackiness {
    fraction_low: f32,
    fraction_medium: f32,
    fraction_high: f32,
}

impl Crackiness {
    /// Where an iron golem starts to show cracks.
    ///
    /// Vanilla parity: `Crackiness.GOLEM`.
    pub const GOLEM: Self = Self::new(0.75, 0.5, 0.25);

    /// Where wolf armor starts to show cracks.
    ///
    /// Vanilla parity: `Crackiness.WOLF_ARMOR`.
    pub const WOLF_ARMOR: Self = Self::new(0.95, 0.69, 0.32);

    const fn new(fraction_low: f32, fraction_medium: f32, fraction_high: f32) -> Self {
        Self {
            fraction_low,
            fraction_medium,
            fraction_high,
        }
    }

    /// Returns the crack stage for a remaining fraction.
    ///
    /// Vanilla parity: `Crackiness.byFraction`.
    #[must_use]
    pub fn by_fraction(&self, fraction: f32) -> CrackinessLevel {
        if fraction < self.fraction_high {
            CrackinessLevel::High
        } else if fraction < self.fraction_medium {
            CrackinessLevel::Medium
        } else if fraction < self.fraction_low {
            CrackinessLevel::Low
        } else {
            CrackinessLevel::None
        }
    }
}

/// How cracked something looks.
///
/// Vanilla parity: `Crackiness.Level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrackinessLevel {
    /// Unmarked.
    None,
    /// The first hairline cracks.
    Low,
    /// Visibly split.
    Medium,
    /// About to break.
    High,
}
