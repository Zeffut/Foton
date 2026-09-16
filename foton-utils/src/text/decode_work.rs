//! Test-only observations of actual private text conversion visits.
use std::cell::Cell;
thread_local! { static VISITS: Cell<usize> = const { Cell::new(0) }; }
pub(super) fn visit() {
    VISITS.set(VISITS.get() + 1);
}
/// Measures recursive text conversion entries on this calling thread.
pub fn measure(operation: impl FnOnce()) -> usize {
    VISITS.set(0);
    operation();
    VISITS.get()
}
