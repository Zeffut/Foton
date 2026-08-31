//! Checks on the event bus.

use std::sync::Arc;

use foton_utils::Identifier;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::locks::SyncMutex;

use super::{Event, EventBus, EventPriority};

/// An event that records who ran, in order.
struct Trace {
    ran: Vec<&'static str>,
    cancelled: bool,
}

// SAFETY: A test-only key, distinct from every other in the process.
unsafe impl DowncastType for Trace {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:test_event/trace");
}

impl Event for Trace {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// A second type, to prove the bus keeps them apart.
struct Other;

// SAFETY: A test-only key, distinct from every other in the process.
unsafe impl DowncastType for Other {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:test_event/other");
}

impl Event for Other {}

fn owner(name: &'static str) -> Identifier {
    Identifier::new_static("test", name)
}

fn record(bus: &EventBus, who: &'static str, priority: EventPriority) {
    bus.listen::<Trace, _>(owner(who), priority, false, move |event| {
        event.ran.push(who);
    });
}

/// Lowest runs first and Highest last, which is the half of Bukkit's ordering
/// that reads backwards and is therefore the half worth pinning.
#[test]
fn listeners_run_from_lowest_priority_to_highest() {
    let bus = EventBus::new();
    record(&bus, "highest", EventPriority::Highest);
    record(&bus, "lowest", EventPriority::Lowest);
    record(&bus, "normal", EventPriority::Normal);

    let mut event = Trace {
        ran: Vec::new(),
        cancelled: false,
    };
    bus.fire(&mut event);

    assert_eq!(event.ran, ["lowest", "normal", "highest"]);
}

/// Two listeners at the same priority keep the order they registered in, which
/// is the only tie-break a listener author can reason about.
#[test]
fn equal_priorities_keep_their_registration_order() {
    let bus = EventBus::new();
    record(&bus, "first", EventPriority::Normal);
    record(&bus, "second", EventPriority::Normal);
    record(&bus, "third", EventPriority::Normal);

    let mut event = Trace {
        ran: Vec::new(),
        cancelled: false,
    };
    bus.fire(&mut event);

    assert_eq!(event.ran, ["first", "second", "third"]);
}

/// A cancelled event skips ordinary listeners and still reaches the ones that
/// asked to see it anyway -- a logger or a cleanup step.
#[test]
fn cancelling_stops_ordinary_listeners_but_not_the_ones_that_opted_in() {
    let bus = EventBus::new();
    bus.listen::<Trace, _>(owner("canceler"), EventPriority::Lowest, false, |event| {
        event.ran.push("canceler");
        event.cancelled = true;
    });
    record(&bus, "ordinary", EventPriority::Normal);
    bus.listen::<Trace, _>(owner("watcher"), EventPriority::Monitor, true, |event| {
        event.ran.push("watcher");
    });

    let mut event = Trace {
        ran: Vec::new(),
        cancelled: false,
    };
    bus.fire(&mut event);

    assert_eq!(event.ran, ["canceler", "watcher"]);
}

/// Unloading one plugin must not take another's listeners with it.
#[test]
fn forgetting_one_owner_leaves_the_others_registered() {
    let bus = EventBus::new();
    record(&bus, "keeper", EventPriority::Normal);
    record(&bus, "leaver", EventPriority::Normal);

    bus.forget(&owner("leaver"));

    let mut event = Trace {
        ran: Vec::new(),
        cancelled: false,
    };
    bus.fire(&mut event);

    assert_eq!(event.ran, ["keeper"]);
    assert_eq!(bus.listener_count::<Trace>(), 1);
}

/// A listener that registers another listener while the event is being
/// dispatched must not deadlock.
///
/// This is why listener lists are replaced rather than mutated. Plugins do
/// this -- a join listener that installs a per-player handler is ordinary --
/// and a bus that held its lock across dispatch would hang the server the
/// first time one did, on a code path nothing else would ever exercise.
#[test]
fn a_listener_may_register_another_while_the_event_is_running() {
    let bus = Arc::new(EventBus::new());
    let inner = Arc::clone(&bus);
    bus.listen::<Trace, _>(
        owner("installer"),
        EventPriority::Lowest,
        false,
        move |event| {
            event.ran.push("installer");
            inner.on::<Other, _>(owner("installed"), |_| {});
        },
    );

    let mut event = Trace {
        ran: Vec::new(),
        cancelled: false,
    };
    bus.fire(&mut event);

    assert_eq!(event.ran, ["installer"]);
    assert_eq!(bus.listener_count::<Other>(), 1);
}

/// Firing one event must not reach another type's listeners.
///
/// The bus keys on a `DowncastTypeKey` rather than on `TypeId`, and a bug in
/// that lookup would show up as one plugin's handler being handed another
/// plugin's event -- which the downcast would then refuse, silently, leaving a
/// listener that simply never runs.
#[test]
fn one_event_type_never_reaches_another_types_listeners() {
    let bus = EventBus::new();
    let other_ran = Arc::new(SyncMutex::new(false));
    let flag = Arc::clone(&other_ran);
    bus.on::<Other, _>(owner("other"), move |_| *flag.lock() = true);
    record(&bus, "trace", EventPriority::Normal);

    let mut event = Trace {
        ran: Vec::new(),
        cancelled: false,
    };
    bus.fire(&mut event);

    assert_eq!(event.ran, ["trace"]);
    assert!(
        !*other_ran.lock(),
        "the other type's listener should not have run"
    );
}

/// Firing an event nobody listens for is not an error.
#[test]
fn firing_with_no_listeners_does_nothing() {
    let bus = EventBus::new();

    let mut event = Trace {
        ran: Vec::new(),
        cancelled: false,
    };
    bus.fire(&mut event);

    assert!(event.ran.is_empty());
}
