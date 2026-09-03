//! The jukebox block entity.
//!
//! Vanilla parity: `JukeboxBlockEntity` and `JukeboxSongPlayer`. It holds one
//! disc and counts how long it has been playing, which is all the server needs
//! to do: the music itself is a level event the client plays, so the server's
//! job is to say when it starts, when it stops, and to keep a comparator and a
//! sculk sensor informed in between.

use std::mem;
use std::sync::{Arc, Weak};

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use foton_registry::data_components::vanilla_components;
use foton_registry::item_stack::ItemStack;
use foton_registry::jukebox_song::JukeboxSong;
use foton_registry::registry::holder::RegistryHolder;
use foton_registry::{
    REGISTRY, RegistryExt as _, level_events, vanilla_block_entity_types, vanilla_game_events,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Whether the jukebox is holding a disc.
const HAS_RECORD: &BoolProperty = &BlockStateProperties::HAS_RECORD;

/// How often a playing jukebox tells the world it is playing.
///
/// Vanilla parity: `JukeboxSongPlayer.PLAY_EVENT_INTERVAL_TICKS`. This is what
/// a sculk sensor and an allay hear.
const PLAY_EVENT_INTERVAL_TICKS: i64 = 20;

/// Grace period after a song's length before it counts as over.
///
/// Vanilla parity: the `+ 20` of `JukeboxSong.hasFinished`.
const FINISH_GRACE_TICKS: i64 = 20;

/// The jukebox.
pub struct JukeboxBlockEntity {
    base: Arc<BlockEntityBase>,
    state: SyncMutex<JukeboxState>,
}

// SAFETY: This key is owned by Foton and uniquely identifies the block entity.
unsafe impl DowncastType for JukeboxBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/jukebox");
}

struct JukeboxState {
    /// The disc inside, empty when there is none.
    item: ItemStack,
    /// What is playing, if anything.
    song: Option<RegistryHolder<JukeboxSong>>,
    /// Vanilla parity: `JukeboxSongPlayer.ticksSinceSongStarted`.
    ticks_since_song_started: i64,
}

impl JukeboxBlockEntity {
    /// Creates a jukebox block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: Arc::new(BlockEntityBase::new(
                &vanilla_block_entity_types::JUKEBOX,
                level,
                pos,
                state,
            )),
            state: SyncMutex::new(JukeboxState {
                item: ItemStack::empty(),
                song: None,
                ticks_since_song_started: 0,
            }),
        }
    }

    /// Returns whether a song is playing.
    ///
    /// Vanilla parity: `JukeboxSongPlayer.isPlaying`, which is the jukebox's
    /// redstone signal: full while the music runs, nothing when it stops.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.state.lock().song.is_some()
    }
    /// Returns the record currently held by the jukebox.
    #[must_use]
    pub fn item(&self) -> ItemStack {
        self.state.lock().item.clone()
    }

    /// Returns what a comparator reads off it.
    ///
    /// Vanilla parity: `JukeboxBlockEntity.getComparatorOutput`, which is a
    /// per-disc number rather than a fullness -- it is how a player tells one
    /// record from another with redstone.
    #[must_use]
    pub fn comparator_output(&self) -> i32 {
        let state = self.state.lock();
        song_of(&state.item).map_or(0, |song| song.value().comparator_output)
    }

    /// Puts a disc in and starts it.
    ///
    /// Vanilla parity: `JukeboxBlockEntity.setTheItem`.
    pub fn insert(&self, world: &Arc<World>, item: ItemStack) {
        let song = song_of(&item);
        {
            let mut state = self.state.lock();
            state.item = item;
            state.song = None;
            state.ticks_since_song_started = 0;
        }

        self.set_has_record(world, true);

        let Some(song) = song else {
            return;
        };
        if let Some(id) = song_id(&song) {
            world.level_event(
                level_events::SOUND_PLAY_JUKEBOX_SONG,
                self.base.pos(),
                id,
                None,
            );
        }
        self.state.lock().song = Some(song);
        self.on_song_changed(world);
    }

    /// Throws the disc back out.
    ///
    /// Vanilla parity: `JukeboxBlockEntity.popOutTheItem`.
    pub fn pop_out_item(&self, world: &Arc<World>) {
        let item = {
            let mut state = self.state.lock();
            if state.item.is_empty() {
                return;
            }
            mem::replace(&mut state.item, ItemStack::empty())
        };

        self.stop(world);
        self.set_has_record(world, false);
        world.drop_item_stack(self.base.pos().above(), item);
        self.on_song_changed(world);
    }

    /// Stops whatever is playing.
    ///
    /// Vanilla parity: `JukeboxSongPlayer.stop`.
    fn stop(&self, world: &Arc<World>) {
        {
            let mut state = self.state.lock();
            if state.song.is_none() {
                return;
            }
            state.song = None;
            state.ticks_since_song_started = 0;
        }

        let pos = self.base.pos();
        world.game_event(
            &vanilla_game_events::JUKEBOX_STOP_PLAY,
            pos,
            &GameEventContext::new(None, None),
        );
        world.level_event(level_events::SOUND_STOP_JUKEBOX_SONG, pos, 0, None);
        self.on_song_changed(world);
    }

    /// Flips the `has_record` property, which is what makes the model change.
    fn set_has_record(&self, world: &Arc<World>, has_record: bool) {
        let pos = self.base.pos();
        let state = world.get_block_state(pos);
        if state.try_get_value(HAS_RECORD) == Some(has_record) {
            return;
        }
        world.set_block(
            pos,
            state.set_value(HAS_RECORD, has_record),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(None, None),
        );
    }

    /// Tells the world the song changed.
    ///
    /// Vanilla parity: `JukeboxBlockEntity.onSongChanged`, which is both halves
    /// -- the neighbor update that a comparator reads, and the `setChanged`
    /// that marks the chunk for saving. Without the second the disc inside can
    /// be lost when the world is written out.
    fn on_song_changed(&self, world: &Arc<World>) {
        let pos = self.base.pos();
        world.update_neighbors_at(pos, world.get_block_state(pos).get_block());
        self.base.set_changed();
    }
}

/// Returns the song a stack carries, if it is a disc.
fn song_of(item: &ItemStack) -> Option<RegistryHolder<JukeboxSong>> {
    item.get(vanilla_components::JUKEBOX_PLAYABLE)
        .map(|playable| playable.song().clone())
}

/// Returns a song's numeric registry id, which is what the level event carries.
///
/// A song written directly into an item rather than referenced from the
/// registry has no id, so it cannot be named to the client and does not play.
/// Vanilla throws in that case; refusing quietly is the friendlier half of the
/// same rule.
fn song_id(song: &RegistryHolder<JukeboxSong>) -> Option<i32> {
    match song {
        RegistryHolder::Reference(song) => REGISTRY
            .jukebox_songs
            .id_from_key(&song.key)
            .and_then(|id| i32::try_from(id).ok()),
        RegistryHolder::Direct(_) => None,
    }
}

impl BlockEntity for JukeboxBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: `JukeboxSongPlayer.tick`.
    fn tick(&self, world: &Arc<World>) {
        let (finished, should_announce) = {
            let mut state = self.state.lock();
            let Some(song) = state.song.as_ref() else {
                return;
            };
            let length = (song.value().length_in_seconds * 20.0).ceil() as i64 + FINISH_GRACE_TICKS;
            let elapsed = state.ticks_since_song_started;
            if elapsed >= length {
                (true, false)
            } else {
                state.ticks_since_song_started += 1;
                (false, elapsed % PLAY_EVENT_INTERVAL_TICKS == 0)
            }
        };

        if finished {
            self.stop(world);
        } else if should_announce {
            world.game_event(
                &vanilla_game_events::JUKEBOX_PLAY,
                self.base.pos(),
                &GameEventContext::new(None, None),
            );
        }
    }

    /// Vanilla parity: `JukeboxBlockEntity.preRemoveSideEffects`, which is why
    /// breaking a jukebox gives the disc back rather than eating it.
    fn pre_remove_side_effects(&self, _pos: BlockPos, _state: BlockStateId) {
        let Some(world) = self.base.level() else {
            return;
        };
        self.pop_out_item(&world);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        if !state.item.is_empty()
            && let NbtTag::Compound(item_nbt) = state.item.clone().to_nbt_tag()
        {
            nbt.insert("RecordItem", item_nbt);
        }
        if state.song.is_some() {
            nbt.insert(
                "ticks_since_song_started",
                NbtTag::Long(state.ticks_since_song_started),
            );
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let item = view
            .compound("RecordItem")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);

        let song = song_of(&item);
        let ticks = view.long("ticks_since_song_started").unwrap_or(0);

        let mut state = self.state.lock();
        state.item = item;
        // A song that had already finished when the world was saved is not
        // resumed, matching `setSongWithoutPlaying`.
        state.song = song.filter(|song| {
            let length = (song.value().length_in_seconds * 20.0).ceil() as i64 + FINISH_GRACE_TICKS;
            ticks < length
        });
        state.ticks_since_song_started = if state.song.is_some() { ticks } else { 0 };
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }
}
