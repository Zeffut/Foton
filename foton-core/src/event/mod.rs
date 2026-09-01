//! Events: what happened, and who gets to hear about it before it takes effect.
//!
//! Nothing in vanilla corresponds to this. It exists because the server has no
//! way to let anything outside `foton-core` observe or veto a state change, and
//! every extension story — plugins, scripting, an audit log, an anti-cheat hook
//! — starts by needing one.
//!
//! The shape is taken from `org.bukkit.event` on purpose, and only that far.
//! Priorities, cancellation and the monitor contract are borrowed because
//! `design/plugin-compatibility.md` measured which events a corpus of real
//! plugins needs, and those plugins were written against those semantics: a bus
//! that ordered listeners differently would run them in an order their authors
//! never intended. Everything else about Bukkit's bus — its reflection, its
//! annotations, its handler lists — is not copied, because Foton is not obliged
//! to inherit twelve years of another project's compromises to be compatible
//! with them.
//!
//! Unlike the block and item registries, this is not frozen after startup.
//! Those hold game data that has to be stable once the world is running; this
//! holds subscriptions, and a plugin being enabled or disabled while the server
//! runs is ordinary.

use std::sync::Arc;

use foton_utils::Identifier;
use foton_utils::downcast::{Downcast as _, DowncastType, DowncastTypeKey, ErasedType};
use foton_utils::locks::SyncRwLock;
use rustc_hash::FxHashMap;

pub mod block;
pub mod command;
pub mod player;
pub mod inventory;
pub mod entity;
pub mod server;

pub use block::{BlockBreakEvent, BlockPlaceEvent};
pub use command::CommandEvent;
pub use player::{
    PlayerChatEvent, PlayerCustomPayloadEvent, PlayerInteractEvent, PlayerJoinEvent,
    PlayerLoginEvent, PlayerMoveEvent, PlayerQuitEvent,
};
pub use inventory::InventoryClickEvent;
pub use entity::EntityDamageByEntityEvent;
pub use server::ServerTickEvent;

/// Something that happened, which a listener may observe and possibly stop.
///
/// Events are identified by their [`DowncastTypeKey`] rather than by `TypeId`.
/// `AGENTS.md` requires it for anything a plugin can extend: `TypeId` is sound
/// within one linked build and meaningless across two, and the whole point of
/// this bus is that something outside this build will use it.
pub trait Event: DowncastType + Send + Sync {
    /// Whether this event has already been stopped.
    ///
    /// An event that cannot be stopped always answers `false`. That is why this
    /// lives here with a default rather than in a separate trait: dispatch has
    /// to ask every event the same question, and a bus that could only ask
    /// *some* of them would need two dispatch paths for no gain.
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// When a listener runs, relative to the others listening for the same event.
///
/// The order is `org.bukkit.event.EventPriority`'s, including its one
/// counter-intuitive part: `Lowest` runs *first* and `Highest` runs last, so
/// that a listener at `Highest` gets the final word on the outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPriority {
    /// Runs first, and has the least say in the result.
    Lowest,
    /// Runs early.
    Low,
    /// The default.
    #[default]
    Normal,
    /// Runs late.
    High,
    /// Runs last among listeners that may still change the outcome.
    Highest,
    /// Runs after everything, to observe the decision that was reached.
    ///
    /// A listener here must not change the outcome. Nothing enforces that —
    /// nothing in Bukkit does either — but a listener that ignores it is
    /// invisible to every other listener, which is its own punishment.
    Monitor,
}

/// A listener's erased body: it downcasts back to its own event and runs.
type Handler = Arc<dyn Fn(&mut dyn ErasedType) + Send + Sync>;

struct Registration {
    priority: EventPriority,
    /// Whether this listener still runs once something has stopped the event.
    ignores_cancelled: bool,
    /// Who registered it, so that unloading them takes their listeners with it.
    owner: Identifier,
    handler: Handler,
}

/// Who is listening for what.
///
/// Listener lists are held behind an [`Arc`] and replaced rather than mutated,
/// so dispatch clones one pointer and releases the lock before calling anyone.
/// A listener that registers another listener — which plugins do — would
/// otherwise deadlock on a lock it is already inside.
#[derive(Default)]
pub struct EventBus {
    listeners: SyncRwLock<FxHashMap<DowncastTypeKey, Arc<Vec<Registration>>>>,
}

impl EventBus {
    /// Creates an empty bus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a listener for one event type.
    ///
    /// `ignores_cancelled` is Bukkit's flag inverted into the affirmative: a
    /// listener that passes `true` still runs after something has stopped the
    /// event, which is what a logger or a cleanup step wants and what an
    /// ordinary listener does not.
    pub fn listen<E, F>(
        &self,
        owner: Identifier,
        priority: EventPriority,
        ignores_cancelled: bool,
        handler: F,
    ) where
        E: Event,
        F: Fn(&mut E) + Send + Sync + 'static,
    {
        let erased: Handler = Arc::new(move |event: &mut dyn ErasedType| {
            // The key was matched before dispatch, so this only fails if two
            // types share a key -- which `DowncastType` makes an unsafe promise
            // not to do. Silently skipping beats panicking in a listener loop.
            if let Some(typed) = event.downcast_mut::<E>() {
                handler(typed);
            }
        });

        let mut listeners = self.listeners.write();
        let existing = listeners
            .get(&E::TYPE_KEY)
            .map_or(&[][..], |list| list.as_slice());
        let mut next = Vec::with_capacity(existing.len() + 1);
        next.extend(existing.iter().map(Registration::clone_shallow));
        next.push(Registration {
            priority,
            ignores_cancelled,
            owner,
            handler: erased,
        });
        // A stable sort keeps registration order within one priority, which is
        // the only tie-break a listener can reason about.
        next.sort_by_key(|registration| registration.priority);
        listeners.insert(E::TYPE_KEY, Arc::new(next));
    }

    /// Registers a listener that stops running once the event is cancelled.
    ///
    /// The common case, spelled shorter so the common case reads shorter.
    pub fn on<E, F>(&self, owner: Identifier, handler: F)
    where
        E: Event,
        F: Fn(&mut E) + Send + Sync + 'static,
    {
        self.listen::<E, F>(owner, EventPriority::Normal, false, handler);
    }

    /// Drops every listener one owner registered.
    ///
    /// A plugin that is disabled must stop being called, and it cannot be
    /// trusted to have kept a handle to each of its own registrations.
    pub fn forget(&self, owner: &Identifier) {
        let mut listeners = self.listeners.write();
        listeners.retain(|_, list| {
            if !list.iter().any(|registration| registration.owner == *owner) {
                return true;
            }
            let kept: Vec<Registration> = list
                .iter()
                .filter(|registration| registration.owner != *owner)
                .map(Registration::clone_shallow)
                .collect();
            if kept.is_empty() {
                return false;
            }
            *list = Arc::new(kept);
            true
        });
    }

    /// Runs every listener for this event, in priority order.
    ///
    /// The event is handed out mutably: a listener changes the outcome by
    /// changing the event, which is the only channel it gets.
    pub fn fire<E: Event>(&self, event: &mut E) {
        let Some(registrations) = self.listeners.read().get(&E::TYPE_KEY).map(Arc::clone) else {
            return;
        };

        for registration in registrations.iter() {
            if event.is_cancelled() && !registration.ignores_cancelled {
                continue;
            }
            (registration.handler)(event);
        }
    }

    /// How many listeners one event type has. For diagnostics and tests.
    #[must_use]
    pub fn listener_count<E: Event>(&self) -> usize {
        self.listeners
            .read()
            .get(&E::TYPE_KEY)
            .map_or(0, |list| list.len())
    }
}

impl Registration {
    /// Copies the registration, sharing the handler rather than the closure.
    fn clone_shallow(&self) -> Self {
        Self {
            priority: self.priority,
            ignores_cancelled: self.ignores_cancelled,
            owner: self.owner.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

#[cfg(test)]
mod tests;
