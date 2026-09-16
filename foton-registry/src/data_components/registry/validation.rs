//! Defer nested persistent checks until their owning value has finished decoding.
use foton_utils::serial::budget;
use std::cell::Cell;

thread_local! {
    static DEFERRED: Cell<bool> = const { Cell::new(false) };
}

pub(crate) struct PersistentValidationScope(bool);

impl PersistentValidationScope {
    pub(crate) fn enter() -> Option<Self> {
        budget::is_active().then(|| Self(DEFERRED.with(|value| value.replace(true))))
    }

    pub(crate) fn is_deferred() -> bool {
        DEFERRED.with(Cell::get)
    }
}

impl Drop for PersistentValidationScope {
    fn drop(&mut self) {
        DEFERRED.with(|value| value.set(self.0));
    }
}
