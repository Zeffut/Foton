//! Boss bars.
//!
//! Vanilla parity: `net.minecraft.world.BossEvent` and
//! `net.minecraft.server.level.ServerBossEvent`. Vanilla makes `BossEvent` the
//! abstract base a `ServerBossEvent` extends, and shares it with the client;
//! Foton is server-only and has no inheritance, so [`BossEvent`] is the plain
//! state and [`ServerBossEvent`] owns it behind a lock and broadcasts every
//! change.
//!
//! The part that is easy to get wrong is the player set. A bar is not sent to
//! everyone in the world: each viewer is added when they start tracking the
//! boss and removed when they stop, so a bar never lingers on the screen of a
//! client that walked away. [`ServerBossEvent::add_player`] and
//! [`ServerBossEvent::remove_player`] are driven from
//! [`Entity::start_seen_by_player`](crate::entity::Entity::start_seen_by_player)
//! and [`Entity::stop_seen_by_player`](crate::entity::Entity::stop_seen_by_player).

pub mod custom;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use foton_protocol::packets::game::{BossBarColor, BossBarOverlay, BossBarProperties, CBossEvent};
use foton_utils::locks::SyncMutex;
use rustc_hash::FxHashMap;
use text_components::TextComponent;
use uuid::Uuid;

use crate::entity::Entity as _;
use crate::player::Player;

/// The state one boss bar carries.
///
/// Vanilla parity: `BossEvent`.
#[derive(Debug, Clone)]
pub struct BossEvent {
    id: Uuid,
    name: TextComponent,
    progress: f32,
    color: BossBarColor,
    overlay: BossBarOverlay,
    properties: BossBarProperties,
}

impl BossEvent {
    /// Creates a full bar.
    ///
    /// Vanilla parity: the `BossEvent` constructor, which starts at full
    /// progress whatever the boss's health is.
    #[must_use]
    pub const fn new(
        id: Uuid,
        name: TextComponent,
        color: BossBarColor,
        overlay: BossBarOverlay,
    ) -> Self {
        Self {
            id,
            name,
            progress: 1.0,
            color,
            overlay,
            properties: BossBarProperties {
                darken_screen: false,
                play_boss_music: false,
                create_world_fog: false,
            },
        }
    }

    /// Returns the id the client keys this bar by.
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the bar's title.
    #[must_use]
    pub const fn name(&self) -> &TextComponent {
        &self.name
    }

    /// Returns how full the bar is drawn, from `0.0` to `1.0`.
    #[must_use]
    pub const fn progress(&self) -> f32 {
        self.progress
    }

    /// Returns the bar's color.
    #[must_use]
    pub const fn color(&self) -> BossBarColor {
        self.color
    }

    /// Returns the bar's segmentation.
    #[must_use]
    pub const fn overlay(&self) -> BossBarOverlay {
        self.overlay
    }

    /// Returns the screen darkening, boss music and fog flags.
    #[must_use]
    pub const fn properties(&self) -> BossBarProperties {
        self.properties
    }

    /// Builds the packet that shows this bar on a client that lacks it.
    ///
    /// Vanilla parity: `ClientboundBossEventPacket.createAddPacket`.
    #[must_use]
    pub fn add_packet(&self) -> CBossEvent {
        CBossEvent::add(
            self.id,
            self.name.clone(),
            self.progress,
            self.color,
            self.overlay,
            self.properties,
        )
    }
}

/// The mutable half of a [`ServerBossEvent`], behind one lock.
#[derive(Debug)]
struct ServerBossEventState {
    event: BossEvent,
    /// Viewers, keyed by UUID so a disconnect can drop one without an `Arc`.
    ///
    /// Vanilla holds `Set<ServerPlayer>` strongly. Foton holds `Weak` handles
    /// so a bar that outlives a viewer -- a bug elsewhere, but a cheap one to
    /// survive -- cannot keep the player object alive.
    players: FxHashMap<Uuid, Weak<Player>>,
    visible: bool,
}

/// A boss bar the server owns and pushes to the clients watching it.
///
/// Vanilla parity: `ServerBossEvent`.
#[derive(Debug)]
pub struct ServerBossEvent {
    id: Uuid,
    state: SyncMutex<ServerBossEventState>,
    /// Set by every change, for owners that persist the bar.
    ///
    /// Vanilla parity: `ServerBossEvent.setDirty`, an empty hook that
    /// `CustomBossEvent` overrides with a callback into its `SavedData`. A flag
    /// the owner drains avoids the reference cycle that callback would need.
    dirty: AtomicBool,
}

impl ServerBossEvent {
    /// Creates a visible, full bar with no viewers.
    #[must_use]
    pub fn new(
        id: Uuid,
        name: TextComponent,
        color: BossBarColor,
        overlay: BossBarOverlay,
    ) -> Self {
        Self {
            id,
            state: SyncMutex::new(ServerBossEventState {
                event: BossEvent::new(id, name, color, overlay),
                players: FxHashMap::default(),
                visible: true,
            }),
            dirty: AtomicBool::new(false),
        }
    }

    /// Creates a bar under a fresh random id.
    ///
    /// Vanilla parity: the `Mth.createInsecureUUID(this.random)` every boss
    /// passes to its `ServerBossEvent`. That helper builds exactly a version-4
    /// variant-2 UUID, which is what [`Uuid::new_v4`] produces; the bar id is
    /// never compared against anything vanilla derives, so the source of
    /// randomness is not observable.
    #[must_use]
    pub fn with_random_id(
        name: TextComponent,
        color: BossBarColor,
        overlay: BossBarOverlay,
    ) -> Self {
        Self::new(Uuid::new_v4(), name, color, overlay)
    }

    /// Returns the id the client keys this bar by.
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// Returns a copy of the bar's current state.
    #[must_use]
    pub fn snapshot(&self) -> BossEvent {
        self.state.lock().event.clone()
    }

    /// Returns the bar's title.
    #[must_use]
    pub fn name(&self) -> TextComponent {
        self.state.lock().event.name.clone()
    }

    /// Returns how full the bar is drawn.
    #[must_use]
    pub fn progress(&self) -> f32 {
        self.state.lock().event.progress
    }

    /// Returns the bar's color.
    #[must_use]
    pub fn color(&self) -> BossBarColor {
        self.state.lock().event.color
    }

    /// Returns the bar's segmentation.
    #[must_use]
    pub fn overlay(&self) -> BossBarOverlay {
        self.state.lock().event.overlay
    }

    /// Returns the screen darkening, boss music and fog flags.
    #[must_use]
    pub fn properties(&self) -> BossBarProperties {
        self.state.lock().event.properties
    }

    /// Returns whether the bar is being shown at all.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.state.lock().visible
    }

    /// Returns the viewers that are still connected.
    #[must_use]
    pub fn players(&self) -> Vec<Arc<Player>> {
        let mut state = self.state.lock();
        Self::live_players(&mut state)
    }

    /// Returns whether this bar has any viewer left.
    #[must_use]
    pub fn has_players(&self) -> bool {
        !self.players().is_empty()
    }

    /// Vanilla parity: `ServerBossEvent.setProgress`.
    #[expect(
        clippy::float_cmp,
        reason = "vanilla's `progress != this.progress` is a change guard, not a \
                  numeric comparison: a tolerance here would swallow the small \
                  steps of a 220-tick countdown"
    )]
    pub fn set_progress(&self, progress: f32) {
        self.update(
            |event| {
                if event.progress == progress {
                    return false;
                }
                event.progress = progress;
                true
            },
            |event| CBossEvent::update_progress(event.id, event.progress),
        );
    }

    /// Vanilla parity: `ServerBossEvent.setName`.
    pub fn set_name(&self, name: TextComponent) {
        self.update(
            |event| {
                if event.name == name {
                    return false;
                }
                event.name = name.clone();
                true
            },
            |event| CBossEvent::update_name(event.id, event.name.clone()),
        );
    }

    /// Vanilla parity: `ServerBossEvent.setColor`.
    pub fn set_color(&self, color: BossBarColor) {
        self.update(
            |event| {
                if event.color == color {
                    return false;
                }
                event.color = color;
                true
            },
            |event| CBossEvent::update_style(event.id, event.color, event.overlay),
        );
    }

    /// Vanilla parity: `ServerBossEvent.setOverlay`.
    pub fn set_overlay(&self, overlay: BossBarOverlay) {
        self.update(
            |event| {
                if event.overlay == overlay {
                    return false;
                }
                event.overlay = overlay;
                true
            },
            |event| CBossEvent::update_style(event.id, event.color, event.overlay),
        );
    }

    /// Vanilla parity: `ServerBossEvent.setDarkenScreen`.
    pub fn set_darken_screen(&self, darken_screen: bool) {
        self.set_properties(|properties| properties.darken_screen = darken_screen);
    }

    /// Vanilla parity: `ServerBossEvent.setPlayBossMusic`.
    pub fn set_play_boss_music(&self, play_boss_music: bool) {
        self.set_properties(|properties| properties.play_boss_music = play_boss_music);
    }

    /// Vanilla parity: `ServerBossEvent.setCreateWorldFog`.
    pub fn set_create_world_fog(&self, create_world_fog: bool) {
        self.set_properties(|properties| properties.create_world_fog = create_world_fog);
    }

    fn set_properties(&self, change: impl FnOnce(&mut BossBarProperties)) {
        self.update(
            |event| {
                let before = event.properties;
                change(&mut event.properties);
                before != event.properties
            },
            |event| CBossEvent::update_properties(event.id, event.properties),
        );
    }

    /// Shows the bar to one more player.
    ///
    /// Vanilla parity: `ServerBossEvent.addPlayer`.
    pub fn add_player(&self, player: &Arc<Player>) {
        let packet = {
            let mut state = self.state.lock();
            let previous = state.players.insert(player.uuid(), Arc::downgrade(player));
            // A stale entry left by a reconnect is not a viewer, so it counts
            // as a fresh add rather than a duplicate.
            let newly_added = previous.is_none_or(|weak| weak.upgrade().is_none());
            if !newly_added {
                return;
            }
            self.dirty.store(true, Ordering::Relaxed);
            if !state.visible {
                return;
            }
            state.event.add_packet()
        };
        player.send_packet(packet);
    }

    /// Takes the bar off one player's screen.
    ///
    /// Vanilla parity: `ServerBossEvent.removePlayer`.
    pub fn remove_player(&self, player: &Player) {
        let send_remove = {
            let mut state = self.state.lock();
            if state.players.remove(&player.uuid()).is_none() {
                return;
            }
            self.dirty.store(true, Ordering::Relaxed);
            state.visible
        };
        if send_remove {
            player.send_packet(CBossEvent::remove(self.id));
        }
    }

    /// Takes the bar off every screen it is on.
    ///
    /// Vanilla parity: `ServerBossEvent.removeAllPlayers`.
    pub fn remove_all_players(&self) {
        let (recipients, visible) = {
            let mut state = self.state.lock();
            if state.players.is_empty() {
                return;
            }
            let recipients = Self::live_players(&mut state);
            state.players.clear();
            self.dirty.store(true, Ordering::Relaxed);
            (recipients, state.visible)
        };
        if !visible {
            return;
        }
        for player in recipients {
            player.send_packet(CBossEvent::remove(self.id));
        }
    }

    /// Hides or reveals the bar without changing who watches it.
    ///
    /// Vanilla parity: `ServerBossEvent.setVisible`.
    pub fn set_visible(&self, visible: bool) {
        let (recipients, packet) = {
            let mut state = self.state.lock();
            if state.visible == visible {
                return;
            }
            state.visible = visible;
            self.dirty.store(true, Ordering::Relaxed);
            let packet = if visible {
                state.event.add_packet()
            } else {
                CBossEvent::remove(self.id)
            };
            (Self::live_players(&mut state), packet)
        };
        for player in recipients {
            player.send_packet(packet.clone());
        }
    }

    /// Returns whether the bar changed since this was last called, clearing the
    /// flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// Marks the bar as changed for its owner's persistence.
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Applies a change and, when it changed anything, tells every viewer.
    ///
    /// Vanilla parity: the `if (x != this.x) { ...; setDirty(); broadcast(...) }`
    /// shape every `ServerBossEvent` setter repeats.
    fn update(
        &self,
        change: impl FnOnce(&mut BossEvent) -> bool,
        packet: impl FnOnce(&BossEvent) -> CBossEvent,
    ) {
        let broadcast = {
            let mut state = self.state.lock();
            if !change(&mut state.event) {
                return;
            }
            self.dirty.store(true, Ordering::Relaxed);
            if !state.visible {
                return;
            }
            let packet = packet(&state.event);
            (Self::live_players(&mut state), packet)
        };
        let (recipients, packet) = broadcast;
        for player in recipients {
            player.send_packet(packet.clone());
        }
    }

    /// Returns the viewers that are still around, dropping the ones that are
    /// not.
    fn live_players(state: &mut ServerBossEventState) -> Vec<Arc<Player>> {
        let mut players = Vec::with_capacity(state.players.len());
        state.players.retain(|_, weak| {
            let Some(player) = weak.upgrade() else {
                return false;
            };
            players.push(player);
            true
        });
        players
    }
}

#[cfg(test)]
mod tests;
