//! Test-only comparisons performed by the production normalization map.

use std::{cell::Cell, cmp::Ordering};

#[derive(Eq, PartialEq)]
pub(super) struct Key(pub(super) String);

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        COMPARISONS.set(COMPARISONS.get() + 1);
        self.0.cmp(&other.0)
    }
}

thread_local! { static COMPARISONS: Cell<usize> = const { Cell::new(0) }; }

/// Measures normalization key comparisons on the calling thread.
pub fn measure(operation: impl FnOnce()) -> usize {
    COMPARISONS.set(0);
    operation();
    COMPARISONS.get()
}
