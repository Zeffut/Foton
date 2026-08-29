//! Boss bars a command owns rather than a boss.
//!
//! Vanilla parity: `CustomBossEvent` and `CustomBossEvents`. A boss's bar
//! lives and dies with the boss, and its progress is whatever fraction of its
//! health is left. A named bar has neither: it is created by `/bossbar add`,
//! it survives a restart, and its fill is a pair of whole numbers a datapack
//! sets -- `value` out of `max` -- because a datapack counting objectives has
//! nothing to divide.
//!
//! Two player sets, and the difference matters. [`ServerBossEvent`] holds the
//! clients the bar is currently on. [`CustomBossEvent`] holds the UUIDs it is
//! *assigned* to, which is what `/bossbar set <id> players` writes and what
//! goes in the save file. A viewer who logs out leaves the first and stays in
//! the second, so the bar is waiting for them when they come back.
//!
//! Vanilla keeps the whole collection in the overworld's data storage and
//! reaches it through the server. Foton's scoreboards and command storage are
//! per domain, and `execute store` addresses all three the same way, so these
//! follow them rather than vanilla's single global.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use foton_protocol::packets::game::{BossBarColor, BossBarOverlay};
use foton_utils::locks::{AsyncMutex, SyncMutex};
use foton_utils::saved_data::names as saved_data_names;
use foton_utils::{Identifier, translations};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use text_components::{Modifier as _, TextComponent, interactivity::HoverEvent};
use uuid::Uuid;

use super::ServerBossEvent;
use crate::entity::Entity as _;
use crate::player::Player;
use crate::server::worlds::WorldMap;
use crate::world::World;

/// The fill a bar starts at.
///
/// Vanilla parity: `CustomBossEvent.DEFAULT_MAX`.
pub const DEFAULT_MAX: i32 = 100;

/// A boss bar addressed by name instead of by the boss behind it.
///
/// Vanilla parity: `CustomBossEvent`.
#[derive(Debug)]
pub struct CustomBossEvent {
    custom_id: Identifier,
    event: ServerBossEvent,
    /// Everyone the bar is assigned to, online or not.
    assigned: SyncMutex<BTreeSet<Uuid>>,
    value: AtomicI32,
    max: AtomicI32,
}

impl CustomBossEvent {
    /// Creates an empty bar under `custom_id`.
    ///
    /// Vanilla parity: the `CustomBossEvent` constructor, which starts at zero
    /// progress rather than the full bar a boss's own starts at.
    #[must_use]
    pub fn new(id: Uuid, custom_id: Identifier, name: TextComponent) -> Self {
        let event = ServerBossEvent::new(id, name, BossBarColor::White, BossBarOverlay::Progress);
        event.set_progress(0.0);
        Self {
            custom_id,
            event,
            assigned: SyncMutex::new(BTreeSet::new()),
            value: AtomicI32::new(0),
            max: AtomicI32::new(DEFAULT_MAX),
        }
    }

    /// The name a command addresses this bar by.
    #[must_use]
    pub const fn custom_id(&self) -> &Identifier {
        &self.custom_id
    }

    /// The bar itself, for the title, color, overlay and visibility a
    /// [`ServerBossEvent`] already owns.
    #[must_use]
    pub const fn event(&self) -> &ServerBossEvent {
        &self.event
    }

    /// Vanilla parity: `CustomBossEvent.value`.
    #[must_use]
    pub fn value(&self) -> i32 {
        self.value.load(Ordering::Relaxed)
    }

    /// Vanilla parity: `CustomBossEvent.max`.
    #[must_use]
    pub fn max(&self) -> i32 {
        self.max.load(Ordering::Relaxed)
    }

    /// Vanilla parity: `CustomBossEvent.setValue`.
    pub fn set_value(&self, value: i32) {
        self.value.store(value, Ordering::Relaxed);
        self.update_progress();
        // Unconditionally, the way vanilla's `setDirty()` is: the fill may
        // round to the same float against a large maximum and the value still
        // has to reach the save file.
        self.event.mark_dirty();
    }

    /// Vanilla parity: `CustomBossEvent.setMax`.
    pub fn set_max(&self, max: i32) {
        self.max.store(max, Ordering::Relaxed);
        self.update_progress();
        self.event.mark_dirty();
    }

    fn update_progress(&self) {
        let progress = self.value() as f32 / self.max() as f32;
        self.event.set_progress(progress.clamp(0.0, 1.0));
    }

    /// Assigns the bar to one more player and shows it to them.
    ///
    /// Vanilla parity: `CustomBossEvent.addPlayer`.
    pub fn add_player(&self, player: &Arc<Player>) {
        self.event.add_player(player);
        if self.assigned.lock().insert(player.uuid()) {
            self.event.mark_dirty();
        }
    }

    /// Takes the bar off one player and unassigns them from it.
    ///
    /// Vanilla parity: `CustomBossEvent.removePlayer`.
    pub fn remove_player(&self, player: &Player) {
        self.event.remove_player(player);
        if self.assigned.lock().remove(&player.uuid()) {
            self.event.mark_dirty();
        }
    }

    /// Vanilla parity: `CustomBossEvent.removeAllPlayers`.
    pub fn remove_all_players(&self) {
        self.event.remove_all_players();
        let emptied = {
            let mut assigned = self.assigned.lock();
            let had_any = !assigned.is_empty();
            assigned.clear();
            had_any
        };
        if emptied {
            self.event.mark_dirty();
        }
    }

    /// The players this bar is assigned to, whether or not they are online.
    #[must_use]
    pub fn assigned_players(&self) -> Vec<Uuid> {
        self.assigned.lock().iter().copied().collect()
    }

    /// Replaces the assignment with exactly `targets`.
    ///
    /// Returns whether anything changed, which is what `/bossbar set players`
    /// reports as a failure when it did not.
    ///
    /// Vanilla parity: `CustomBossEvent.setPlayers`.
    pub fn set_players(&self, targets: &[Arc<Player>]) -> bool {
        let wanted = targets
            .iter()
            .map(|player| player.uuid())
            .collect::<BTreeSet<_>>();
        let current = self.assigned.lock().clone();

        let dropped = current.difference(&wanted).copied().collect::<Vec<_>>();
        for uuid in &dropped {
            // Only a player who is online is on a screen to take the bar off.
            if let Some(player) = self
                .event
                .players()
                .into_iter()
                .find(|player| player.uuid() == *uuid)
            {
                self.event.remove_player(&player);
            }
            self.assigned.lock().remove(uuid);
        }

        let mut added = 0;
        for player in targets {
            if current.contains(&player.uuid()) {
                continue;
            }
            self.add_player(player);
            added += 1;
        }

        let changed = !dropped.is_empty() || added > 0;
        if changed {
            self.event.mark_dirty();
        }
        changed
    }

    /// Puts the bar back on a player who was assigned to it before they left.
    ///
    /// Vanilla parity: `CustomBossEvent.onPlayerConnect`.
    pub fn on_player_connect(&self, player: &Arc<Player>) {
        if self.assigned.lock().contains(&player.uuid()) {
            self.event.add_player(player);
        }
    }

    /// Drops a leaving player from the viewers without unassigning them.
    ///
    /// Vanilla parity: `CustomBossEvent.onPlayerDisconnect`, which calls
    /// `super.removePlayer` on purpose: logging out is not the same as being
    /// taken off the bar, and the assignment has to survive it.
    pub fn on_player_disconnect(&self, player: &Player) {
        self.event.remove_player(player);
    }

    /// The bar's name as a command prints it.
    ///
    /// Vanilla parity: `CustomBossEvent.getDisplayName` -- the title in square
    /// brackets, tinted by the bar's color, carrying its id as both a hover
    /// and a shift-click insertion.
    #[must_use]
    pub fn display_name(&self) -> TextComponent {
        let id = self.custom_id.to_string();
        translations::CHAT_SQUARE_BRACKETS
            .message([self.event.name()])
            .component()
            .color(self.event.color().chat_color())
            .hover_event(HoverEvent::show_text(TextComponent::plain(id.clone())))
            .insertion(id)
    }
}

/// One boss bar as it is stored.
///
/// Vanilla parity: `CustomBossEvent.Packed`. The name travels as component NBT
/// for the same reason an entity's custom name does: a text component is a
/// tree, and its codec is the only thing that round-trips one.
#[derive(Deserialize, Serialize)]
struct PersistentBossBar {
    name: Vec<u8>,
    visible: bool,
    value: i32,
    max: i32,
    color: String,
    overlay: String,
    darken_screen: bool,
    play_boss_music: bool,
    create_world_fog: bool,
    players: Vec<[u8; 16]>,
}

/// Every named bar of one domain, as it is stored.
#[derive(Default, Deserialize, Serialize)]
struct PersistentCustomBossEvents {
    bars: BTreeMap<String, PersistentBossBar>,
}

struct CustomBossEventsSaveSnapshot {
    revision: u64,
    state: PersistentCustomBossEvents,
}

/// The named boss bars of one Foton domain.
///
/// Vanilla parity: `CustomBossEvents`.
pub struct CustomBossEvents {
    bars: SyncMutex<FxHashMap<Identifier, Arc<CustomBossEvent>>>,
    revision: AtomicU64,
    saved_revision: AtomicU64,
}

impl CustomBossEvents {
    /// Creates an empty, clean collection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bars: SyncMutex::new(FxHashMap::default()),
            revision: AtomicU64::new(0),
            saved_revision: AtomicU64::new(0),
        }
    }

    /// Returns the bar `id` names.
    ///
    /// Vanilla parity: `CustomBossEvents.get`.
    #[must_use]
    pub fn get(&self, id: &Identifier) -> Option<Arc<CustomBossEvent>> {
        self.bars.lock().get(id).map(Arc::clone)
    }

    /// Creates a bar under `id`.
    ///
    /// Vanilla parity: `CustomBossEvents.create`. The caller is responsible
    /// for refusing an id that already exists; this replaces it, the way
    /// vanilla's `Map.put` does.
    pub fn create(&self, id: Identifier, name: TextComponent) -> Arc<CustomBossEvent> {
        let bar = Arc::new(CustomBossEvent::new(Uuid::new_v4(), id.clone(), name));
        self.bars.lock().insert(id, Arc::clone(&bar));
        self.revision.fetch_add(1, Ordering::Release);
        bar
    }

    /// Vanilla parity: `CustomBossEvents.remove`.
    pub fn remove(&self, id: &Identifier) -> bool {
        let removed = self.bars.lock().remove(id).is_some();
        if removed {
            self.revision.fetch_add(1, Ordering::Release);
        }
        removed
    }

    /// Every bar id, in stable resource-location order.
    ///
    /// Vanilla iterates a hash map and takes whatever order the JVM gives it.
    /// A command that lists bars should not reshuffle them between runs, so
    /// these are sorted.
    ///
    /// Vanilla parity: `CustomBossEvents.getIds`.
    #[must_use]
    pub fn ids(&self) -> Vec<Identifier> {
        let mut ids = self.bars.lock().keys().cloned().collect::<Vec<_>>();
        ids.sort_by_cached_key(ToString::to_string);
        ids
    }

    /// Every bar, in the same order as [`Self::ids`].
    ///
    /// Vanilla parity: `CustomBossEvents.getEvents`.
    #[must_use]
    pub fn events(&self) -> Vec<Arc<CustomBossEvent>> {
        let mut events = self
            .bars
            .lock()
            .values()
            .map(Arc::clone)
            .collect::<Vec<_>>();
        events.sort_by_cached_key(|bar| bar.custom_id().to_string());
        events
    }

    /// Vanilla parity: `CustomBossEvents.onPlayerConnect`.
    pub fn on_player_connect(&self, player: &Arc<Player>) {
        for bar in self.events() {
            bar.on_player_connect(player);
        }
    }

    /// Vanilla parity: `CustomBossEvents.onPlayerDisconnect`.
    pub fn on_player_disconnect(&self, player: &Player) {
        for bar in self.events() {
            bar.on_player_disconnect(player);
        }
    }

    fn from_persistent(persistent: PersistentCustomBossEvents) -> io::Result<Self> {
        let mut bars = FxHashMap::default();
        for (raw_id, packed) in persistent.bars {
            let id = raw_id.parse::<Identifier>().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid boss bar id '{raw_id}': {error}"),
                )
            })?;
            bars.insert(id.clone(), Arc::new(load_bar(&id, packed)?));
        }
        Ok(Self {
            bars: SyncMutex::new(bars),
            revision: AtomicU64::new(0),
            saved_revision: AtomicU64::new(0),
        })
    }

    /// Folds every bar's own change flag into the collection's revision.
    ///
    /// A bar reports its own changes through [`ServerBossEvent::take_dirty`],
    /// which is Foton's stand-in for the `setDirty` callback vanilla's
    /// `CustomBossEvent` holds into its saved data. Draining them here is what
    /// turns "a color changed" into "this domain needs writing".
    fn absorb_bar_changes(&self) {
        let dirty = self
            .bars
            .lock()
            .values()
            .filter(|bar| bar.event.take_dirty())
            .count();
        if dirty > 0 {
            self.revision.fetch_add(1, Ordering::Release);
        }
    }

    fn pending_save(&self) -> Option<CustomBossEventsSaveSnapshot> {
        self.absorb_bar_changes();
        let revision = self.revision.load(Ordering::Acquire);
        if revision == self.saved_revision.load(Ordering::Acquire) {
            return None;
        }

        let bars = self
            .bars
            .lock()
            .iter()
            .map(|(id, bar)| (id.to_string(), pack_bar(bar)))
            .collect::<BTreeMap<_, _>>();
        Some(CustomBossEventsSaveSnapshot {
            revision,
            state: PersistentCustomBossEvents { bars },
        })
    }

    fn mark_saved(&self, revision: u64) {
        self.saved_revision.fetch_max(revision, Ordering::Release);
    }
}

impl Default for CustomBossEvents {
    fn default() -> Self {
        Self::new()
    }
}

/// Vanilla parity: `CustomBossEvent.pack`.
fn pack_bar(bar: &CustomBossEvent) -> PersistentBossBar {
    let event = bar.event();
    let properties = event.properties();
    let mut root = NbtCompound::new();
    root.insert("Name", event.name().to_codec_nbt());
    let mut name = Vec::new();
    root.write(&mut name);

    PersistentBossBar {
        name,
        visible: event.is_visible(),
        value: bar.value(),
        max: bar.max(),
        color: event.color().serialized_name().to_owned(),
        overlay: event.overlay().serialized_name().to_owned(),
        darken_screen: properties.darken_screen,
        play_boss_music: properties.play_boss_music,
        create_world_fog: properties.create_world_fog,
        players: bar
            .assigned_players()
            .into_iter()
            .map(Uuid::into_bytes)
            .collect(),
    }
}

/// Vanilla parity: `CustomBossEvent.load`.
fn load_bar(id: &Identifier, packed: PersistentBossBar) -> io::Result<CustomBossEvent> {
    let name = read_bar_name(id, &packed.name)?;
    let bar = CustomBossEvent::new(Uuid::new_v4(), id.clone(), name);
    let event = bar.event();

    event.set_visible(packed.visible);
    bar.set_value(packed.value);
    bar.set_max(packed.max);
    event.set_color(bar_enum(
        id,
        "color",
        &packed.color,
        BossBarColor::from_serialized_name(&packed.color),
    )?);
    event.set_overlay(bar_enum(
        id,
        "overlay",
        &packed.overlay,
        BossBarOverlay::from_serialized_name(&packed.overlay),
    )?);
    event.set_darken_screen(packed.darken_screen);
    event.set_play_boss_music(packed.play_boss_music);
    event.set_create_world_fog(packed.create_world_fog);
    {
        let mut assigned = bar.assigned.lock();
        for bytes in packed.players {
            assigned.insert(Uuid::from_bytes(bytes));
        }
    }

    // Everything above went through a setter that marks the bar changed. A bar
    // that has just been read off disk has not changed, and leaving the flag
    // set would rewrite every file on the first autosave.
    let _ = event.take_dirty();
    Ok(bar)
}

fn read_bar_name(id: &Identifier, bytes: &[u8]) -> io::Result<TextComponent> {
    let root = read_borrowed_compound(&mut Cursor::new(bytes)).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid name NBT for boss bar '{id}': {error:?}"),
        )
    })?;
    let root = simdnbt::borrow::NbtCompound::from(&root);
    root.get("Name")
        .and_then(|tag| TextComponent::from_nbt(&tag.to_owned()))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("boss bar '{id}' has no readable name"),
            )
        })
}

fn bar_enum<T>(id: &Identifier, field: &str, raw: &str, parsed: Option<T>) -> io::Result<T> {
    parsed.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("boss bar '{id}' has an unknown {field} '{raw}'"),
        )
    })
}

/// Loaded boss bars keyed by Foton domain.
pub struct DomainCustomBossEvents {
    domains: BTreeMap<String, CustomBossEvents>,
    save_lock: AsyncMutex<()>,
}

impl DomainCustomBossEvents {
    /// Loads one set of boss bars through each domain's default world.
    pub async fn load(worlds: &WorldMap) -> io::Result<Self> {
        let mut names = worlds.domain_names().collect::<Vec<_>>();
        names.sort_unstable();
        let mut domains = BTreeMap::new();
        for domain in names {
            let world = domain_default_world(worlds, domain)?;
            let persistent: PersistentCustomBossEvents = world
                .saved_data
                .load_or_default(saved_data_names::CUSTOM_BOSS_EVENTS)
                .await
                .map_err(|error| boss_bar_io_error(domain, error))?;
            let events = CustomBossEvents::from_persistent(persistent)
                .map_err(|error| boss_bar_io_error(domain, error))?;
            domains.insert(domain.to_owned(), events);
        }
        Ok(Self {
            domains,
            save_lock: AsyncMutex::new(()),
        })
    }

    /// Returns the boss bars of a domain.
    #[must_use]
    pub fn get(&self, domain: &str) -> Option<&CustomBossEvents> {
        self.domains.get(domain)
    }

    /// Runs `visitor` over every domain's bars.
    ///
    /// Player join and leave are server-wide events, and a player is only ever
    /// in one domain at a time, but a bar can be assigned to somebody who is
    /// not in its own; vanilla has one global collection and no such question
    /// to answer.
    pub fn for_each(&self, visitor: impl Fn(&CustomBossEvents)) {
        for events in self.domains.values() {
            visitor(events);
        }
    }

    /// Saves every dirty domain's bars and returns the number written.
    pub async fn save(&self, worlds: &WorldMap) -> io::Result<usize> {
        let _save_guard = self.save_lock.lock().await;
        let mut saved = 0;
        for (domain, events) in &self.domains {
            let Some(snapshot) = events.pending_save() else {
                continue;
            };
            let world = domain_default_world(worlds, domain)?;
            world
                .saved_data
                .save(saved_data_names::CUSTOM_BOSS_EVENTS, &snapshot.state)
                .await
                .map_err(|error| boss_bar_io_error(domain, error))?;
            events.mark_saved(snapshot.revision);
            saved += 1;
        }
        Ok(saved)
    }
}

fn domain_default_world<'a>(worlds: &'a WorldMap, domain: &str) -> io::Result<&'a World> {
    worlds
        .default_world(domain)
        .map(AsRef::as_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("domain '{domain}' has no loaded default world"),
            )
        })
}

fn boss_bar_io_error(domain: &str, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("boss bar I/O failed for domain '{domain}': {error}"),
    )
}

#[cfg(test)]
mod tests;
