//! Development-only observations of registered codec preliminary work.
use std::cell::Cell;

/// Work performed before streaming component values.
#[derive(Clone, Copy, Debug, Default)]
pub struct Work {
    /// UTF-16 units examined by network string validators.
    pub utf16: usize,
    /// Persistent profiles copied from borrowed NBT.
    pub profile_copies: usize,
}
thread_local! {
    static WORK: Cell<Work> = const { Cell::new(Work { utf16: 0, profile_copies: 0 }) };
}
pub(crate) fn utf16() {
    WORK.update(|mut w| {
        w.utf16 += 1;
        w
    });
}
pub(crate) fn profile_copy() {
    WORK.update(|mut w| {
        w.profile_copies += 1;
        w
    });
}
/// Observe the current thread while executing an actual registered codec.
pub fn measure(operation: impl FnOnce()) -> Work {
    WORK.set(Work::default());
    operation();
    WORK.get()
}
