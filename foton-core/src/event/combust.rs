//! An entity about to be set on fire.

use foton_utils::BlockPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use super::Event;

/// What set the entity on fire, as Bukkit splits its combust events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combuster {
    /// Nothing in particular: `EntityCombustEvent` itself.
    Nothing,
    /// An entity: `EntityCombustByEntityEvent`.
    Entity(Uuid),
    /// A block, possibly one nobody recorded: `EntityCombustByBlockEvent`.
    Block(Option<BlockPos>),
}

/// An entity is about to catch fire for a number of seconds.
///
/// Paper parity: `EntityCombustEvent` and its two subclasses. A listener may
/// cancel the ignition or change how long it lasts.
pub struct EntityCombustEvent {
    entity: Uuid,
    combuster: Combuster,
    seconds: f32,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityCombustEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_combust");
}

impl Event for EntityCombustEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl EntityCombustEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, combuster: Combuster, seconds: f32) -> Self {
        Self {
            entity,
            combuster,
            seconds,
            cancelled: false,
        }
    }

    /// Who is catching fire.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }

    /// What set them on fire.
    #[must_use]
    pub const fn combuster(&self) -> Combuster {
        self.combuster
    }

    /// How long they will burn, in seconds.
    #[must_use]
    pub const fn seconds(&self) -> f32 {
        self.seconds
    }

    /// Changes how long they will burn.
    pub const fn set_seconds(&mut self, seconds: f32) {
        self.seconds = seconds;
    }

    /// Stops the ignition, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
