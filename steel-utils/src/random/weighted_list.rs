//! A weighted list of values.
//!
//! Vanilla parity: `net.minecraft.util.random.WeightedList`.

use std::io::{Result, Write};

use crate::codec::VarInt;
use crate::random::Random;
use crate::serial::WriteTo;

/// One entry and the weight it is drawn with.
///
/// Vanilla parity: `WeightedList.Weighted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Weighted<T> {
    /// The value this entry hands back when it is drawn.
    pub value: T,
    /// Its share of the draw, against the sum of every entry's.
    pub weight: i32,
}

/// A list of values drawn in proportion to their weights.
///
/// Vanilla parity: `WeightedList`. Vanilla precomputes a running total and
/// binary-searches it; the lists Steel builds are a handful of entries long,
/// so this walks them instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedList<T> {
    entries: Vec<Weighted<T>>,
    total_weight: i64,
}

impl<T> Default for WeightedList<T> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<T> WeightedList<T> {
    /// Returns the list with nothing in it.
    ///
    /// Vanilla parity: `WeightedList.of()`.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            entries: Vec::new(),
            total_weight: 0,
        }
    }

    /// Returns a list holding one value at weight one.
    ///
    /// Vanilla parity: the single-value `WeightedList.of`.
    #[must_use]
    pub fn single(value: T) -> Self {
        Self::new(vec![Weighted { value, weight: 1 }])
    }

    /// Returns a list over the given entries.
    ///
    /// Vanilla parity: the list-taking `WeightedList.of`. Vanilla's codec
    /// refuses a negative weight; the callers here are generated data and
    /// saved NBT, so a negative weight is clamped to zero instead. An entry
    /// that can never be drawn is closer to vanilla than a spawner that
    /// refuses to load.
    #[must_use]
    pub fn new(entries: Vec<Weighted<T>>) -> Self {
        let entries: Vec<Weighted<T>> = entries
            .into_iter()
            .map(|entry| Weighted {
                value: entry.value,
                weight: entry.weight.max(0),
            })
            .collect();
        let total_weight = entries.iter().map(|entry| i64::from(entry.weight)).sum();
        Self {
            entries,
            total_weight,
        }
    }

    /// Returns whether nothing can be drawn.
    ///
    /// Vanilla parity: `WeightedList.isEmpty`, which asks only about the entry
    /// list -- a list of zero-weight entries is not empty to vanilla either.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns how many entries the list holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns the entries in declaration order.
    #[must_use]
    pub fn entries(&self) -> &[Weighted<T>] {
        &self.entries
    }

    /// Returns the sum of every weight.
    #[must_use]
    pub const fn total_weight(&self) -> i64 {
        self.total_weight
    }

    /// Returns the value a roll below the total weight lands on.
    ///
    /// Split out of the draw so the seeded and unseeded callers share the
    /// selection itself, and so a test can name a roll.
    #[must_use]
    pub fn get_by_roll(&self, mut roll: i64) -> Option<&T> {
        if self.total_weight <= 0 {
            return None;
        }
        for entry in &self.entries {
            roll -= i64::from(entry.weight);
            if roll < 0 {
                return Some(&entry.value);
            }
        }
        None
    }

    /// Draws one value with Steel's unseeded runtime RNG.
    ///
    /// Vanilla parity: `WeightedList.getRandom`.
    #[must_use]
    pub fn get_random(&self) -> Option<&T> {
        if self.total_weight <= 0 {
            return None;
        }
        self.get_by_roll(rand::random_range(0..self.total_weight))
    }

    /// Draws one value with a seeded source.
    #[must_use]
    pub fn get_random_with<R: Random + ?Sized>(&self, random: &mut R) -> Option<&T> {
        if self.total_weight <= 0 {
            return None;
        }
        let bound = i32::try_from(self.total_weight).unwrap_or(i32::MAX);
        self.get_by_roll(i64::from(random.next_i32_bounded(bound)))
    }
}

impl<T: Clone> WeightedList<T> {
    /// Draws one value and clones it.
    #[must_use]
    pub fn get_random_cloned(&self) -> Option<T> {
        self.get_random().cloned()
    }
}

impl<T: WriteTo> WriteTo for WeightedList<T> {
    /// Vanilla parity: `WeightedList.streamCodec`, a length-prefixed list of
    /// `Weighted.streamCodec` -- the value, then its weight as a `VarInt`.
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.entries.len() as i32).write(writer)?;
        for entry in &self.entries {
            entry.value.write(writer)?;
            VarInt(entry.weight).write(writer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The roll boundaries are the whole of the selection, so an off-by-one
    /// here would silently make one entry unreachable.
    #[test]
    fn every_roll_lands_inside_the_entry_that_owns_it() {
        let list = WeightedList::new(vec![
            Weighted {
                value: "key",
                weight: 3,
            },
            Weighted {
                value: "consumables",
                weight: 7,
            },
        ]);
        assert_eq!(list.total_weight(), 10);
        assert_eq!(list.get_by_roll(0), Some(&"key"));
        assert_eq!(list.get_by_roll(2), Some(&"key"));
        assert_eq!(list.get_by_roll(3), Some(&"consumables"));
        assert_eq!(list.get_by_roll(9), Some(&"consumables"));
        assert_eq!(list.get_by_roll(10), None);
    }

    /// A spawner whose potentials all carry weight zero must draw nothing
    /// rather than always hand back the first entry.
    #[test]
    fn a_list_of_zero_weights_draws_nothing() {
        let list = WeightedList::new(vec![Weighted {
            value: 1,
            weight: 0,
        }]);
        assert!(!list.is_empty(), "vanilla isEmpty only counts entries");
        assert_eq!(list.get_random(), None);
    }
}
