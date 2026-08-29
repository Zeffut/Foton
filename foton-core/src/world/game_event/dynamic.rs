//! A game-event listener that follows the entity carrying it.
//!
//! Vanilla parity: `net.minecraft.world.level.gameevent.DynamicGameEventListener`.
//! A block's listener sits in one chunk section forever, so the chunk registry
//! can own it; a mob's listener has to move between sections as the mob does,
//! and this is the wrapper that re-registers it when it crosses a boundary.

use std::sync::Arc;

use foton_utils::SectionPos;
use foton_utils::locks::SyncMutex;

use super::listener::SharedGameEventListener;
use crate::world::World;

/// Which half of the listener's life this call is.
///
/// Vanilla parity: the three method references `ServerLevel` passes as its
/// `BiConsumer<DynamicGameEventListener<?>, ServerLevel>` -- `add`, `remove`
/// and `move`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicListenerAction {
    /// The entity joined the world.
    Add,
    /// The entity left it.
    Remove,
    /// The entity changed chunk section.
    Move,
}

/// Keeps one listener registered in whichever section its source is in.
pub struct DynamicGameEventListener {
    listener: SharedGameEventListener,
    last_section: SyncMutex<Option<SectionPos>>,
}

impl DynamicGameEventListener {
    /// Wraps `listener` so a moving entity can carry it.
    #[must_use]
    pub fn new(listener: SharedGameEventListener) -> Self {
        Self {
            listener,
            last_section: SyncMutex::new(None),
        }
    }

    /// Applies one of the three lifecycle actions.
    ///
    /// Vanilla parity: `DynamicGameEventListener.add`, which is just `move`,
    /// plus `remove` and `move` themselves.
    pub fn apply(&self, action: DynamicListenerAction, world: &Arc<World>) {
        match action {
            DynamicListenerAction::Add | DynamicListenerAction::Move => self.move_to_current(world),
            DynamicListenerAction::Remove => self.remove(world),
        }
    }

    /// Vanilla parity: `DynamicGameEventListener.move`.
    fn move_to_current(&self, world: &Arc<World>) {
        let Some(position) = self.listener.listener_pos() else {
            return;
        };
        let current_section = SectionPos::from_entity_pos(position);

        let mut last_section = self.last_section.lock();
        if *last_section == Some(current_section) {
            return;
        }
        if let Some(previous) = *last_section {
            world.unregister_game_event_listener(previous, &self.listener);
        }
        *last_section = Some(current_section);
        world.register_game_event_listener(current_section, Arc::clone(&self.listener));
    }

    /// Vanilla parity: `DynamicGameEventListener.remove`.
    fn remove(&self, world: &Arc<World>) {
        let mut last_section = self.last_section.lock();
        if let Some(previous) = last_section.take() {
            world.unregister_game_event_listener(previous, &self.listener);
        }
    }
}
