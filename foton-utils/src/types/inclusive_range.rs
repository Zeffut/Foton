//! A closed range of comparable values.
//!
//! Vanilla parity: `net.minecraft.util.InclusiveRange`.

use std::fmt;

/// A range that holds both of its ends.
///
/// Vanilla parity: `InclusiveRange`. Vanilla throws when the minimum is above
/// the maximum; [`InclusiveRange::create`] returns `None` instead, and the
/// plain constructor swaps nothing, so a caller has to decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InclusiveRange<T> {
    pub min_inclusive: T,
    pub max_inclusive: T,
}

impl<T: PartialOrd> InclusiveRange<T> {
    /// Returns the range, or `None` when the ends are the wrong way round.
    ///
    /// Vanilla parity: `InclusiveRange.create`.
    #[must_use]
    pub fn create(min_inclusive: T, max_inclusive: T) -> Option<Self> {
        (min_inclusive <= max_inclusive).then_some(Self {
            min_inclusive,
            max_inclusive,
        })
    }

    /// Returns whether `value` lies between the ends, ends included.
    ///
    /// Vanilla parity: `InclusiveRange.isValueInRange`.
    #[must_use]
    pub fn is_value_in_range(&self, value: T) -> bool {
        value >= self.min_inclusive && value <= self.max_inclusive
    }

    /// Returns whether `sub_range` lies wholly inside this one.
    ///
    /// Vanilla parity: `InclusiveRange.contains`.
    #[must_use]
    pub fn contains(&self, sub_range: &Self) -> bool {
        sub_range.min_inclusive >= self.min_inclusive
            && sub_range.max_inclusive <= self.max_inclusive
    }
}

impl<T> InclusiveRange<T> {
    /// Returns a range covering exactly one value.
    ///
    /// Vanilla parity: the single-argument `InclusiveRange` constructor.
    #[must_use]
    pub const fn of(value: T) -> Self
    where
        T: Copy,
    {
        Self {
            min_inclusive: value,
            max_inclusive: value,
        }
    }

    /// Returns a range over the two ends without checking their order.
    #[must_use]
    pub const fn new(min_inclusive: T, max_inclusive: T) -> Self {
        Self {
            min_inclusive,
            max_inclusive,
        }
    }
}

impl<T: fmt::Display> fmt::Display for InclusiveRange<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.min_inclusive, self.max_inclusive)
    }
}
